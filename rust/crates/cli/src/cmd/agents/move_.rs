use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct MoveArgs {
    /// Agent ID
    pub id: String,
    /// Target project ID
    #[arg(long)]
    pub project: String,
}

pub async fn run(args: MoveArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "projectId": args.project });
    let agent = ctx
        .client
        .do_request(
            reqwest::Method::PATCH,
            &format!("/api/v1/agents/{}/move", args.id),
            Some(&body),
            ResponseKind::Auto,
        )
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &agent, None).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
