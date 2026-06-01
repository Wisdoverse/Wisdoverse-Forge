//! Wisdoverse Forge Server — main application binary.
//!
//! Initializes tracing, connects to PostgreSQL, runs migrations,
//! builds the Axum router, and starts the HTTP server with graceful shutdown.

mod streams;

use std::path::PathBuf;
use std::sync::Arc;

use agentforge_api::health::ContextFeatureFlags;
use agentforge_api::repositories::credential::cli::CliCredentialRepository;
use agentforge_api::repositories::user::llm_config::UserLlmConfigRepository;
use agentforge_api::services::cli_credential::CliCredentialService;
use agentforge_api::services::credential_writer::ServiceCredentialWriter;
use agentforge_api::{AppState, create_router};
use agentforge_auth::JwtManager;
use agentforge_core::AppConfig;
use agentforge_db::{create_pool, run_migrations};
use agentforge_infra::{NatsClient, ObjectStorageClient, RedisClient};
use agentforge_jobs::{
    DependencyReconcileWorker, EventStreamWorker, OrchestrationMetricsWorker, OrchestrationOutboxPublisher,
    OrchestrationResultWorker, ParticipantLivenessWorker, SqlxAgentOwnerLookup, SqlxCredentialHmacSecretLookup,
    SqlxHmacSecretLookup, SqlxNatsConnectPasswordLookup, SqlxParticipantLookup, SqlxTaskWriter,
};
use anyhow::{Result, anyhow};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use secrecy::{ExposeSecret, SecretString};
use tokio::signal;
use tokio::sync::{RwLock, watch};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    Serve,
    MigrateOnly,
}

fn parse_run_mode(args: &[String]) -> Result<RunMode> {
    match args {
        [_] => Ok(RunMode::Serve),
        [_, flag] if flag == "--migrate-only" => Ok(RunMode::MigrateOnly),
        [_, flag] => Err(anyhow!("unknown argument: {flag}")),
        [_, first, ..] => Err(anyhow!("unexpected arguments starting at: {first}")),
        [] => Ok(RunMode::Serve),
    }
}

