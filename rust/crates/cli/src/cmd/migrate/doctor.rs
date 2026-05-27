//! `agentforge migrate doctor` — pre-flight checks before applying
//! migration 062 (agents.runtime_kind discriminator) and friends.

use anyhow::{Context, Result};
use clap::Args;
use sqlx::PgPool;

#[derive(Debug, Args)]
pub struct DoctorOpts {
    /// Override the row-count threshold (default 100000).
    #[arg(long, default_value_t = 100_000)]
    pub max_row_count: i64,
    /// Skip the row-count gate.
    #[arg(long)]
    pub force: bool,
}

pub async fn run(pool: PgPool, opts: DoctorOpts) -> Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool)
        .await
        .context("counting agents")?;
    println!("agents row count: {}", count.0);

    if !opts.force && count.0 > opts.max_row_count {
        anyhow::bail!(
            "agents table has {} rows (> {}). Migration 062's batched backfill will be slow. \
             Rerun with --force after planning an off-peak window.",
            count.0,
            opts.max_row_count
        );
    }

    let column_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                        WHERE table_name='agents' AND column_name='runtime_kind')",
    )
    .fetch_one(&pool)
    .await?;
    if column_exists.0 {
        let bad: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agents
             WHERE NOT (
               (runtime_kind = 'container' AND cli_tool IS NOT NULL) OR
               (runtime_kind = 'cli'       AND cli_tool IS NOT NULL AND container_id IS NULL) OR
               (runtime_kind = 'api'       AND cli_tool IS NULL    AND container_id IS NULL)
             )",
        )
        .fetch_one(&pool)
        .await?;
        if bad.0 > 0 {
            anyhow::bail!(
                "{} rows would violate the invariant CHECK. \
                 Inspect with: SELECT id, runtime_kind, cli_tool, container_id, runtime_id \
                 FROM agents WHERE NOT (...); and remediate before 063 ships.",
                bad.0
            );
        }
        println!("invariant CHECK pre-flight: 0 offenders");
    } else {
        println!("agents.runtime_kind column not yet present (062 has not run)");
    }

    let pg_version: (String,) = sqlx::query_as("SHOW server_version")
        .fetch_one(&pool)
        .await?;
    println!("postgres server version: {}", pg_version.0);

    println!("migrate doctor: OK");
    Ok(())
}
