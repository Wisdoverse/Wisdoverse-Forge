//! Health check endpoints.
//!
//! - `GET /health` — lightweight liveness probe (no dependencies).
//! - `GET /api/health` — deep readiness probe (checks DB, Redis, NATS).

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use tokio::sync::RwLock;

use agentforge_auth::JwtManager;
use agentforge_core::{AgentId, AppConfig, AppResult, TenantScope};
use agentforge_infra::{NatsClient, ObjectStorageClient, RedisClient};
use agentforge_platform::DockerClient;

pub use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::domain::system::{HealthDependencyChecks, health_response};
use crate::mcp::McpAgentTools;
use crate::services::billing::BillingGateway;
use crate::services::email::EmailSender;
use crate::services::system::HealthReadinessService;

/// Shared application state passed to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<AppConfig>,
    pub jwt: Arc<JwtManager>,
    pub redis: Arc<RwLock<RedisClient>>,
    pub nats: Arc<NatsClient>,
    pub object_storage: Arc<ObjectStorageClient>,
    pub billing_gateway: Arc<dyn BillingGateway>,
    pub email_sender: Arc<dyn EmailSender>,
    /// Optional command-bus override for HTTP integration tests that need to
    /// exercise the CLI-agent route without a live NATS broker.
    pub agent_command_bus: Option<Arc<dyn crate::services::agent_commands::AgentCommandBus>>,
    /// Optional Docker client — may not have Docker access in all environments.
    pub docker: Option<Arc<DockerClient>>,
    /// Optional MCP tool surface used by the Rust orchestrator.
    pub mcp_tools: Option<Arc<dyn McpAgentTools>>,
    pub mcp_internal_token: Option<String>,
    /// 32-byte AES-256 key for decrypting `user_cli_credentials` /
    /// `user_llm_configs`. `None` disables tier-1/2 credential injection; the
    /// system-wide fallback keys on `config` still apply.
    pub encryption_key: Option<[u8; 32]>,
    /// Shared in-memory PKCE state store. Used by the CLI auth proxy only
    /// when Redis is not configured — must live on `AppState` (not inside
    /// `CliAuthProxyService::new`) so `authorize` and the later
    /// `complete_manual` / `server_callback` requests hit the same store.
    pub cli_auth_memory_store: Arc<crate::services::cli_auth_proxy::MemoryStateStore>,
    /// Prometheus metrics handle. Render output via `handle.render()` to serve
    /// the `/metrics` scrape endpoint. Wrapped in `Arc` so sub-state extraction
    /// via [`FromRef`] stays cheap and the handle can be shared with background
    /// tasks if/when we introduce them.
    pub prometheus_handle: Arc<PrometheusHandle>,
    /// Handle to the NATS auth callout worker's revocation surface (issue
    /// #38 phase 2). `Some(...)` only when NATS is configured AND the
    /// callout worker started successfully; `None` otherwise so dev
    /// deployments without a configured callout still boot. Handlers on
    /// the stop-agent / admin-delete paths call `.as_ref()?.revoke(id)`
    /// to target a specific agent's live NATS connection — KICK is
    /// best-effort; the 15-min JWT TTL is the real correctness boundary.
    pub auth_callout: Option<Arc<crate::services::auth_callout::AuthCalloutService>>,
    /// Per-request LLM provider factory. Does NOT hold API keys — keys are
    /// resolved per request from `UserLlmConfigRepository` so a multi-user
    /// deployment doesn't leak one operator's credentials to every tenant.
    pub llm_factory: Arc<agentforge_llm::LlmProviderFactory>,
    /// Shared context resolver used by orchestration assignment and later
    /// preview/envelope routes. Holds only read-side caches.
    pub context_resolver: Arc<crate::services::context_resolver::ContextResolverService>,
    /// Deployment kill switches for the governed context rollout. The
    /// org-scoped `feature_flags` table can narrow an enabled deployment flag;
    /// a false deployment flag always wins for rollback.
    pub context_features: ContextFeatureFlags,
    /// Map of in-flight SSE prompt streams keyed by agent. The `oneshot::Sender`
    /// signals the generator inside `PromptService::stream` to flush its partial
    /// buffer and persist an assistant row with `finish_reason="interrupted"`.
    ///
    /// Collision policy: the `POST /agents/:id/prompt` route rejects a second
    /// concurrent prompt for the same agent with 409 Conflict ("agent_busy").
    /// Callers that want to pre-empt an active stream must first call
    /// `POST /agents/:id/prompt/interrupt` — which fires the oneshot on their
    /// behalf — and then submit the new prompt once the entry is cleared.
    ///
    /// Cleanup: the route installs an `InflightGuard` that removes the entry
    /// synchronously on stream drop (normal completion, client disconnect,
    /// or server-side interrupt). Held under `std::sync::Mutex` rather than
    /// `tokio::sync::Mutex` so `Drop` can do the cleanup without spawning.
    pub inflight_prompts: Arc<std::sync::Mutex<HashMap<AgentId, tokio::sync::oneshot::Sender<()>>>>,
    /// Read-only snapshot of the CLI agent-image auto-updater's latest per-tool
    /// state, served by `GET /admin/cli-images`. Always present (built in
    /// `main`); the map is empty until the (default-off) worker runs a tick.
    /// Deployment-global — image state is per host, not per org — so it carries
    /// no tenant scope.
    pub cli_image_status: Arc<agentforge_jobs::CliImageUpdateStatus>,
    /// Single-flight set of tool names with an in-progress `POST
    /// /admin/cli-images/{tool}/roll`. Prevents two concurrent rolls of the same
    /// tool (and a roll racing the updater's re-tag). Deployment-global.
    pub cli_image_roll_inflight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl AppState {
    pub async fn context_feature_enabled(&self, scope: &TenantScope, feature: ContextFeature) -> AppResult<bool> {
        self.context_feature_service().is_enabled(scope, feature).await
    }
}

