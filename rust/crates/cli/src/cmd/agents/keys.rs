use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::json;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct KeysArgs {
    /// Agent ID
    pub id: String,
    /// One or more environment variable keys to push
    #[arg(required = true)]
    pub keys: Vec<String>,
}

pub async fn run(args: KeysArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "keys": args.keys });
    ctx.client
        .do_request(reqwest::Method::POST, &format!("/api/v1/agents/{}/keys", args.id), Some(&body), ResponseKind::Auto)
        .await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Keys sent to agent {}: {:?}", args.id, args.keys),
        &json!({ "id": args.id, "keys": args.keys }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
