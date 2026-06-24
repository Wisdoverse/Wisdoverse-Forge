use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::{self, Column, Pagination};
use serde_json::{Value, json};
use std::io::Write;

const COLUMNS: &[Column] = &[
    Column { header: "USER_ID", field: "userId" },
    Column { header: "PERMISSION", field: "permission" },
    Column { header: "ADDED", field: "createdAt" },
];

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct CollaboratorsArgs {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(Debug, clap::Subcommand)]
pub enum Sub {
    /// List collaborators for an agent
    List {
        /// Agent ID
        id: String,
    },
    /// Add a collaborator to an agent
    Add {
        /// Agent ID
        id: String,
        /// User ID to grant access
        #[arg(long)]
        user: String,
        /// Permission level (view, prompt, manage)
        #[arg(long)]
        permission: String,
    },
    /// Remove a collaborator from an agent
    Remove {
        /// Agent ID
        id: String,
        /// User ID to revoke access from
        #[arg(long)]
        user: String,
    },
}

pub async fn run(args: CollaboratorsArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    match args.command {
        Sub::List { id } => list(id, ctx, stdout).await,
        Sub::Add { id, user, permission } => add(id, user, permission, ctx, stdout).await,
        Sub::Remove { id, user } => remove(id, user, ctx, stdout).await,
    }
}

async fn list(id: String, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = format!("/api/v1/agents/{id}/collaborators");
    let (items, total, _limit, _offset) = ctx.client.do_request_list(reqwest::Method::GET, &path, None).await?;
    let len = items.len();
    let pag = Pagination { total: if total > 0 { total as usize } else { len }, limit: len, offset: 0 };
    let data = Value::Array(items);
    output::format(stdout, &ctx.format, COLUMNS, &data, Some(&pag)).map_err(|e| CliError::Other(e.to_string()))
}

async fn add(id: String, user: String, permission: String, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "userId": user, "permission": permission });
    let c = ctx
        .client
        .do_request(
            reqwest::Method::POST,
            &format!("/api/v1/agents/{id}/collaborators"),
            Some(&body),
            ResponseKind::Auto,
        )
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &c, None).map_err(|e| CliError::Other(e.to_string()))
}

async fn remove(id: String, user: String, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = format!("/api/v1/agents/{id}/collaborators/{user}");
    ctx.client.do_request(reqwest::Method::DELETE, &path, None, ResponseKind::Auto).await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Collaborator {user} removed from agent {id}."),
        &json!({ "id": id, "userId": user, "removed": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))
}
