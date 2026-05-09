//! Database pool setup and health checking.
//!
//! Provides functions to create a configured PostgreSQL connection pool
//! and check database connectivity.

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

/// Run pending SQLx migrations against the database.
///
/// Uses the embedded migrations from the `./migrations` directory.
/// This is safe to call on every startup — already-applied migrations are skipped.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
