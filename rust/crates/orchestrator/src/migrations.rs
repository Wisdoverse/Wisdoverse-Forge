//! Orchestrator database migrations + supply-chain manifest verification.
//!
//! Mirrors the database crate's manifest check: before any orchestrator
//! migration runs, every embedded `.sql` file is verified against the
//! committed `MANIFEST.sha256`. See [`agentforge_db::manifest`] for the exact
//! guarantees (PR-time + startup staleness detection, not post-build crypto
//! integrity).
//!
//! When this check fails the process must not migrate. To recover, regenerate
//! the manifest and commit it:
//!
//! ```text
//! cd rust/crates/orchestrator/migrations
//! sha256sum *.sql > MANIFEST.sha256
//! git add MANIFEST.sha256 && git commit -m "chore(orchestrator): update migration manifest"
//! ```

use agentforge_db::{ManifestError, verify_manifest};
use sqlx::PgPool;

/// Embedded `MANIFEST.sha256` for the orchestrator's migrations.
///
/// Lives next to the migration files so it travels with the binary and is
/// checked against [`MIGRATION_SOURCES`] on startup.
const MIGRATION_MANIFEST: &str = include_str!("../migrations/MANIFEST.sha256");

/// Embedded migration SQL sources used for manifest verification.
///
/// Each tuple is `(filename, sql_content)`. Populated at compile time from the
/// same `migrations/` directory used by `sqlx::migrate!()`, so the two sources
/// stay in sync. Keep this list aligned with the `.sql` files on disk; the
/// `manifest_integrity_test` and CI `migration-manifest.yml` enforce that.
const MIGRATION_SOURCES: &[(&str, &str)] = &[
    ("001_initial.sql", include_str!("../migrations/001_initial.sql")),
    ("002_handler_mcp_fields.sql", include_str!("../migrations/002_handler_mcp_fields.sql")),
    ("004_temporal_workflow.sql", include_str!("../migrations/004_temporal_workflow.sql")),
    ("005_knowledge_expansion.sql", include_str!("../migrations/005_knowledge_expansion.sql")),
    ("006_audit_logs.sql", include_str!("../migrations/006_audit_logs.sql")),
    ("007_teams_multitenant.sql", include_str!("../migrations/007_teams_multitenant.sql")),
    ("008_adopt_legacy_integer_schema.sql", include_str!("../migrations/008_adopt_legacy_integer_schema.sql")),
];

/// Run pending orchestrator migrations against the database.
///
/// Verifies the SHA-256 manifest before applying any migration. If the manifest
/// check fails the process should not proceed — the caller receives a
/// [`RunMigrationsError::Manifest`] before any SQL is executed.
///
/// Safe to call on every startup — already-applied migrations are skipped.
pub async fn run_migrations(pool: &PgPool) -> Result<(), RunMigrationsError> {
    verify_manifest(MIGRATION_MANIFEST, MIGRATION_SOURCES)?;
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Unified error type for [`run_migrations`].
#[derive(Debug, thiserror::Error)]
pub enum RunMigrationsError {
    /// Migration manifest verification failed — possible drift or tampering.
    #[error("orchestrator migration manifest verification failed: {0}")]
    Manifest(#[from] ManifestError),

    /// SQLx migration execution failed.
    #[error("orchestrator migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_embedded_sources() {
        // The committed manifest must verify cleanly against the embedded
        // migration set. Catches a forgotten manifest regen or a source-list /
        // on-disk drift at build time, before any deployment.
        verify_manifest(MIGRATION_MANIFEST, MIGRATION_SOURCES)
            .expect("orchestrator MANIFEST.sha256 must match embedded migration sources");
    }
}