pub async fn ensure_context_feature_enabled(
    state: &AppState,
    scope: &TenantScope,
    feature: ContextFeature,
) -> AppResult<()> {
    state.context_feature_service().ensure_enabled(scope, feature).await
}

/// Allow handlers to extract just the Prometheus handle via
/// `State(handle): State<Arc<PrometheusHandle>>`, which keeps the metrics
/// route testable without a fully-wired `AppState`.
impl FromRef<AppState> for Arc<PrometheusHandle> {
    fn from_ref(state: &AppState) -> Self {
        state.prometheus_handle.clone()
    }
}

/// `GET /health` — lightweight liveness check.
///
/// Returns `200 OK` immediately. Used by infrastructure probes (Docker, k8s)
/// to verify the process is alive and accepting connections.
pub async fn health() -> impl IntoResponse {
    Json(health_response())
}

/// `GET /api/health` — deep readiness check.
///
/// Verifies database connectivity. Redis and NATS are optional (graceful degradation
/// per CLAUDE.md: "Circuit breaker: Redis is optional"). The response contract
/// is owned by the system domain boundary.
pub async fn health_ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = agentforge_db::check_health(&state.pool).await;
    let redis_ok = state.redis.write().await.check_health().await;
    let nats_ok = state.nats.check_health().await;

    // Docker is optional — absence is not degraded, but if present, check connectivity.
    let docker_ok = match &state.docker {
        Some(docker) => docker.check_health().await,
        None => true,
    };

    // DB is always required. NATS is required once configured because
    // orchestration/event delivery depend on it; deployments that do not use
    // NATS can still omit NATS_URL and remain ready.
    let nats_required = state.config.nats_url.is_some();
    let readiness = HealthReadinessService::evaluate(
        HealthDependencyChecks { database: db_ok, redis: redis_ok, nats: nats_ok, docker: docker_ok },
        nats_required,
    );
    let http_status = if readiness.is_ready() { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };

    (http_status, Json(readiness.response()))
}
