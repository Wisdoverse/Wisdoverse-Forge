//! Unit 3.1 coverage for the runtime capability registry read-cache.

use agentforge_api::repositories::runtime_capability::RuntimeCapabilityRepository;
use agentforge_api::services::runtime_capability_registry::RuntimeCapabilityRegistryService;
use agentforge_core::{CliToolKind, RuntimeCapability, RuntimeKind};
use sqlx::PgPool;

fn registry(pool: PgPool) -> RuntimeCapabilityRegistryService {
    RuntimeCapabilityRegistryService::new(RuntimeCapabilityRepository::new(pool))
}

#[sqlx::test(migrations = "../db/migrations")]
async fn startup_refresh_seeds_runtime_capabilities_from_typed_matrix(pool: PgPool) {
    let repo = RuntimeCapabilityRepository::new(pool.clone());
    let service = registry(pool);

    service.refresh_from_code().await.expect("refresh capabilities from code");

    let rows = repo.list_all().await.expect("list seeded capabilities");
    assert_eq!(rows.len(), RuntimeCapability::all().len());

    let capability = service.for_cli_tool(CliToolKind::Claude, RuntimeKind::Container).await;
    assert_eq!(capability, RuntimeCapability::for_cli_tool(CliToolKind::Claude, RuntimeKind::Container));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn lookup_without_startup_refresh_returns_fallback_and_does_not_seed(pool: PgPool) {
    let repo = RuntimeCapabilityRepository::new(pool.clone());
    let service = registry(pool);

    let fallback = service.for_cli_tool(CliToolKind::Codex, RuntimeKind::Api).await;

    assert_eq!(fallback.cli_tool, Some(CliToolKind::Codex));
    assert_eq!(fallback.runtime_kind, RuntimeKind::Api);
    assert_eq!(fallback.max_context_tokens, 1);
    assert!(!fallback.supports_skills_mount);
    assert!(!fallback.supports_hooks);
    assert!(!fallback.supports_subagents);
    assert!(!fallback.supports_mcp_bridge);
    assert!(!fallback.supports_terminal);
    assert_eq!(repo.count().await.expect("count rows"), 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn malformed_seed_profile_refuses_to_load_cache(pool: PgPool) {
    sqlx::query(
        r#"INSERT INTO runtime_capabilities (
               cli_tool, runtime_kind, max_context_tokens,
               supports_skills_mount, supports_hooks, supports_subagents,
               supports_mcp_bridge, supports_terminal, capability_profile
           )
           VALUES (
               'claude', 'container', 200000,
               TRUE, TRUE, TRUE, TRUE, TRUE,
               '{"cli_tool":"claude","runtime_kind":"container"}'::jsonb
           )"#,
    )
    .execute(&pool)
    .await
    .expect("insert malformed profile");

    let err = registry(pool).refresh_from_code().await.expect_err("malformed row should fail startup refresh");
    let message = err.kind.to_string();
    assert!(message.contains("invalid capability_profile for claude/container"), "{message}");
    assert!(message.contains("max_context_tokens"), "{message}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn divergent_seed_refuses_startup_and_preserves_previous_cache(pool: PgPool) {
    let service = registry(pool.clone());
    service.refresh_from_code().await.expect("initial refresh");
    let before = service.for_cli_tool(CliToolKind::Claude, RuntimeKind::Container).await;

    sqlx::query(
        r#"UPDATE runtime_capabilities
              SET max_context_tokens = 42,
                  capability_profile = jsonb_set(capability_profile, '{max_context_tokens}', '42'::jsonb, false)
            WHERE cli_tool = 'claude'
              AND runtime_kind = 'container'"#,
    )
    .execute(&pool)
    .await
    .expect("corrupt seeded row");

    let err = service.refresh_from_code().await.expect_err("divergent row should fail startup refresh");
    let message = err.kind.to_string();
    assert!(message.contains("runtime_capabilities row for claude/container diverges from code"), "{message}");
    assert!(message.contains("051_runtime_capabilities.sql"), "{message}");

    let after = service.for_cli_tool(CliToolKind::Claude, RuntimeKind::Container).await;
    assert_eq!(after, before, "failed refresh must not replace the last valid cache");
}
