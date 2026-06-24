use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use axum::Router;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::time::Duration;

use crate::audit::{MemoryStore as MemoryAuditStore, PgAuditStore, Store as AuditStore};
use crate::auth::{AgentDirectory, MemoryAgentDirectory, PgAgentDirectory, Provisioner, SessionManager};
use crate::config::Config;
use crate::knowledge::KnowledgeService;
use crate::mcp::McpServer;
use crate::mcp::client::{CreateSessionArgs, CreateSessionResult, OutboundMcp, OutboundMcpClient, SessionStatusResult};
use crate::metrics::{MemoryMetricsStore, MetricsCache, PgMetricsStore, Store as MetricsStore};
use crate::realtime::Broadcaster;
use crate::review::{MemoryStore as MemoryReviewStore, PgReviewStore, Store as ReviewStore};
use crate::task::{MemoryStore as MemoryTaskStore, PgTaskStore, Store as TaskStore};
use crate::team::{MemoryStore as MemoryTeamStore, PgTeamStore, Store as TeamStore};
use crate::workflow::{
    MemoryStore as MemoryWorkflowStore, MemoryWorkflowRuntime, PgWorkflowStore, Store as WorkflowStore,
    WorkflowService, WorkflowWorkerHandle, build_workflow_runtime,
};

type AuthServices = (Option<Arc<SessionManager>>, Option<Arc<Provisioner>>);

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: Option<PgPool>,
    pub knowledge: Option<Arc<KnowledgeService>>,
    pub audit_store: Option<Arc<dyn AuditStore>>,
    pub task_store: Option<Arc<dyn TaskStore>>,
    pub review_store: Option<Arc<dyn ReviewStore>>,
    pub team_store: Option<Arc<dyn TeamStore>>,
    pub workflow_store: Option<Arc<dyn WorkflowStore>>,
    pub workflow_service: Option<Arc<WorkflowService>>,
    pub sessions: Option<Arc<SessionManager>>,
    pub provisioner: Option<Arc<Provisioner>>,
    pub mcp_server: Option<Arc<McpServer>>,
    pub outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    pub agent_directory: Option<Arc<dyn AgentDirectory>>,
    pub metrics_store: Option<Arc<dyn MetricsStore>>,
    pub metrics_cache: Arc<MetricsCache>,
    pub broadcaster: Arc<Broadcaster>,
    pub workflow_runtime: crate::workflow::WorkflowRuntimeStatus,
    pub ready: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Arc::new(Config::default()),
            pool: None,
            knowledge: None,
            audit_store: None,
            task_store: None,
            review_store: None,
            team_store: None,
            workflow_store: None,
            workflow_service: None,
            sessions: None,
            provisioner: None,
            mcp_server: None,
            outbound_mcp: None,
            agent_directory: None,
            metrics_store: None,
            metrics_cache: Arc::new(MetricsCache::with_default_ttl()),
            broadcaster: Arc::new(Broadcaster::new()),
            workflow_runtime: crate::workflow::WorkflowRuntimeStatus::Disabled,
            ready: false,
        }
    }
}

impl AppState {
    pub fn test() -> Self {
        Self::default()
    }

    pub fn test_ready() -> Self {
        Self::test_with_config(Config::default())
    }