fn env_flag(name: &str, default: bool) -> Result<bool> {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(anyhow!("{name} must be boolean when set (true/false/1/0/yes/no/on/off)")),
        },
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let run_mode = parse_run_mode(&args)?;

    // 1. Load configuration from environment variables.
    let config = AppConfig::from_env()?;

    // 2. Initialize tracing — JSON for production, pretty for development.
    if config.is_production() {
        tracing_subscriber::registry()
            .with(EnvFilter::new(&config.log_level))
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(EnvFilter::new(&config.log_level))
            .with(tracing_subscriber::fmt::layer().pretty())
            .init();
    }

    tracing::info!(version = agentforge_core::VERSION, ?run_mode, "Wisdoverse Forge starting (Rust)");

    // Install the Prometheus recorder before any worker can emit metrics.
    // `install_recorder` registers a global `metrics::Recorder`, so every
    // `metrics::counter!` / `metrics::gauge!` call feeds into the same handle
    // we hand to the `/metrics` scrape route.
    //
    // Override the histogram buckets for `http_request_duration_seconds` with
    // SLO-aligned bounds so `histogram_quantile(0.95, ...)` resolves
    // meaningfully near each agents-runtime budget (500ms create, 800ms
    // enroll, 2s container restart). Without explicit buckets the exporter
    // renders this metric as a Prometheus summary (quantiles) instead of a
    // histogram, so no `_bucket{le=...}` series exist and the dashboard's
    // histogram_quantile() / SLO _bucket rate queries return nothing.
    let prometheus_handle = Arc::new(
        PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("http_request_duration_seconds".to_owned()),
                &agentforge_api::observability::http_metrics::HTTP_DURATION_BUCKETS,
            )
            .map_err(|err| anyhow!("configure http_request_duration_seconds buckets: {err}"))?
            .install_recorder()
            .map_err(|err| anyhow!("install prometheus recorder: {err}"))?,
    );

    // Register metric descriptions so dashboards have series present from the
    // first scrape even before traffic or a background sweep fires.
    agentforge_jobs::register_metrics();
    agentforge_api::observability::register_http_metrics();
    agentforge_api::services::cli_auth_proxy::register_cli_auth_proxy_metrics();
    agentforge_api::services::usage_analytics::register_usage_analytics_metrics();

    // 3. Create database pool.
    let pool = create_pool(&config).await?;
    tracing::info!("Database connected");

    // 4. Run pending migrations.
    run_migrations(&pool).await?;
    tracing::info!("Migrations complete");

    if run_mode == RunMode::MigrateOnly {
        tracing::info!("Migrate-only mode complete");
        return Ok(());
    }

    let runtime_capability_registry =
        agentforge_api::services::runtime_capability_registry::RuntimeCapabilityRegistryService::new(
            agentforge_api::repositories::runtime_capability::RuntimeCapabilityRepository::new(pool.clone()),
        );
    runtime_capability_registry
        .refresh_from_code()
        .await
        .map_err(|err| anyhow!("runtime capability registry startup refresh failed: {}", err.kind))?;
    tracing::info!("Runtime capability registry refreshed from typed matrix");

    let orchestration_result_consumer_enabled = env_flag("ORCHESTRATION_RESULT_CONSUMER_ENABLED", true)?;
    let orchestration_outbox_publisher_enabled = env_flag("ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED", true)?;
    let orchestration_liveness_enabled = env_flag("ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED", true)?;
    let orchestration_metrics_enabled = env_flag("ORCHESTRATION_CONTROL_PLANE_METRICS_ENABLED", true)?;
    let context_features = ContextFeatureFlags {
        governance: env_flag("CONTEXT_GOVERNANCE_ENABLED", false)?,
        preview: env_flag("CONTEXT_PREVIEW_ENABLED", false)?,
        injection: env_flag("CONTEXT_INJECTION_ENABLED", false)?,
        analytics: env_flag("CONTEXT_ANALYTICS_ENABLED", false)?,
    };
    let context_analytics_enabled = context_features.analytics;
    let orchestration_requires_nats = orchestration_result_consumer_enabled
        || orchestration_outbox_publisher_enabled
        || orchestration_liveness_enabled;
    tracing::info!(
        result_consumer = orchestration_result_consumer_enabled,
        assignment_outbox_publisher = orchestration_outbox_publisher_enabled,
        participant_liveness = orchestration_liveness_enabled,
        control_plane_metrics = orchestration_metrics_enabled,
        context_governance = context_features.governance,
        context_preview = context_features.preview,
        context_injection = context_features.injection,
        context_analytics = context_features.analytics,
        "rollout flags resolved"
    );

    // 5. Initialize infrastructure clients (Redis and NATS are optional).
    let jwt = Arc::new(JwtManager::new(config.jwt_secret.expose_secret(), config.jwt_expiry_seconds));
    let redis = Arc::new(RwLock::new(RedisClient::new(&config).await));
    let context_resolver = Arc::new(
        agentforge_api::services::context_resolver::ContextResolverService::new(
            pool.clone(),
            runtime_capability_registry.clone(),
        )
        .with_redis(redis.clone()),
    );
    let nats = Arc::new(NatsClient::new(&config).await);
    let object_storage = Arc::new(
        ObjectStorageClient::new(&config)
            .await
            .map_err(|err| anyhow!("failed to initialize attachment object storage: {}", err.kind))?,
    );
    tracing::info!(backend = object_storage.backend(), "Attachment object storage initialized");
    let billing_gateway = agentforge_api::services::billing::billing_gateway_from_config(&config)
        .map_err(|err| anyhow!("failed to initialize billing gateway: {}", err.kind))?;
    if billing_gateway.is_configured() {
        tracing::info!("Stripe billing gateway configured");
    } else {
        tracing::warn!("Stripe billing gateway disabled; billing write paths fail closed");
    }
    let email_sender = agentforge_api::services::email::sender_from_config(&config)
        .map_err(|err| anyhow!("failed to initialize email sender: {}", err.kind))?;
    if email_sender.is_configured() {
        tracing::info!("SMTP email sender configured");
    } else {
        tracing::warn!("SMTP email sender disabled; password reset requests will return EMAIL_UNAVAILABLE");
    }
    if config.is_production() && orchestration_requires_nats && nats.client().is_none() {
        return Err(anyhow!(
            "NATS must be connected in production when orchestration result/outbox/liveness workers are enabled"
        ));
    }

    // Ensure JetStream streams exist before any consumer tries to get_stream().
    // Skipped when NATS is not connected; the JetStream-backed workers are
    // also skipped in that case so no stream is needed.
    if let Some(client) = nats.client()
        && let Err(err) = streams::ensure(client.clone()).await
    {
        if config.is_production() && orchestration_requires_nats {
            return Err(anyhow!("failed to ensure required JetStream streams: {err}"));
        }
        tracing::warn!(error = %err, "failed to ensure required JetStream streams; durable workers may not start");
    }

    // 6. Try to connect Docker (optional — container features disabled if unavailable).
    let docker = match agentforge_platform::DockerClient::new(&config) {
        Ok(client) => {
            tracing::info!("Docker client connected");
            Some(Arc::new(client))
        }
        Err(err) => {
            tracing::warn!(error = %err, "Docker not available, container features disabled");
            None
        }
    };

    let live_mcp = agentforge_api::mcp::build_live_mcp_components(pool.clone(), docker.clone()).await?;
    if live_mcp.is_some() {
        tracing::info!("Internal MCP bridge enabled on Rust API");
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let event_worker = if let Some(client) = nats.client().cloned() {
        Some(EventStreamWorker::connect(pool.clone(), client).await?)
    } else {
        tracing::info!("NATS not connected; event stream worker disabled");
        None
    };
    let event_worker_handle = event_worker.map(|worker| {
        let worker_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            worker.run(worker_shutdown).await;
        })
    });

    // Orchestration result consumer — drains durable JetStream task outcomes
    // into DB complete/fail so a short backend outage does not drop the sidecar
    // result on the floor.
    let orchestration_result_handle = if orchestration_result_consumer_enabled {
        match nats.client().cloned() {
            Some(client) => match OrchestrationResultWorker::connect(
                client.clone(),
                SqlxParticipantLookup::new(pool.clone()),
                SqlxTaskWriter::new(pool.clone()).with_realtime(client),
                SqlxHmacSecretLookup::new(pool.clone()),
            )
            .await
            {
                Ok(worker) => {
                    let worker_shutdown = shutdown_rx.clone();
                    Some(tokio::spawn(async move { worker.run(worker_shutdown).await }))
                }
                Err(err) => {
                    tracing::warn!(error = %err, "orchestration result worker: failed to connect, skipping");
                    None
                }
            },
            None => {
                tracing::info!("NATS not connected; orchestration result worker disabled");
                None
            }
        }
    } else {
        tracing::info!("orchestration result worker disabled (flag off)");
        None
    };

    let orchestration_outbox_handle = if orchestration_outbox_publisher_enabled {
        nats.client().cloned().map(|client| {
            let worker = OrchestrationOutboxPublisher::new(pool.clone(), client);
            let worker_shutdown = shutdown_rx.clone();
            tokio::spawn(async move { worker.run(worker_shutdown).await })
        })
    } else {
        tracing::info!("orchestration assignment outbox publisher disabled (flag off)");
        None
    };

    let orchestration_metrics_handle = if orchestration_metrics_enabled {
        let worker = OrchestrationMetricsWorker::new(pool.clone(), agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER);
        let worker_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move { worker.run(worker_shutdown).await }))
    } else {
        tracing::info!("orchestration control-plane metrics worker disabled (flag off)");
        None
    };

    // Participant liveness consumer — bridges sidecar heartbeats into the
    // orchestration participants table, marks stale rows offline, fails
    // expired `working` leases closed, and drains queued work onto recovered
    // participants. This closes the "running sidecar but no schedulable
    // participant" gap without silently reassigning in-flight work.
    let participant_liveness_handle = if orchestration_liveness_enabled {
        nats.client().cloned().map(|client| {
            let worker = ParticipantLivenessWorker::new(client, pool.clone());
            let worker_shutdown = shutdown_rx.clone();
            tokio::spawn(async move { worker.run(worker_shutdown).await })
        })
    } else {
        tracing::info!("participant liveness worker disabled (flag off)");
        None
    };

    // Dependency reconcile worker — backstop for `complete_task`'s tx
    // (issue #37). Periodically catches `blocked/waiting_dependency` rows
    // whose parent already completed and flips them back to `queued`.
    let dependency_reconcile_handle = {
        let worker = DependencyReconcileWorker::new(pool.clone());
        let worker_shutdown = shutdown_rx.clone();
        tokio::spawn(async move { worker.run(worker_shutdown).await })
    };

    // CLI agent-image auto-updater — default-OFF. When enabled AND a Docker
    // daemon is available, periodically pulls newer `agent-<tool>:latest`
    // overlays so newly spawned agents use the current CLI. Running agents are
    // never touched. Skipped when docker is None (air-gapped / no daemon).
    // Shared read-only status snapshot the worker writes each tick and the
    // `GET /admin/cli-images` endpoint reads. Built unconditionally so the
    // endpoint exists even when the worker is off (it then reports an empty
    // set), and held in AppState independent of whether the worker spawned.
    let cli_image_status = Arc::new(agentforge_jobs::CliImageUpdateStatus::new());
    let cli_image_updater_handle = if config.cli_image_auto_update_enabled {
        match docker.clone() {
            Some(client) => {
                let worker = agentforge_jobs::CliImageUpdater::new(client, cli_image_status.clone())
                    .with_interval(std::time::Duration::from_secs(config.cli_image_auto_update_interval_secs))
                    // Publish admin toasts on `broadcast.admin.cli_image` when NATS
                    // is configured; None leaves toasts off (status + metrics only).
                    .with_event_sink(nats.client().cloned())
                    // Default-off prune of superseded dangling agent overlays.
                    .with_prune(config.cli_image_prune_enabled);
                let worker_shutdown = shutdown_rx.clone();
                Some(tokio::spawn(async move { worker.run(worker_shutdown).await }))
            }
            None => {
                tracing::warn!("cli image auto-updater enabled but Docker unavailable; skipping");
                None
            }
        }
    } else {
        tracing::info!("cli image auto-updater disabled (flag off)");
        // Surface a likely misconfiguration: prune lives inside the updater
        // loop, so CLI_IMAGE_PRUNE_ENABLED is inert while auto-update is off.
        if config.cli_image_prune_enabled {
            tracing::warn!(
                "CLI_IMAGE_PRUNE_ENABLED=true has no effect because CLI_IMAGE_AUTO_UPDATE_ENABLED is off; \
                 prune runs inside the updater loop, which is not spawned"
            );
        }
        None
    };

    // Auth callout worker — mints per-agent User JWTs for every sidecar
    // CONNECT on `$SYS.REQ.USER.AUTH`. Only spawns when NATS is
    // configured; `AppConfig::from_env` already fail-fasted if any
    // `NatsCalloutConfig` field is missing when `NATS_URL` is set.
    // Separate from the NATS client in `state.nats` — that one carries
    // backend credentials; this worker authenticates as the AUTH-account
    // `auth_service` user.
    //
    // The `"AGENTFORGE"` literal below must match the account label in
    // `docker/nats.conf` — it is the `aud` claim on every inner User JWT,
    // and NATS server-config mode resolves it via
    // `s.LookupAccount(aud)` to place the minted user in the right
    // account (see issue #55 + `server/auth_callout.go`).
    let (auth_callout_handle, auth_callout_service) = match config.nats_url.as_deref() {
        Some(url) => {
            let lookup = SqlxNatsConnectPasswordLookup::new(pool.clone());
            match agentforge_api::services::auth_callout::AuthCalloutWorker::new(
                url,
                &config.nats_callout,
                "AGENTFORGE".to_string(),
                lookup,
            )
            .await
            {
                Ok(worker) => {
                    let service = Arc::new(worker.service_handle());
                    let worker_shutdown = shutdown_rx.clone();
                    let handle = tokio::spawn(async move { worker.run(worker_shutdown).await });
                    (Some(handle), Some(service))
                }
                Err(err) => {
                    tracing::error!(
                        error = ?err,
                        "Auth callout worker init failed; continuing without per-agent auth"
                    );
                    (None, None)
                }
            }
        }
        None => (None, None),
    };

    // 7. Capture bind address before moving config into Arc.
    let addr = format!("{}:{}", config.host, config.port);

    // 8. Build router with shared state.
    // `LLM_ENCRYPTION_KEY` unset ⇒ None (credential tiers 1/2 disabled, tier
    // 3 system keys still work). But if the var IS set and fails to decode,
    // fail startup — silent disablement would route every user to the system
    // fallback without warning, which is exactly the misconfiguration nightmare
    // this platform is meant to prevent.
    let encryption_key = match config.llm_encryption_key.as_ref().map(|s| s.expose_secret()) {
        Some(s) if !s.is_empty() => Some(
            agentforge_core::crypto::decode_key_hex(s)
                .map_err(|err| anyhow!("LLM_ENCRYPTION_KEY is set but invalid: {err}"))?,
        ),
        _ => None,
    };

    // Construct a shared CliCredentialService for the credential sync worker.
    // Mirrors the on-demand construction in routes/cli_credentials.rs but
    // hoisted here so the worker can hold an Arc without depending on AppState.
    let cli_credential_service = Arc::new(CliCredentialService::new(
        CliCredentialRepository::new(pool.clone()),
        UserLlmConfigRepository::new(pool.clone()),
        encryption_key,
        config
            .oauth_mount_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/agentforge/oauth-mounts")),
        config.container_anthropic_api_key.as_ref().map(|s| SecretString::from(s.expose_secret().to_string())),
        config.container_google_api_key.as_ref().map(|s| SecretString::from(s.expose_secret().to_string())),
        config.container_openai_api_key.as_ref().map(|s| SecretString::from(s.expose_secret().to_string())),
    ));

    // Credential sync worker — reads NATS JetStream `CREDENTIALS` stream and
    // upserts encrypted credential blobs. Only spawns when NATS is connected
    // and the feature flag is on.
    let credential_worker_handle = if config.credential_sync_enabled {
        match nats.client() {
            Some(client) => {
                let owners = SqlxAgentOwnerLookup::new(pool.clone());
                let hmac = SqlxCredentialHmacSecretLookup::new(pool.clone());
                let writer = ServiceCredentialWriter::new(cli_credential_service.clone());
                match agentforge_jobs::CredentialStreamWorker::connect(client.clone(), owners, hmac, writer).await {
                    Ok(worker) => {
                        let worker_shutdown = shutdown_rx.clone();
                        Some(tokio::spawn(async move { worker.run(worker_shutdown).await }))
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "credential sync worker: failed to connect, skipping");
                        None
                    }
                }
            }
            None => {
                tracing::info!("NATS not connected; credential sync worker disabled");
                None
            }
        }
    } else {
        tracing::info!("credential sync worker disabled (flag off)");
        None
    };

    let cli_auth_memory_store = Arc::new(agentforge_api::services::cli_auth_proxy::MemoryStateStore::new());

    let llm_factory = Arc::new(agentforge_llm::LlmProviderFactory::new(config.ollama_base_url.clone()));
    let inflight_prompts = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let cli_auth_nats_client = nats.client().cloned();

    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        jwt,
        redis: redis.clone(),
        nats,
        object_storage,
        billing_gateway,
        email_sender,
        agent_command_bus: None,
        docker,
        mcp_tools: live_mcp.as_ref().map(|(_, tools)| tools.clone()),
        mcp_internal_token: live_mcp.as_ref().map(|(token, _)| token.clone()),
        encryption_key,
        cli_auth_memory_store: cli_auth_memory_store.clone(),
        prometheus_handle,
        auth_callout: auth_callout_service,
        llm_factory,
        context_resolver,
        context_features,
        inflight_prompts,
        cli_image_status,
    };

    // CLI auth proxy refresh loop — every 4 hours, refresh tokens older than
    // 3 hours (matches legacy `cli-auth-proxy-refresh.worker.ts`). Self-
    // disables when no encryption key is configured: refresh_stale would
    // return an empty summary every tick, so skip spawning the task.
    let cli_auth_refresh_handle = if let Some(key) = state.encryption_key {
        let pool = pool.clone();
        let redis = redis.clone();
        let config = state.config.clone();
        let memory = cli_auth_memory_store.clone();
        let nats_client = cli_auth_nats_client;
        let worker_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            run_cli_auth_refresh_loop(pool, redis, config, key, memory, nats_client, worker_shutdown).await;
        }))
    } else {
        tracing::info!("CLI auth proxy refresh worker disabled (no LLM_ENCRYPTION_KEY)");
        None
    };

    let context_usage_analytics_handle = if context_analytics_enabled {
        let pool = pool.clone();
        let worker_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            run_context_usage_analytics_refresh_loop(pool, worker_shutdown).await;
        }))
    } else {
        tracing::info!("context usage analytics refresh worker disabled (flag off)");
        None
    };

    let app = create_router(state);

    // 9. Bind and serve with graceful shutdown.
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Listening");

    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal(shutdown_tx)).await?;

    if let Some(handle) = event_worker_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "event stream worker join failed"),
        }
    }
    if let Some(handle) = orchestration_result_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "orchestration result worker join failed"),
        }
    }
    if let Some(handle) = orchestration_outbox_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "orchestration outbox worker join failed"),
        }
    }
    if let Some(handle) = orchestration_metrics_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "orchestration metrics worker join failed"),
        }
    }
    if let Some(handle) = cli_image_updater_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "cli image updater join failed"),
        }
    }
    if let Some(handle) = participant_liveness_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "participant liveness worker join failed"),
        }
    }
    if let Some(handle) = auth_callout_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "auth callout worker join failed"),
        }
    }
    if let Some(handle) = cli_auth_refresh_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "cli auth refresh worker join failed"),
        }
    }
    if let Some(handle) = context_usage_analytics_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "context usage analytics refresh worker join failed"),
        }
    }
    if let Some(handle) = credential_worker_handle {
        match handle.await {
            Ok(()) => {}
            Err(err) => tracing::warn!(error = %err, "credential sync worker join failed"),
        }
    }
    match dependency_reconcile_handle.await {
        Ok(()) => {}
        Err(err) => tracing::warn!(error = %err, "dependency reconcile worker join failed"),
    }

    tracing::info!("Server shut down gracefully");
    Ok(())
}

