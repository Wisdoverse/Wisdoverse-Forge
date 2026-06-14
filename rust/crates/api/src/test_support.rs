//! Full-app test harness for integration tests — mints a real `AppState` + full
//! `create_router(state)` with a mock LLM provider and a bearer-token rewrite
//! middleware. Gated so this only compiles for test/dev builds.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use futures::FutureExt;
use secrecy::SecretString;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use agentforge_auth::JwtManager;
use agentforge_core::{
    AgentId, AppConfig, AppResult, NatsCalloutConfig, OrgId, ProjectId, StripeConfig, TeamId, TenantScope, UserId,
    WorkspaceId, crypto,
};
use agentforge_infra::{NatsClient, ObjectStorageClient, RedisClient};
use agentforge_llm::LlmProviderFactory;

use crate::health::{AppState, ContextFeatureFlags};
use crate::services::agent_commands::AgentCommandBus;
use crate::services::billing::DisabledBillingGateway;
use crate::services::cli_auth_proxy::MemoryStateStore;
use crate::services::email::{DisabledEmailSender, EmailSender};

/// Test JWT secret — 32+ bytes so `JwtManager::new` accepts it.
pub const TEST_JWT_SECRET: &str = "provider-prompt-integration-test-secret-32bytes!";
pub const TEST_LLM_ENCRYPTION_KEY: [u8; 32] = [0x42; 32];

#[derive(Default)]
struct NoopCommandBus;

impl AgentCommandBus for NoopCommandBus {
    fn publish_json<'a>(&'a self, _subject: &'a str, _payload: Value) -> futures::future::BoxFuture<'a, AppResult<()>> {
        async move { Ok(()) }.boxed()
    }
}

/// Build a minimal `AppConfig` wired for tests (no NATS, no Redis).
fn test_app_config(database_url: &str) -> AppConfig {
    AppConfig {
        port: 4003,
        host: "0.0.0.0".to_string(),
        database_url: database_url.to_string(),
        redis_url: None,
        presence_redis_enabled: false,
        nats_url: None,
        nats_agent_url: None,
        nats_container_url: None,
        nats_callout: NatsCalloutConfig::default(),
        stripe: StripeConfig::default(),
        jwt_secret: SecretString::from(TEST_JWT_SECRET.to_string()),
        jwt_expiry_seconds: 3600,
        environment: "test".to_string(),
        log_level: "warn".to_string(),
        cors_origin: None,
        static_dir: None,
        container_server_url: None,
        ollama_base_url: None,
        llm_encryption_key: None,
        container_anthropic_api_key: None,
        container_google_api_key: None,
        container_openai_api_key: None,
        codex_default_model: "gpt-5.5".to_string(),
        oauth_mount_dir: None,
        storage_provider: "local".to_string(),
        storage_local_path: std::env::temp_dir()
            .join(format!("agentforge-test-attachments-{}", Uuid::new_v4()))
            .to_string_lossy()
            .to_string(),
        storage_max_file_size: 10 * 1024 * 1024,
        storage_max_files_per_session: 20,
        storage_signed_url_expiry: 3600,
        minio_endpoint: None,
        minio_access_key: None,
        minio_secret_key: None,
        minio_bucket: "agentforge".to_string(),
        minio_use_ssl: false,
        minio_region: None,
        credential_sync_enabled: false,
        cli_auth_proxy_openai_client_id: None,
        cli_auth_proxy_openai_client_secret: None,
        cli_auth_proxy_openai_auth_endpoint: None,
        cli_auth_proxy_openai_token_endpoint: None,
        app_url: None,
        cli_auth_proxy_revoke_threshold: 2,
        smtp_host: None,
        smtp_port: None,
        smtp_user: None,
        smtp_password: None,
        smtp_from: None,
        smtp_secure: false,
        allow_plaintext_host_nats: false,
        host_join_binary_base_url: None,
        cli_image_auto_update_enabled: false,
        cli_image_auto_update_interval_secs: 900,
        cli_image_prune_enabled: false,
        cli_image_claude_auto_build: false,
        cli_image_npm_registry: None,
        project_clone_worker_enabled: false,
        project_clone_image: None,
        project_clone_secret_root: None,
        project_clone_timeout_secs: 600,
        github_app_id: None,
        github_app_installation_id: None,
        github_app_private_key: None,
        github_app_repo: None,
    }
}