    pub fn test_internal_token(token: &str) -> Self {
        Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() })
    }

    pub fn test_with_jwt_signing_key(signing_key: &str) -> Self {
        Self::test_with_config(Config { jwt_signing_key: Some(signing_key.to_string()), ..Config::default() })
    }

    pub fn test_with_auth(signing_key: &str, internal_token: &str) -> Self {
        Self::test_with_config(Config {
            internal_token: Some(internal_token.to_string()),
            jwt_signing_key: Some(signing_key.to_string()),
            ..Config::default()
        })
    }

    pub fn test_audit_internal_token(token: &str, _org_id: &str) -> Self {
        let mut state = Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() });
        state.audit_store = Some(Arc::new(MemoryAuditStore::new()));
        state
    }

    pub fn test_team_internal_token(token: &str, _org_id: &str, _user_id: &str) -> Self {
        let mut state = Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() });
        state.team_store = Some(Arc::new(MemoryTeamStore::new()));
        state
    }

    pub fn test_workflow_internal_token(token: &str, _org_id: &str, _user_id: &str) -> Self {
        let mut state = Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() });
        let workflow_store: Arc<dyn WorkflowStore> = Arc::new(MemoryWorkflowStore::new());
        let workflow_runtime = Arc::new(MemoryWorkflowRuntime::new(workflow_store.clone()));
        state.workflow_service = Some(Arc::new(WorkflowService::new(workflow_store.clone(), workflow_runtime)));
        state.workflow_store = Some(workflow_store);
        state
    }

    pub fn test_task_internal_token(token: &str, _org_id: &str, _user_id: &str) -> Self {
        let mut state = Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() });
        state.task_store = Some(Arc::new(MemoryTaskStore::new()));
        state.agent_directory = Some(Arc::new(MemoryAgentDirectory::new()));
        state
    }

    pub fn test_review_internal_token(token: &str, _org_id: &str, _user_id: &str) -> Self {
        let mut state = Self::test_with_config(Config { internal_token: Some(token.to_string()), ..Config::default() });
        let task_store: Arc<dyn TaskStore> = Arc::new(MemoryTaskStore::new());
        let review_store: Arc<dyn ReviewStore> = Arc::new(MemoryReviewStore::new());
        let agent_directory: Arc<dyn AgentDirectory> = Arc::new(MemoryAgentDirectory::new());
        let metrics_store: Arc<dyn MetricsStore> =
            Arc::new(MemoryMetricsStore::new(task_store.clone(), review_store.clone(), Some(agent_directory.clone())));
        state.task_store = Some(task_store);
        state.review_store = Some(review_store);
        state.audit_store = Some(Arc::new(MemoryAuditStore::new()));
        state.agent_directory = Some(agent_directory);
        state.metrics_store = Some(metrics_store);
        state
    }

    pub fn test_mcp_internal_token(token: &str, org_id: &str) -> Self {
        Self::test_with_config(Config {
            internal_token: Some(token.to_string()),
            mcp_server_enabled: true,
            mcp_server_org: org_id.to_string(),
            ..Config::default()
        })
    }

    /// Build a Postgres-backed test state for the MCP review-tool contract tests.
    ///
    /// Wires the real `PgReviewStore` / `PgTaskStore` / `PgAuditStore` and a
    /// Postgres `Provisioner` (so internal-token requests resolve to real
    /// participant ids), enables the MCP server, and gates it on `token`. This is
    /// the `#[sqlx::test]` analogue of `test_review_internal_token`, driving the
    /// `/mcp` endpoint against a real database.
    pub fn test_mcp_pg(pool: PgPool, token: &str, org_id: &str) -> Self {
        let config = Arc::new(Config {
            internal_token: Some(token.to_string()),
            mcp_server_enabled: true,
            mcp_server_org: org_id.to_string(),
            ..Config::default()
        });
        let provisioner = Some(Arc::new(Provisioner::postgres(pool.clone())));
        let task_store = build_task_store(Some(&pool));
        let review_store = build_review_store(Some(&pool));
        let audit_store = build_audit_store(Some(&pool));
        let mcp_server = build_mcp_server(config.as_ref());
        Self {
            config,
            pool: Some(pool),
            provisioner,
            task_store,
            review_store,
            audit_store,
            mcp_server,
            ready: true,
            ..Self::default()
        }
    }

    /// Like [`AppState::test_mcp_pg`] but also wires a JWT `SessionManager` from
    /// `signing_key` so tests can mint a session token and exercise the MCP review
    /// tools' `"human"` actor-type branch (session JWT), not just the internal
    /// token's `"system"` branch.
    pub fn test_mcp_pg_with_sessions(pool: PgPool, token: &str, org_id: &str, signing_key: &str) -> Self {
        let config = Arc::new(Config {
            internal_token: Some(token.to_string()),
            jwt_signing_key: Some(signing_key.to_string()),
            mcp_server_enabled: true,
            mcp_server_org: org_id.to_string(),
            ..Config::default()
        });
        let (sessions, provisioner) =
            build_auth_services(config.as_ref(), Some(&pool)).expect("test mcp auth services");
        let task_store = build_task_store(Some(&pool));
        let review_store = build_review_store(Some(&pool));
        let audit_store = build_audit_store(Some(&pool));
        let mcp_server = build_mcp_server(config.as_ref());
        Self {
            config,
            pool: Some(pool),
            sessions,
            provisioner,
            task_store,
            review_store,
            audit_store,
            mcp_server,
            ready: true,
            ..Self::default()
        }
    }

    pub fn with_outbound_mcp(mut self, outbound_mcp: Arc<dyn OutboundMcp>) -> Self {
        self.outbound_mcp = Some(outbound_mcp);
        self
    }

    pub fn with_outbound_mcp_test_success(self, session_id: &str) -> Self {
        self.with_outbound_mcp(Arc::new(FixedOutboundMcp::successful(session_id)))
    }

    pub fn with_outbound_mcp_test_failure(self, error: &str) -> Self {
        self.with_outbound_mcp(Arc::new(FailingOutboundMcp { error: error.to_string() }))
    }

    pub fn with_metrics_store(mut self, metrics_store: Arc<dyn MetricsStore>) -> Self {
        self.metrics_store = Some(metrics_store);
        self
    }

    fn test_with_config(config: Config) -> Self {
        let config = Arc::new(config);
        let knowledge = Arc::new(KnowledgeService::test());
        let (sessions, provisioner) = build_auth_services(config.as_ref(), None).expect("test auth services");
        let mcp_server = build_mcp_server(config.as_ref());

        Self {
            config,
            pool: None,
            knowledge: Some(knowledge),
            audit_store: None,
            task_store: None,
            review_store: None,
            team_store: None,
            workflow_store: None,
            workflow_service: None,
            sessions,
            provisioner,
            mcp_server,
            outbound_mcp: None,
            agent_directory: None,
            metrics_store: None,
            metrics_cache: Arc::new(MetricsCache::with_default_ttl()),
            broadcaster: Arc::new(Broadcaster::new()),
            workflow_runtime: crate::workflow::WorkflowRuntimeStatus::Disabled,
            ready: true,
        }
    }

    pub async fn live(config: Config) -> anyhow::Result<Self> {
        let (state, _workflow_worker) = Self::live_with_runtime(config).await?;
        Ok(state)
    }

    pub async fn live_with_runtime(config: Config) -> anyhow::Result<(Self, Option<WorkflowWorkerHandle>)> {
        let config = Arc::new(config);
        let pool = if config.database_url.is_empty() {
            None
        } else {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(Duration::from_secs(5))
                .connect(&config.database_url)
                .await?;
            crate::migrations::run_migrations(&pool).await.context("run orchestrator migrations")?;
            Some(pool)
        };

        let knowledge = match &pool {
            Some(pool) => Some(Arc::new(KnowledgeService::live(pool.clone(), config.as_ref()).await?)),
            None => None,
        };
        let (sessions, provisioner) = build_auth_services(config.as_ref(), pool.as_ref())?;
        let audit_store = build_audit_store(pool.as_ref());
        let task_store = build_task_store(pool.as_ref());
        let review_store = build_review_store(pool.as_ref());
        let team_store = build_team_store(pool.as_ref());
        let workflow_store = build_workflow_store(pool.as_ref());
        let outbound_mcp = build_outbound_mcp(config.as_ref())?;
        let agent_directory = build_agent_directory(pool.as_ref());
        let metrics_store =
            build_metrics_store(pool.as_ref(), task_store.clone(), review_store.clone(), agent_directory.clone());
        // Single shared Broadcaster — also handed to WorkflowActivities and the
        // review escalation reaper below, so wiring the NATS relay here makes
        // BOTH workflow:* and review.escalated relay to the browser. Graceful:
        // no NATS_URL (or an unreachable server) → in-process-only.
        let broadcaster = Arc::new(build_broadcaster(config.as_ref()).await);
        let (workflow_components, workflow_runtime) = build_workflow_runtime(
            config.as_ref(),
            workflow_store.clone(),
            outbound_mcp.clone(),
            Some(broadcaster.clone()),
        )
        .await;
        let workflow_service = workflow_components.as_ref().map(|components| components.service.clone());
        let workflow_worker = workflow_components.map(|components| components.worker);
        let mcp_server = build_mcp_server(config.as_ref());

        Ok((
            Self {
                config,
                pool,
                ready: knowledge.is_some(),
                knowledge,
                audit_store,
                task_store,
                review_store,
                team_store,
                workflow_store,
                workflow_service,
                sessions,
                provisioner,
                mcp_server,
                outbound_mcp,
                agent_directory,
                metrics_store,
                metrics_cache: Arc::new(MetricsCache::with_default_ttl()),
                broadcaster,
                workflow_runtime,
            },
            workflow_worker,
        ))
    }

    pub fn router(self) -> Router {
        crate::router::create_router(self)
    }
}

