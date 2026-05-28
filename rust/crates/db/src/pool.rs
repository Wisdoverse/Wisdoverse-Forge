//! Database pool setup and health checking.
//!
//! Provides functions to create a configured PostgreSQL connection pool
//! and check database connectivity.

use crate::manifest::{ManifestError, verify_manifest};
use agentforge_core::AppConfig;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Create a PostgreSQL connection pool from the application configuration.
///
/// Pool settings:
/// - Max 20 connections (matches typical PostgreSQL `max_connections / workers` ratio)
/// - Min 2 connections kept alive (reduces cold-start latency)
/// - 5s acquire timeout (fail fast on pool exhaustion)
/// - 5min idle timeout (release unused connections)
pub async fn create_pool(config: &AppConfig) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect(&config.database_url)
        .await
}

/// Check database health by executing a simple query.
///
/// Returns `true` if the database is reachable and responsive.
/// Logs a warning with the error reason when the check fails.
pub async fn check_health(pool: &PgPool) -> bool {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool).await {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(error = %err, "database health check failed");
            false
        }
    }
}

/// Embedded migration SQL sources used for manifest verification.
///
/// Each tuple is `(filename, sql_content)`. The list is populated at compile
/// time from the same `migrations/` directory used by `sqlx::migrate!()`, so
/// the two sources are always in sync.
const MIGRATION_SOURCES: &[(&str, &str)] = &[
    ("000_legacy_prepare.sql",                      include_str!("../migrations/000_legacy_prepare.sql")),
    ("001_init.sql",                                include_str!("../migrations/001_init.sql")),
    ("002_credentials.sql",                         include_str!("../migrations/002_credentials.sql")),
    ("003_billing.sql",                             include_str!("../migrations/003_billing.sql")),
    ("004_orchestration.sql",                       include_str!("../migrations/004_orchestration.sql")),
    ("005_misc.sql",                                include_str!("../migrations/005_misc.sql")),
    ("006_groups_admin.sql",                        include_str!("../migrations/006_groups_admin.sql")),
    ("007_prompts_attachments.sql",                 include_str!("../migrations/007_prompts_attachments.sql")),
    ("008_plugins_analytics.sql",                   include_str!("../migrations/008_plugins_analytics.sql")),
    ("009_legacy_backfill.sql",                     include_str!("../migrations/009_legacy_backfill.sql")),
    ("010_orchestration_kanban.sql",                include_str!("../migrations/010_orchestration_kanban.sql")),
    ("011_email_domain_orgs.sql",                   include_str!("../migrations/011_email_domain_orgs.sql")),
    ("012_agents_cli_tool.sql",                     include_str!("../migrations/012_agents_cli_tool.sql")),
    ("013_agents_runtime_fields.sql",               include_str!("../migrations/013_agents_runtime_fields.sql")),
    ("014_agent_plugins.sql",                       include_str!("../migrations/014_agent_plugins.sql")),
    ("015_agent_fk_policy.sql",                     include_str!("../migrations/015_agent_fk_policy.sql")),
    ("016_legacy_nav_canonical_columns.sql",        include_str!("../migrations/016_legacy_nav_canonical_columns.sql")),
    ("017_legacy_nav_backfill.sql",                 include_str!("../migrations/017_legacy_nav_backfill.sql")),
    ("018_legacy_nav_validate_projects_team_fk.sql",include_str!("../migrations/018_legacy_nav_validate_projects_team_fk.sql")),
    ("019_legacy_nav_validate_groups_project_fk.sql",include_str!("../migrations/019_legacy_nav_validate_groups_project_fk.sql")),
    ("020_legacy_nav_idx_projects_team.sql",        include_str!("../migrations/020_legacy_nav_idx_projects_team.sql")),
    ("021_legacy_nav_idx_groups_project.sql",       include_str!("../migrations/021_legacy_nav_idx_groups_project.sql")),
    ("022_legacy_nav_idx_teams_org_slug.sql",       include_str!("../migrations/022_legacy_nav_idx_teams_org_slug.sql")),
    ("023_legacy_nav_idx_projects_team_slug.sql",   include_str!("../migrations/023_legacy_nav_idx_projects_team_slug.sql")),
    ("024_legacy_nav_reconcile_function.sql",       include_str!("../migrations/024_legacy_nav_reconcile_function.sql")),
    ("025_agents_hmac_secret.sql",                  include_str!("../migrations/025_agents_hmac_secret.sql")),
    ("026_legacy_nav_canonical_not_null.sql",       include_str!("../migrations/026_legacy_nav_canonical_not_null.sql")),
    ("027_drop_legacy_nav.sql",                     include_str!("../migrations/027_drop_legacy_nav.sql")),
    ("028_agents_nats_connect_password.sql",        include_str!("../migrations/028_agents_nats_connect_password.sql")),
    ("029_cli_credentials_revoked.sql",             include_str!("../migrations/029_cli_credentials_revoked.sql")),
    ("030_user_llm_configs_adopt.sql",              include_str!("../migrations/030_user_llm_configs_adopt.sql")),
    ("031_agent_messages.sql",                      include_str!("../migrations/031_agent_messages.sql")),
    ("032_agents_system_prompt.sql",                include_str!("../migrations/032_agents_system_prompt.sql")),
    ("033_orchestration_durable_delivery.sql",      include_str!("../migrations/033_orchestration_durable_delivery.sql")),
    ("034_password_reset_tokens.sql",               include_str!("../migrations/034_password_reset_tokens.sql")),
    ("035_orchestration_blocked_reason_triggers.sql",include_str!("../migrations/035_orchestration_blocked_reason_triggers.sql")),
    ("036_user_credential_schema_ownership.sql",    include_str!("../migrations/036_user_credential_schema_ownership.sql")),
    ("037_events_org_agent_created_idx.sql",        include_str!("../migrations/037_events_org_agent_created_idx.sql")),
    ("038_billing_stripe_reconciliation.sql",       include_str!("../migrations/038_billing_stripe_reconciliation.sql")),
    ("039_agents_workspace_scope.sql",              include_str!("../migrations/039_agents_workspace_scope.sql")),
    ("040_team_project_permissions.sql",            include_str!("../migrations/040_team_project_permissions.sql")),
    ("041_team_project_member_role_constraints.sql",include_str!("../migrations/041_team_project_member_role_constraints.sql")),
    ("042_task_runs_evidence.sql",                  include_str!("../migrations/042_task_runs_evidence.sql")),
    ("043_events_run_id_index.sql",                 include_str!("../migrations/043_events_run_id_index.sql")),
    ("044_agent_messages_run_id_index.sql",         include_str!("../migrations/044_agent_messages_run_id_index.sql")),
    ("045_attachments_run_id_index.sql",            include_str!("../migrations/045_attachments_run_id_index.sql")),
    ("046_memory_items.sql",                        include_str!("../migrations/046_memory_items.sql")),
    ("047_skills_governance_extension.sql",         include_str!("../migrations/047_skills_governance_extension.sql")),
    ("048_skill_versions.sql",                      include_str!("../migrations/048_skill_versions.sql")),
    ("049_context_candidates_approvals.sql",        include_str!("../migrations/049_context_candidates_approvals.sql")),
    ("050_context_links_feedback.sql",              include_str!("../migrations/050_context_links_feedback.sql")),
    ("051_runtime_capabilities.sql",                include_str!("../migrations/051_runtime_capabilities.sql")),
    ("052_context_resolver_indexes.sql",            include_str!("../migrations/052_context_resolver_indexes.sql")),
    ("053_run_context_injections.sql",              include_str!("../migrations/053_run_context_injections.sql")),
    ("054_run_context_injections_run_idx.sql",      include_str!("../migrations/054_run_context_injections_run_idx.sql")),
    ("055_run_context_injections_item_idx.sql",     include_str!("../migrations/055_run_context_injections_item_idx.sql")),
    ("056_run_context_injections_applied_at_idx.sql",include_str!("../migrations/056_run_context_injections_applied_at_idx.sql")),
    ("057_context_previews.sql",                    include_str!("../migrations/057_context_previews.sql")),
    ("058_context_usage_analytics.sql",             include_str!("../migrations/058_context_usage_analytics.sql")),
    ("059_governance_audit_projection.sql",         include_str!("../migrations/059_governance_audit_projection.sql")),
    ("060_inbox_notifications.sql",                 include_str!("../migrations/060_inbox_notifications.sql")),
    ("062_agents_runtime_kind.sql",                 include_str!("../migrations/062_agents_runtime_kind.sql")),
    ("063_agents_runtime_kind_check.sql",           include_str!("../migrations/063_agents_runtime_kind_check.sql")),
    ("064_agents_runtime_kind_index.sql",           include_str!("../migrations/064_agents_runtime_kind_index.sql")),
    ("065_enrollment_idempotency.sql",              include_str!("../migrations/065_enrollment_idempotency.sql")),
];

/// Run pending SQLx migrations against the database.
///
/// Verifies the SHA-256 manifest before applying any migration. If the manifest
/// check fails the process should not proceed — the caller receives a
/// [`ManifestError`] wrapped in a [`MigrateError`]-compatible error path.
///
/// This is safe to call on every startup — already-applied migrations are skipped.
pub async fn run_migrations(pool: &PgPool) -> Result<(), RunMigrationsError> {
    verify_manifest(MIGRATION_SOURCES)?;
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Unified error type for [`run_migrations`].
///
/// Callers that previously handled `sqlx::migrate::MigrateError` directly
/// should now match this type or propagate with `?` into `anyhow::Error`.
#[derive(Debug, thiserror::Error)]
pub enum RunMigrationsError {
    /// Migration manifest verification failed — possible supply-chain tampering.
    #[error("migration manifest verification failed: {0}")]
    Manifest(#[from] ManifestError),

    /// SQLx migration execution failed.
    #[error("migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
