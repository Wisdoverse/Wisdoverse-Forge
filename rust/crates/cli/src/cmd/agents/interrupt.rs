use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::json;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct InterruptArgs {
    /// Agent ID
    pub id: String,
}

pub async fn run(args: InterruptArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    ctx.client
        .do_request(reqwest::Method::POST, &format!("/api/v1/agents/{}/interrupt", args.id), None, ResponseKind::Auto)
        .await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Agent {} interrupted.", args.id),
        &json!({ "id": args.id, "interrupted": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