fn build_auth_services(config: &Config, pool: Option<&PgPool>) -> anyhow::Result<AuthServices> {
    let sessions = match config.jwt_signing_key.as_deref() {
        Some(signing_key) => {
            let signing_key = hex::decode(signing_key).context("decode ORCHESTRATOR_JWT_SIGNING_KEY")?;
            Some(Arc::new(SessionManager::new(signing_key)?))
        }
        None => None,
    };

    let provisioner = match pool {
        Some(pool) => Some(Arc::new(Provisioner::postgres(pool.clone()))),
        None if sessions.is_some() => Some(Arc::new(Provisioner::new())),
        None => None,
    };

    Ok((sessions, provisioner))
}

/// Build the shared realtime `Broadcaster`, wiring the NATS relay when
/// `config.nats_url` is set and reachable. Graceful degradation: no URL, an
/// unparseable URL, or a connection failure all fall back to an in-process-only
/// `Broadcaster` (today's behavior) — the orchestrator must boot in
/// environments without NATS, so this never errors.
///
/// `connect_nats` connects with whatever credential is encoded in the URL. The
/// operator MUST point this at a BACKEND NATS URL (same as the main API/jobs
/// publishers), not a per-agent sidecar credential: `broadcast.>` publish is
/// denied to sidecar JWTs by the NATS auth callout.
async fn build_broadcaster(config: &Config) -> Broadcaster {
    let Some(url) = config.nats_url.as_deref() else {
        tracing::info!("orchestrator realtime NATS relay disabled (no NATS_URL); running in-process-only");
        return Broadcaster::new();
    };

    match agentforge_infra::nats::connect_nats(url).await {
        Ok(client) => {
            tracing::info!("orchestrator realtime NATS relay enabled");
            Broadcaster::with_nats(client)
        }
        Err(err) => {
            tracing::warn!(error = %err, "orchestrator NATS connection failed; realtime relay disabled (in-process-only)");
            Broadcaster::new()
        }
    }
}

