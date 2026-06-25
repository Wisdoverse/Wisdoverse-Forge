//! Wisdoverse Forge Sidecar — lightweight agent container companion.
//!
//! Runs inside each agent container and communicates with the platform via NATS.
//! Provides event publishing (with HMAC auth), command handling (request-reply),
//! heartbeat emission, and a file-based WAL for offline resilience.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InfoCommand {
    Help,
    Version,
}

const SIDECAR_HELP: &str = "\
Wisdoverse Forge Sidecar

Connects one managed agent to Wisdoverse Forge and forwards work, heartbeats, and results.

Most users should start it by copying the join command from the Agents page.

Usage:
  agentforge-sidecar
  agentforge-sidecar --help
  agentforge-sidecar --version

Required environment when starting manually:
  AGENT_ID             Agent identifier from the platform
  NATS_URL             Agent messaging URL
  HMAC_SECRET          Per-agent signing secret

Optional environment:
  AGENTFORGE_CLI_TOOL      Work tool to run, such as codex or claude
  AGENTFORGE_CLI_MODEL     Model override for tools that support one
  AGENTFORGE_RUNTIME_KIND  container, cli, or api
  WAL_PATH                 Folder for offline event retry records

Success looks like:
  The sidecar logs that config loaded, NATS connected, and heartbeats are publishing.
";

/// How often the periodic WAL-drain task wakes to flush events buffered during a
/// NATS outage. The WAL is otherwise only drained once at startup, so without
/// this a record written during the ~15-min per-agent JWT reconnect would sit
/// until the next process restart. 20s is frequent enough to flush promptly
/// after a reconnect without measurable overhead (an empty WAL is a no-op).
const WAL_DRAIN_INTERVAL_SECS: u64 = 20;

/// Number of pending WAL entries that indicates the relay is backing up. When
/// `pending >= DEGRADED_WAL_PENDING_THRESHOLD` OR any events have been dropped,
/// the heartbeat carries `health.degraded = true` so the liveness consumer can
/// warn operators without changing participant status (issue #808).
const DEGRADED_WAL_PENDING_THRESHOLD: usize = 1_000;

/// Build a relay health snapshot from current WAL counters for inclusion in the
/// heartbeat payload. Does not change any state — purely a read + struct build.
fn build_health_snapshot(wal: &wal::Wal, creds_sync_errors: u64) -> publisher::HealthSnapshot {
    let pending = wal.pending_cached();
    let dropped = wal.dropped_total();
    let degraded = pending >= DEGRADED_WAL_PENDING_THRESHOLD || dropped > 0 || creds_sync_errors > 0;
    let reason =
        degraded.then(|| format!("wal_pending={pending} wal_dropped={dropped} creds_sync_errors={creds_sync_errors}"));
    publisher::HealthSnapshot { degraded, reason, wal_pending: pending, wal_dropped: dropped, creds_sync_errors }
}