/// Background refresh loop for the CLI auth proxy. Ticks on `REFRESH_INTERVAL`,
/// each tick sweeps every stored credential whose `last_refresh` is older
/// than `REFRESH_THRESHOLD` and hits the provider's `refresh_token` grant.
///
/// The cadence mirrors legacy TS (`cli-auth-proxy-refresh.worker.ts`) so
/// tokens are refreshed at most ~4h after they age past the 3h threshold.
/// First tick waits a short grace period so server startup isn't blocked by
/// HTTP round-trips to upstream IdPs.
async fn run_cli_auth_refresh_loop(
    pool: sqlx::PgPool,
    redis: std::sync::Arc<tokio::sync::RwLock<agentforge_infra::RedisClient>>,
    config: std::sync::Arc<AppConfig>,
    encryption_key: [u8; 32],
    memory: std::sync::Arc<agentforge_api::services::cli_auth_proxy::MemoryStateStore>,
    nats_client: Option<async_nats::Client>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use agentforge_api::repositories::credential::cli::CliCredentialRepository;
    use agentforge_api::services::cli_auth_proxy::{CliAuthProxyService, resolve_providers};

    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(4 * 3600);
    const REFRESH_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(3 * 3600);

    // First tick grace period so server startup isn't blocked by outbound
    // IdP round-trips.
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    tracing::info!(
        interval_secs = REFRESH_INTERVAL.as_secs(),
        threshold_secs = REFRESH_THRESHOLD.as_secs(),
        "CLI auth proxy refresh loop started"
    );

    let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            _ = ticker.tick() => {
                let store = if config.redis_url.is_some() {
                    agentforge_api::services::cli_auth_proxy::StateStore::Redis(redis.clone())
                } else {
                    agentforge_api::services::cli_auth_proxy::StateStore::Memory(memory.clone())
                };
                let service = CliAuthProxyService::new(
                    resolve_providers(&config),
                    CliCredentialRepository::new(pool.clone()),
                    Some(encryption_key),
                    store,
                    config.cli_auth_proxy_revoke_threshold,
                );
                let summary = service.refresh_stale(REFRESH_THRESHOLD).await;
                if !summary.revoked_credentials.is_empty() {
                    publish_cli_credential_notifications(&pool, nats_client.as_ref(), &summary.revoked_credentials).await;
                }
                tracing::info!(
                    refreshed = summary.refreshed,
                    failed = summary.failed,
                    eligible = summary.eligible,
                    invalid_grant = summary.invalid_grant,
                    invalid_client = summary.invalid_client,
                    revoked = summary.revoked_credentials.len(),
                    "CLI auth proxy refresh completed"
                );
            }
        }
    }

    tracing::info!("CLI auth proxy refresh loop shut down");
}