fn build_mcp_server(config: &Config) -> Option<Arc<McpServer>> {
    if !config.mcp_server_enabled {
        return None;
    }

    let org_id = if config.mcp_server_org.is_empty() { "default".to_string() } else { config.mcp_server_org.clone() };
    Some(Arc::new(McpServer::new(org_id)))
}

fn build_outbound_mcp(config: &Config) -> anyhow::Result<Option<Arc<dyn OutboundMcp>>> {
    if config.mcp_endpoint.trim().is_empty() {
        return Ok(None);
    }

    let client = Arc::new(OutboundMcpClient::new(config.mcp_endpoint.clone(), config.mcp_token.clone())?);
    Ok(Some(client as Arc<dyn OutboundMcp>))
}

fn build_agent_directory(pool: Option<&PgPool>) -> Option<Arc<dyn AgentDirectory>> {
    pool.map(|pool| Arc::new(PgAgentDirectory::new(pool.clone())) as Arc<dyn AgentDirectory>)
}

fn build_metrics_store(
    pool: Option<&PgPool>,
    task_store: Option<Arc<dyn TaskStore>>,
    review_store: Option<Arc<dyn ReviewStore>>,
    agent_directory: Option<Arc<dyn AgentDirectory>>,
) -> Option<Arc<dyn MetricsStore>> {
    if let Some(pool) = pool {
        return Some(Arc::new(PgMetricsStore::new(pool.clone())) as Arc<dyn MetricsStore>);
    }

    match (task_store, review_store) {
        (Some(task_store), Some(review_store)) => {
            Some(Arc::new(MemoryMetricsStore::new(task_store, review_store, agent_directory)) as Arc<dyn MetricsStore>)
        }
        _ => None,
    }
}

