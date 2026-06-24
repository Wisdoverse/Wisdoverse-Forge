use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::json;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct PermissionArgs {
    /// Agent ID
    pub id: String,
    /// Permission response (e.g. approve, deny)
    pub response: String,
}

pub async fn run(args: PermissionArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "response": args.response });
    ctx.client
        .do_request(
            reqwest::Method::POST,
            &format!("/api/v1/agents/{}/permission", args.id),
            Some(&body),
            ResponseKind::Auto,
        )
        .await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Permission response sent to agent {}.", args.id),
        &json!({ "id": args.id, "response": args.response, "sent": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
