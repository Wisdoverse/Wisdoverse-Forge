use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::Value;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct GetArgs {
    /// Agent ID
    pub id: String,
}

pub async fn run(args: GetArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = format!("/api/v1/agents/{}", args.id);
    let agent =
        ctx.client.do_request(reqwest::Method::GET, &path, None, ResponseKind::Auto).await?.unwrap_or(Value::Null);
    output::format_with_jq(stdout, &ctx.format, COLUMNS, &agent, None, &ctx.jq)
        .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
