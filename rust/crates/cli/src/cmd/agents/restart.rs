use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct RestartArgs {
    /// Agent ID
    pub id: String,
    /// Keep the existing container on restart
    #[arg(long = "preserve-container")]
    pub preserve_container: bool,
}

pub async fn run(args: RestartArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "preserveContainer": args.preserve_container });
    ctx.client
        .do_request(
            reqwest::Method::POST,
            &format!("/api/v1/agents/{}/restart", args.id),
            Some(&body),
            ResponseKind::Auto,
        )
        .await?;
    let agent = ctx
        .client
        .do_request(reqwest::Method::GET, &format!("/api/v1/agents/{}", args.id), None, ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &agent, None).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