/// Build a full `AppState` wired for tests:
/// - Real `PgPool` (caller supplies from `#[sqlx::test]`).
/// - Mock LLM provider registered as `mock_provider_name` returning `mock_reply`.
/// - Degraded NATS/Redis (no URLs configured → returns early-success stubs).
/// - Test-only no-op command bus for CLI-agent prompt/interrupt routes.
/// - No Docker, no MCP, no auth_callout.
pub async fn app_state_with_mock_provider(pool: PgPool, mock_provider_name: &str, mock_reply: &str) -> AppState {
    app_state_with_mock_provider_and_email_sender(
        pool,
        mock_provider_name,
        mock_reply,
        Arc::new(DisabledEmailSender),
        None,
    )
    .await
}

pub async fn app_state_with_mock_provider_and_email_sender(
    pool: PgPool,
    mock_provider_name: &str,
    mock_reply: &str,
    email_sender: Arc<dyn EmailSender>,
    app_url: Option<String>,
) -> AppState {
    let prometheus_handle = crate::testing::ensure_recorder();

    // `sqlx::test` provides the pool but we don't have the URL on hand.
    // The config's `database_url` is only used by `AppConfig::from_env`-driven
    // server startup; the test runtime already owns `pool` directly.
    let mut config = test_app_config("postgres://localhost/agentforge_test");
    config.app_url = app_url;
    let config = Arc::new(config);

    let jwt = Arc::new(JwtManager::new(TEST_JWT_SECRET, 3600));
    let redis = Arc::new(RwLock::new(RedisClient::new(&config).await));
    let runtime_capability_registry =
        crate::services::runtime_capability_registry::RuntimeCapabilityRegistryService::new(
            crate::repositories::runtime_capability::RuntimeCapabilityRepository::new(pool.clone()),
        );
    runtime_capability_registry.refresh_from_code().await.expect("refresh test runtime capability registry");
    let context_resolver = Arc::new(
        crate::services::context_resolver::ContextResolverService::new(pool.clone(), runtime_capability_registry)
            .with_redis(redis.clone()),
    );
    let nats = Arc::new(NatsClient::new(&config).await);
    let object_storage = Arc::new(ObjectStorageClient::new(&config).await.expect("test object storage"));

    AppState {
        pool,
        config,
        jwt,
        redis,
        nats,
        object_storage,
        billing_gateway: Arc::new(DisabledBillingGateway),
        email_sender,
        agent_command_bus: Some(Arc::new(NoopCommandBus)),
        docker: None,
        mcp_tools: None,
        mcp_internal_token: None,
        encryption_key: Some(TEST_LLM_ENCRYPTION_KEY),
        cli_auth_memory_store: Arc::new(MemoryStateStore::default()),
        prometheus_handle,
        auth_callout: None,
        llm_factory: Arc::new(LlmProviderFactory::with_mock(mock_provider_name, mock_reply)),
        context_resolver,
        context_features: ContextFeatureFlags::all_enabled(),
        inflight_prompts: Arc::new(std::sync::Mutex::new(HashMap::new())),
        cli_image_status: Arc::new(agentforge_jobs::CliImageUpdateStatus::new()),
        cli_image_roll_inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
    }
}

/// Build a full `Router` from an `AppState` configured as above.
pub async fn test_app_with_mock_provider(pool: PgPool, mock_provider_name: &str, mock_reply: &str) -> Router {
    let state = app_state_with_mock_provider(pool, mock_provider_name, mock_reply).await;
    crate::router::create_router(state)
}

/// Mint a signed JWT for integration tests. Use `Bearer <token>` in `Authorization` header.
pub fn mint_test_jwt(org_id: Uuid, user_id: Uuid, role: &str) -> String {
    let jwt = JwtManager::new(TEST_JWT_SECRET, 3600);
    jwt.create_token(user_id, org_id, role).expect("mint test jwt")
}