/// Nightly refresh loop for the context usage analytics materialized view.
///
/// The dashboard reads the last-good snapshot and shows a staleness banner
/// after 24h. Multiple API replicas can run this loop safely: the service
/// refresh method uses a PostgreSQL advisory lock and skips if another replica
/// is already refreshing.
async fn run_context_usage_analytics_refresh_loop(
    pool: sqlx::PgPool,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

    tracing::info!(interval_secs = REFRESH_INTERVAL.as_secs(), "context usage analytics refresh loop started");
    let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // First tick from `interval` fires immediately; skip it. Migrations create
    // the initial snapshot, and Unit 5.1 intentionally avoids on-demand refresh.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            _ = ticker.tick() => {
                let service = agentforge_api::services::usage_analytics::UsageAnalyticsService::new(pool.clone());
                match service.refresh_context_usage_snapshot().await {
                    Ok(agentforge_api::services::usage_analytics::RefreshOutcome::Refreshed) => {
                        tracing::info!("context usage analytics snapshot refreshed");
                    }
                    Ok(agentforge_api::services::usage_analytics::RefreshOutcome::SkippedLocked) => {
                        tracing::debug!("context usage analytics refresh skipped; another replica holds the lock");
                    }
                    Err(err) => {
                        tracing::error!(
                            error = %err.kind,
                            "context usage analytics refresh failed; dashboard will keep serving last-good snapshot"
                        );
                    }
                }
            }
        }
    }

    tracing::info!("context usage analytics refresh loop shut down");
}