// The WAL drain lives in `unix_socket_listener::drain_wal` so it shares the
// WAL-first confirmed-handoff contract (publish → bounded flush → ack only on a
// confirmed handoff) with the live relay path, instead of acknowledging on
// `publish()` Ok alone (F062).

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(command) = info_command(std::env::args().skip(1)) {
        match command {
            InfoCommand::Help => println!("{SIDECAR_HELP}"),
            InfoCommand::Version => println!("{}", agentforge_core::VERSION),
        }
        return Ok(());
    }

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
    // Seed the O(1) cached pending counter from the actual on-disk file count so
    // crash-recovered WAL files are counted against the backpressure cap before
    // the relay-socket listener starts accepting new events.
    wal_instance.init_pending().await;

    // Replay any events buffered during a previous NATS outage — but only once
    // NATS is actually connected. Draining while disconnected would re-buffer
    // every record and (without the confirmed-handoff contract) risk deleting
    // them before the server confirms; the periodic drain picks them up after
    // the connection establishes (F062).
    let pending = wal_instance.pending_count().await.unwrap_or(0);
    let startup_connected = nats_client.connection_state() == async_nats::connection::State::Connected;
    if unix_socket_listener::should_drain(startup_connected, pending) {
        tracing::info!(count = pending, "Replaying WAL entries");
        unix_socket_listener::drain_wal(&wal_instance, &publisher).await;
    } else if pending > 0 {
        tracing::info!(
            count = pending,
            connected = startup_connected,
            "WAL has entries; deferring drain to periodic task"
        );
    }

    // Bind the relay socket owner-only NOW, while the process is still effectively
    // single-threaded (before any task is spawned), so the brief process-global
    // umask change during bind cannot affect concurrent file creation (F065). The
    // listener is served by `unix_socket_listener::run` further below.
    let relay_socket = unix_socket_listener::RELAY_SOCKET_PATH;
    let relay_listener = unix_socket_listener::bind_relay_listener(relay_socket)?;

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
    // Shared counter of credential-sync losses (NATS unreachable, no WAL retry),
    // folded into the heartbeat health snapshot so the platform has visibility
    // instead of only a container-local log (#891/F063).
    let creds_sync_errors = Arc::new(AtomicU64::new(0));
    let credentials_task = if cfg.credential_sync_enabled {
        match (cfg.creds_dir.as_deref(), cfg.org_id.as_deref(), cfg.resolved_cli_tool()) {
            (Some(dir), Some(org_id_str), Some(cli_tool)) => {
                match (uuid::Uuid::parse_str(&cfg.agent_id), uuid::Uuid::parse_str(org_id_str)) {
                    (Ok(agent_id), Ok(org_id)) => {
                        let dir = std::path::PathBuf::from(dir);
                        let client = nats_client.clone();
                        let secret = cfg.hmac_secret.clone();
                        let creds_shutdown = shutdown_rx.clone();
                        let creds_errors = creds_sync_errors.clone();
                        Some(tokio::spawn(async move {
                            let watcher_errors = creds_errors.clone();
                            if let Err(err) = credentials::run(
                                dir,
                                client,
                                agent_id,
                                org_id,
                                cli_tool,
                                secret,
                                creds_shutdown,
                                watcher_errors,
                            )
                            .await
                            {
                                tracing::error!(error = %err, "credential watcher exited with error");
                                // The watcher never entered its loop (e.g. CREDS_DIR
                                // unwatchable) so credential sync is permanently stopped
                                // for this container — mark the heartbeat degraded so it
                                // is not silently reported as healthy (#891/F063).
                                creds_errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }))
                    }
                    _ => {
                        tracing::warn!(
                            "credential sync enabled but agent_id/org_id are not valid UUIDs — skipping watcher"
                        );
                        // Sync is ENABLED but can never start (misconfig) — degrade
                        // the heartbeat instead of reporting healthy (#891/F063).
                        creds_sync_errors.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            }
            _ => {
                tracing::info!("credential sync enabled but CREDS_DIR/ORG_ID/cli_tool not all set — skipping watcher");
                // Sync is ENABLED but required env is missing so it can never
                // start — degrade the heartbeat instead of reporting healthy.
                creds_sync_errors.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    } else {
        // Sync intentionally disabled (flag off) — NOT a failure, stays healthy.
        tracing::info!("credential sync disabled (flag off)");
        None
    };

    // Publish once before the interval loop so a freshly started container is
    // immediately visible as an orchestration participant in the UI.
    if let Err(err) =
        publisher.heartbeat(build_health_snapshot(&wal_instance, creds_sync_errors.load(Ordering::Relaxed))).await
    {
        tracing::warn!(error = %err, "Initial heartbeat failed");
    }

    // Spawn heartbeat loop.
    let hb_interval = std::time::Duration::from_secs(cfg.heartbeat_interval_secs);
    let mut hb_shutdown = shutdown_rx.clone();
    let hb_publisher = publisher.clone();
    let hb_wal = wal_instance.clone();
    let hb_creds_errors = creds_sync_errors.clone();
    let hb_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = hb_shutdown.changed() => {
                    if *hb_shutdown.borrow() { break; }
                }
                _ = tokio::time::sleep(hb_interval) => {
                    let health = build_health_snapshot(&hb_wal, hb_creds_errors.load(Ordering::Relaxed));
                    if let Err(err) = hb_publisher.heartbeat(health).await {
                        tracing::warn!(error = %err, "Heartbeat failed");
                    }
                }
            }
        }
    });

    // Serve the relay-socket listener on the pre-bound socket. The Unix socket the
    // CLI relay hook writes to was bound owner-only above (`relay_listener`),
    // BEFORE any task spawned, so the brief umask change during bind cannot affect
    // concurrent file creation (F065). The path is a single hardcoded const shared
    // with the hook default, the entrypoint, and the healthcheck — no env override,
    // so all four sides can never disagree.
    let listener_publisher = publisher.clone();
    let listener_wal = wal_instance.clone();
    let listener_shutdown = shutdown_rx.clone();
    let listener_task = tokio::spawn(async move {
        if let Err(err) =
            unix_socket_listener::run(relay_listener, relay_socket, listener_publisher, listener_wal, listener_shutdown)
                .await
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
                    let pending = drain_wal_handle.pending_cached();
                    if unix_socket_listener::should_drain(connected, pending) {
                        tracing::info!(count = pending, "Draining WAL after NATS reconnect");
                        unix_socket_listener::drain_wal(&drain_wal_handle, &drain_publisher).await;
                    }
                }
            }
        }
    });

    // CN-1: block until SIGINT (ctrl-c) OR SIGTERM. A container stop
    // (`docker stop`, a Kubernetes pod termination) delivers SIGTERM, not
    // SIGINT; awaiting only ctrl_c meant the sidecar ignored it and was
    // hard-killed after the grace period, dropping in-flight events instead of
    // draining. Mirror the API server's dual-signal handler.
    wait_for_shutdown_signal().await;
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

