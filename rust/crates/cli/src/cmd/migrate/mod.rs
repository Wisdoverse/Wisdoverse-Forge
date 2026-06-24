//! `agentforge migrate` — operator migration utilities.
//!
//! The `migrate doctor` subcommand checks pre-flight conditions before
//! applying migration 062 (`agents.runtime_kind` discriminator). It connects
//! directly to PostgreSQL via `DATABASE_URL` (or `--database-url` flag).

pub mod doctor;

use crate::error::{CliError, CliResult};
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct MigrateArgs {
    /// PostgreSQL connection URL. Defaults to DATABASE_URL env var.
    #[arg(long, global = true, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    #[command(subcommand)]
    pub command: MigrateSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum MigrateSubcommand {
    /// Run pre-flight checks before applying migration 062 (runtime_kind).
    Doctor(doctor::DoctorOpts),
}

pub async fn dispatch(args: MigrateArgs) -> CliResult<()> {
    let db_url = args.database_url.ok_or_else(|| {
        CliError::other(
            "DATABASE_URL is not set. \
             Pass --database-url or set the DATABASE_URL environment variable.",
        )
    })?;

    let pool = sqlx::PgPool::connect(&db_url)
        .await
        .map_err(|e| CliError::other(format!("failed to connect to database: {e}")))?;

    match args.command {
        MigrateSubcommand::Doctor(opts) => doctor::run(pool, opts).await.map_err(CliError::from),
    }
}