async fn publish_cli_credential_notifications(
    pool: &sqlx::PgPool,
    nats_client: Option<&async_nats::Client>,
    revoked_credentials: &[agentforge_api::services::cli_auth_proxy::RevokedCliCredential],
) {
    let Some(client) = nats_client else {
        tracing::debug!(
            revoked = revoked_credentials.len(),
            "CLI credential owner notifications skipped because NATS is not connected"
        );
        return;
    };

    for credential in revoked_credentials {
        let org_ids = match sqlx::query_scalar::<_, sqlx::types::Uuid>(
            r#"SELECT organization_id FROM organization_members WHERE user_id = $1"#,
        )
        .bind(credential.user_id)
        .fetch_all(pool)
        .await
        {
            Ok(org_ids) => org_ids,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    user_id = %credential.user_id,
                    cli_tool = %credential.cli_tool,
                    "failed to resolve orgs for revoked CLI credential notification"
                );
                continue;
            }
        };

        if org_ids.is_empty() {
            tracing::debug!(
                user_id = %credential.user_id,
                cli_tool = %credential.cli_tool,
                "revoked CLI credential has no organization memberships to notify"
            );
            continue;
        }

        let event_id = sqlx::types::Uuid::now_v7();
        let message = serde_json::json!({
            "type": "credential:status_update",
            "payload": {
                "action": "credential.revoked",
                "eventId": event_id,
                "credential": {
                    "ownerUserId": credential.user_id,
                    "cliTool": credential.cli_tool,
                    "status": "expired",
                    "reason": credential.reason,
                    "revokedAt": credential.revoked_at.to_rfc3339(),
                },
            },
        });
        let payload = match serde_json::to_vec(&message) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    user_id = %credential.user_id,
                    cli_tool = %credential.cli_tool,
                    "failed to serialize revoked CLI credential notification"
                );
                continue;
            }
        };

        for org_id in org_ids {
            let subject = format!("broadcast.{org_id}");
            if let Err(err) = client.publish(subject.clone(), payload.clone().into()).await {
                tracing::warn!(
                    error = %err,
                    %subject,
                    user_id = %credential.user_id,
                    cli_tool = %credential.cli_tool,
                    "failed to publish revoked CLI credential notification"
                );
            }
        }
    }
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM, then notify background tasks.
async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
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

    let _ = shutdown_tx.send(true);
    tracing::info!("Shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::{RunMode, parse_run_mode};

    #[test]
    fn parse_run_mode_defaults_to_serve() {
        let args = vec!["agentforge-server".to_string()];
        assert_eq!(parse_run_mode(&args).unwrap(), RunMode::Serve);
    }

    #[test]
    fn parse_run_mode_accepts_migrate_only() {
        let args = vec!["agentforge-server".to_string(), "--migrate-only".to_string()];
        assert_eq!(parse_run_mode(&args).unwrap(), RunMode::MigrateOnly);
    }

    #[test]
    fn parse_run_mode_rejects_unknown_flags() {
        let args = vec!["agentforge-server".to_string(), "--wat".to_string()];
        assert!(parse_run_mode(&args).is_err());
    }
}