/// CN-1: resolve when EITHER SIGINT (ctrl-c) or SIGTERM is received, so a
/// container/pod stop (which delivers SIGTERM) triggers the same graceful drain
/// as an interactive ctrl-c. Mirrors the API server's `shutdown_signal`.
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn info_command<I, S>(args: I) -> Option<InfoCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().find_map(|arg| match arg.as_ref() {
        "-h" | "--help" | "help" => Some(InfoCommand::Help),
        "-V" | "--version" | "version" => Some(InfoCommand::Version),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{DEGRADED_WAL_PENDING_THRESHOLD, InfoCommand, build_health_snapshot, info_command, wal};

    #[test]
    fn detects_help_flags_before_env_config() {
        assert_eq!(info_command(["--help"]), Some(InfoCommand::Help));
        assert_eq!(info_command(["help"]), Some(InfoCommand::Help));
    }

    #[test]
    fn detects_version_flags_before_env_config() {
        assert_eq!(info_command(["--version"]), Some(InfoCommand::Version));
        assert_eq!(info_command(["version"]), Some(InfoCommand::Version));
    }

    #[test]
    fn ignores_runtime_args() {
        assert_eq!(info_command(["--some-runtime-flag"]), None);
    }

    // -------------------------------------------------------------------------
    // Health snapshot builder tests (issue #808)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn health_snapshot_not_degraded_below_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = wal::Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 10_000);
        // Write a few entries — well below threshold.
        for _ in 0..5 {
            wal.append(b"{}").await.unwrap();
        }
        let snap = build_health_snapshot(&wal, 0);
        assert!(!snap.degraded, "well below threshold must not be degraded");
        assert!(snap.reason.is_none());
        assert_eq!(snap.wal_pending, 5);
        assert_eq!(snap.wal_dropped, 0);
    }

    #[tokio::test]
    async fn health_snapshot_degraded_at_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = wal::Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), DEGRADED_WAL_PENDING_THRESHOLD + 100);
        // Manually advance the cached counter to the threshold.
        for _ in 0..DEGRADED_WAL_PENDING_THRESHOLD {
            wal.append(b"{}").await.unwrap();
        }
        let snap = build_health_snapshot(&wal, 0);
        assert!(snap.degraded, "at threshold must be degraded");
        assert!(snap.reason.is_some());
        let reason = snap.reason.unwrap();
        assert!(reason.contains(&format!("wal_pending={DEGRADED_WAL_PENDING_THRESHOLD}")));
    }

    #[tokio::test]
    async fn health_snapshot_degraded_on_any_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        // Cap = 1 so the second append triggers a drop.
        let wal = wal::Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 1);
        wal.append(b"{}").await.unwrap();
        wal.append(b"{}").await.unwrap(); // dropped
        let snap = build_health_snapshot(&wal, 0);
        assert!(snap.degraded, "any dropped event must mark degraded");
        assert_eq!(snap.wal_dropped, 1);
    }

    #[tokio::test]
    async fn health_snapshot_degraded_on_any_creds_sync_error() {
        // #891/F063: a credential-sync loss (NATS unreachable) must surface as a
        // degraded heartbeat with a reason the platform can warn on.
        let tmp = tempfile::tempdir().unwrap();
        let wal = wal::Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 10_000);
        let snap = build_health_snapshot(&wal, 2);
        assert!(snap.degraded, "any credential-sync error must mark degraded");
        assert_eq!(snap.creds_sync_errors, 2);
        assert!(snap.reason.unwrap().contains("creds_sync_errors=2"));
    }
}