/// Mint a signed JWT with active governance axes for integration tests.
pub fn mint_test_jwt_with_axes(
    org_id: Uuid,
    user_id: Uuid,
    role: &str,
    workspace_id: Option<Uuid>,
    team_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> String {
    let jwt = JwtManager::new(TEST_JWT_SECRET, 3600);
    jwt.create_token_with_axes(user_id, org_id, role, workspace_id, team_id, project_id)
        .expect("mint test jwt with axes")
}

/// Build a tenant scope for tests from explicit organization and user IDs.
///
/// Keep direct `TenantScope::new` calls centralized here so production and
/// feature code cannot accidentally bypass the auth middleware scope path.
pub fn tenant_scope_for_ids(org_id: Uuid, user_id: Uuid) -> TenantScope {
    TenantScope::new(OrgId::from(org_id), UserId::from(user_id))
}

/// Build a tenant scope with active governance axes for tests.
pub fn tenant_scope_for_ids_with_axes(
    org_id: Uuid,
    user_id: Uuid,
    workspace_id: Option<Uuid>,
    team_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> TenantScope {
    TenantScope::with_axes(
        OrgId::from(org_id),
        UserId::from(user_id),
        workspace_id.map(WorkspaceId::from),
        team_id.map(TeamId::from),
        project_id.map(ProjectId::from),
    )
}

/// Build a tenant scope for tests with random organization and user IDs.
pub fn tenant_scope() -> TenantScope {
    tenant_scope_for_ids(Uuid::new_v4(), Uuid::new_v4())
}

/// Build a tenant scope for tests with a specific user and throwaway organization.
pub fn tenant_scope_for_user(user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(Uuid::new_v4(), user_id)
}

/// Seed result returned from `seed_provider_agent`.
pub struct SeedResult {
    pub scope: TenantScope,
    pub agent_id: AgentId,
    pub org_id: OrgId,
    pub user_id: UserId,
    pub jwt: String,
}

/// Seed one org + workspace + user + membership + one provider+prompt agent.
/// Returns `(scope, agent_id, jwt_token)` suitable for a streaming prompt test.
pub async fn seed_provider_agent(pool: &PgPool, provider: &str, model: &str) -> SeedResult {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    // Workspace id can be any UUID; reusing org_id keeps it simple.
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(org_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");

    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");

    let encrypted_api_key =
        crypto::encrypt_base64(&TEST_LLM_ENCRYPTION_KEY, "test-api-key").expect("encrypt test api key");
    sqlx::query(
        "INSERT INTO user_llm_configs (user_id, provider, model, encrypted_api_key, is_default)
         VALUES ($1, $2, $3, $4, TRUE)",
    )
    .bind(user_id)
    .bind(provider)
    .bind(model)
    .bind(&encrypted_api_key)
    .execute(pool)
    .await
    .expect("seed user llm config");

    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, provider, model, status)
         VALUES ($1, $2, $2, $3, $4, $5, 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(provider)
    .bind(model)
    .execute(pool)
    .await
    .expect("seed agent");

    let jwt = mint_test_jwt(org_id, user_id, "admin");
    let scope = tenant_scope_for_ids(org_id, user_id);

    SeedResult {
        scope,
        agent_id: AgentId::from(agent_id),
        jwt,
        user_id: UserId::from(user_id),
        org_id: OrgId::from(org_id),
    }
}

/// Seed a CLI-tool agent (cli_tool = 'claude', no provider). Pass an existing
/// org_id/user_id to put the agent on the same tenant as another seed result.
pub async fn seed_cli_agent(pool: &PgPool, org_id: Uuid, user_id: Uuid, cli_tool: &str) -> AgentId {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, cli_tool, status, runtime_kind)
         VALUES ($1, $2, $2, $3, $4, 'idle', 'container')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(cli_tool)
    .execute(pool)
    .await
    .expect("seed cli agent");
    AgentId::from(agent_id)
}
