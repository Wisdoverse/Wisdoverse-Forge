//! Wisdoverse Forge Sidecar — lightweight agent container companion.
//!
//! Runs inside each agent container and communicates with the platform via NATS.
//! Provides event publishing (with HMAC auth), command handling (request-reply),
//! heartbeat emission, and a file-based WAL for offline resilience.

use std::sync::Arc;

use agentforge_infra::nats::connect_nats;
use tokio::sync::watch;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod commands;
mod config;
mod credentials;
mod orchestration;
mod publisher;
mod unix_socket_listener;
mod wal;

/// How often the periodic WAL-drain task wakes to flush events buffered during a
/// NATS outage. The WAL is otherwise only drained once at startup, so without
/// this a record written during the ~15-min per-agent JWT reconnect would sit
/// until the next process restart. 20s is frequent enough to flush promptly
/// after a reconnect without measurable overhead (an empty WAL is a no-op).
const WAL_DRAIN_INTERVAL_SECS: u64 = 20;

/// Drain the WAL through `publisher`: replay each buffered event, and on
/// successful publish acknowledge (delete) its file. Failed publishes are left
/// in place to retry on the next drain. Shared by the startup replay and the
/// periodic drain task so both honour the same replay-compatible record shape.
async fn drain_wal(wal: &wal::Wal, publisher: &publisher::EventPublisher) {
    let entries = match wal.replay().await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to read WAL for drain");
            return;
        }
    };
    for (path, entry) in entries {
        match serde_json::from_slice::<serde_json::Value>(&entry) {
            Ok(msg) => {
                let event_type = msg["payload"]["event_type"].as_str().unwrap_or("unknown");
                let data = msg["payload"]["data"].clone();
                match publisher.publish(event_type, data).await {
                    Ok(()) => {
                        if let Err(err) = wal.acknowledge(&path).await {
                            tracing::warn!(error = %err, path = %path.display(), "Failed to acknowledge WAL entry");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, path = %path.display(), "Failed to publish WAL entry, will retry next drain");
                    }
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, path = %path.display(), "Failed to deserialize WAL entry, skipping");
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured JSON logging with env-controlled filter.
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("info".parse().expect("valid tracing directive")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    tracing::info!(version = agentforge_core::VERSION, "Wisdoverse Forge Sidecar starting");

    let cfg = config::SidecarConfig::from_env()?;
    tracing::info!(agent_id = %cfg.agent_id, "Config loaded");

    // Connect to NATS.
    let nats_client = connect_nats(&cfg.nats_url).await?;
    tracing::info!("NATS connected");

    let resolved_cli_tool = cfg.resolved_cli_tool();
    let resolved_cli_model = cfg.resolved_cli_model();
    let resolved_runtime_kind = cfg.resolved_runtime_kind();
    tracing::info!(runtime_kind = %resolved_runtime_kind, "Resolved runtime kind for event subject namespacing");

    // Initialise components. The publisher and WAL are shared across the
    // heartbeat loop, startup/periodic WAL drain, and the relay-socket listener,
    // so both are `Arc`-wrapped and cloned per consumer.
    let publisher = Arc::new(publisher::EventPublisher::new(
        nats_client.clone(),
        cfg.agent_id.clone(),
        &cfg.hmac_secret,
        resolved_cli_tool.clone(),
        resolved_runtime_kind,
    ));
    let cmd_handler = commands::CommandHandler::new(nats_client.clone(), cfg.agent_id.clone());
    let wal_instance = Arc::new(wal::Wal::new(cfg.wal_path.as_deref()));

    // Replay any events buffered during a previous NATS outage.
    let pending = wal_instance.pending_count().await.unwrap_or(0);
    if pending > 0 {
        tracing::info!(count = pending, "Replaying WAL entries");
        drain_wal(&wal_instance, &publisher).await;
    }

    // Shutdown coordination.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn command handler.
    let cmd_shutdown = shutdown_rx.clone();
    let cmd_task = tokio::spawn(async move {
        cmd_handler.run(cmd_shutdown).await;
    });

    // Spawn the orchestration worker bridge if we know which CLI to drive. A
    // missing cli_tool means this container is heartbeat-only (legacy behaviour)
    // so we deliberately skip the subscriber rather than fail startup.
    let orchestration_task = match resolved_cli_tool {
        Some(cli_tool) => {
            let subscriber = orchestration::OrchestrationSubscriber::new(
                nats_client.clone(),
                cfg.agent_id.clone(),
                &cfg.hmac_secret,
                cli_tool,
                resolved_cli_model.clone(),
                cfg.wal_path.as_deref(),
                resolved_runtime_kind,
            );
            let orch_shutdown = shutdown_rx.clone();
            Some(tokio::spawn(async move {
                subscriber.run(orch_shutdown).await;
            }))
        }
        None => {
            tracing::info!("cli_tool unset — orchestration worker bridge disabled");
            None
        }
    };

    // Spawn the credential sync watcher if configured. The backend rollout
    // gate (CREDENTIAL_SYNC_ENABLED) is the off switch; the per-container
    // env (CREDS_DIR, ORG_ID, cli_tool) must all be set for the watcher to
    // have work to do. Any missing piece logs + skips without failing
    // startup — older container images must still boot on the new sidecar.
    let credentials_task = if cfg.credential_sync_enabled {
        match (cfg.creds_dir.as_deref(), cfg.org_id.as_deref(), cfg.resolved_cli_tool()) {
            (Some(dir), Some(org_id_str), Some(cli_tool)) => {
                match (uuid::Uuid::parse_str(&cfg.agent_id), uuid::Uuid::parse_str(org_id_str)) {
                    (Ok(agent_id), Ok(org_id)) => {
                        let dir = std::path::PathBuf::from(dir);
                        let client = nats_client.clone();
                        let secret = cfg.hmac_secret.clone();
                        let creds_shutdown = shutdown_rx.clone();
                        Some(tokio::spawn(async move {
                            if let Err(err) =
                                credentials::run(dir, client, agent_id, org_id, cli_tool, secret, creds_shutdown).await
                            {
                                tracing::error!(error = %err, "credential watcher exited with error");
                            }
                        }))
                    }
                    _ => {
                        tracing::warn!(
                            "credential sync enabled but agent_id/org_id are not valid UUIDs — skipping watcher"
                        );
                        None
                    }
                }
            }
            _ => {
                tracing::info!("credential sync enabled but CREDS_DIR/ORG_ID/cli_tool not all set — skipping watcher");
                None
            }
        }
    } else {
        tracing::info!("credential sync disabled (flag off)");
        None
    };

    // Publish once before the interval loop so a freshly started container is
    // immediately visible as an orchestration participant in the UI.
    if let Err(err) = publisher.heartbeat().await {
        tracing::warn!(error = %err, "Initial heartbeat failed");
    }

    // Spawn heartbeat loop.
    let hb_interval = std::time::Duration::from_secs(cfg.heartbeat_interval_secs);
    let mut hb_shutdown = shutdown_rx.clone();
    let hb_publisher = publisher.clone();
    let hb_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = hb_shutdown.changed() => {
                    if *hb_shutdown.borrow() { break; }
                }
                _ = tokio::time::sleep(hb_interval) => {
                    if let Err(err) = hb_publisher.heartbeat().await {
                        tracing::warn!(error = %err, "Heartbeat failed");
                    }
                }
            }
        }
    });

    // Spawn the relay-socket listener. This binds the Unix socket the CLI relay
    // hook writes to (previously unbound, so hook events were silently dropped)
    // and durably publishes each framed event.
    let relay_socket = cfg.resolved_relay_socket();
    let listener_publisher = publisher.clone();
    let listener_wal = wal_instance.clone();
    let listener_shutdown = shutdown_rx.clone();
    let listener_task = tokio::spawn(async move {
        if let Err(err) =
            unix_socket_listener::run(&relay_socket, listener_publisher, listener_wal, listener_shutdown).await
        {
            tracing::error!(error = %err, "Relay socket listener exited with error");
        }
    });

    // Spawn the periodic WAL-drain task. The WAL is otherwise only drained once
    // at startup, so an event buffered during the per-agent JWT reconnect would
    // sit until the next restart. Each tick, when NATS is connected and the WAL
    // is non-empty, replay the buffered events through the publisher.
    let drain_publisher = publisher.clone();
    let drain_wal_handle = wal_instance.clone();
    let drain_nats = nats_client.clone();
    let mut drain_shutdown = shutdown_rx.clone();
    let drain_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(WAL_DRAIN_INTERVAL_SECS));
        // Skip the immediate first tick — startup already drained once.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = drain_shutdown.changed() => {
                    if *drain_shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    let connected =
                        drain_nats.connection_state() == async_nats::connection::State::Connected;
                    let pending = drain_wal_handle.pending_count().await.unwrap_or(0);
                    if unix_socket_listener::should_drain(connected, pending) {
                        tracing::info!(count = pending, "Draining WAL after NATS reconnect");
                        drain_wal(&drain_wal_handle, &drain_publisher).await;
                    }
                }
            }
        }
    });

    // Block until SIGINT / ctrl-c.
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received");
    let _ = shutdown_tx.send(true);

    let _ = tokio::join!(cmd_task, hb_task, listener_task, drain_task);
    if let Some(task) = orchestration_task {
        let _ = task.await;
    }
    if let Some(task) = credentials_task {
        let _ = task.await;
    }

    tracing::info!("Sidecar shut down");
    Ok(())
}