fn build_task_store(pool: Option<&PgPool>) -> Option<Arc<dyn TaskStore>> {
    pool.map(|pool| Arc::new(PgTaskStore::new(pool.clone())) as Arc<dyn TaskStore>)
}

fn build_review_store(pool: Option<&PgPool>) -> Option<Arc<dyn ReviewStore>> {
    pool.map(|pool| Arc::new(PgReviewStore::new(pool.clone())) as Arc<dyn ReviewStore>)
}

fn build_audit_store(pool: Option<&PgPool>) -> Option<Arc<dyn AuditStore>> {
    pool.map(|pool| Arc::new(PgAuditStore::new(pool.clone())) as Arc<dyn AuditStore>)
}

fn build_team_store(pool: Option<&PgPool>) -> Option<Arc<dyn TeamStore>> {
    pool.map(|pool| Arc::new(PgTeamStore::new(pool.clone())) as Arc<dyn TeamStore>)
}

fn build_workflow_store(pool: Option<&PgPool>) -> Option<Arc<dyn WorkflowStore>> {
    pool.map(|pool| Arc::new(PgWorkflowStore::new(pool.clone())) as Arc<dyn WorkflowStore>)
}

struct FixedOutboundMcp {
    session_id: String,
}

struct FailingOutboundMcp {
    error: String,
}

#[async_trait]
impl OutboundMcp for FailingOutboundMcp {
    async fn session_create(&self, _args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult> {
        Err(anyhow::anyhow!("{}", self.error))
    }

    async fn session_prompt(&self, _org_id: &str, _agent_id: &str, _prompt: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("{}", self.error))
    }

    async fn session_destroy(&self, _org_id: &str, _agent_id: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("{}", self.error))
    }

    async fn session_status(&self, _org_id: &str, _agent_id: &str) -> anyhow::Result<SessionStatusResult> {
        Err(anyhow::anyhow!("{}", self.error))
    }
}

impl FixedOutboundMcp {
    fn successful(session_id: &str) -> Self {
        Self { session_id: session_id.to_string() }
    }
}

#[async_trait]
impl OutboundMcp for FixedOutboundMcp {
    async fn session_create(&self, args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult> {
        Ok(CreateSessionResult {
            agent_id: self.session_id.clone(),
            status: "created".to_string(),
            name: args.name.unwrap_or_else(|| self.session_id.clone()),
        })
    }

    async fn session_prompt(&self, _org_id: &str, _agent_id: &str, _prompt: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn session_destroy(&self, _org_id: &str, _agent_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn session_status(&self, _org_id: &str, _agent_id: &str) -> anyhow::Result<SessionStatusResult> {
        Ok(SessionStatusResult { agent_id: self.session_id.clone(), status: "idle".to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn build_broadcaster_without_nats_url_degrades_gracefully_to_in_process() {
        // Graceful-degradation arm: with no NATS_URL the orchestrator must still
        // boot and return a working in-process-only Broadcaster — no panic, no
        // error, no NATS dependency. (The connect-failure arm needs a network
        // and is intentionally not exercised here.)
        let config = Config { nats_url: None, ..Config::default() };
        let broadcaster = build_broadcaster(&config).await;
        // In-process fan-out still works with zero subscribers / no NATS.
        assert_eq!(broadcaster.client_count(), 0);
    }
}
