use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::json;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct TransferArgs {
    /// Agent ID
    pub id: String,
    /// Target user ID to transfer ownership to
    #[arg(long)]
    pub to: String,
}

pub async fn run(args: TransferArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({ "newOwnerId": args.to });
    ctx.client
        .do_request(
            reqwest::Method::POST,
            &format!("/api/v1/agents/{}/transfer-ownership", args.id),
            Some(&body),
            ResponseKind::Auto,
        )
        .await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Agent {} transferred to {}.", args.id, args.to),
        &json!({ "id": args.id, "newOwnerId": args.to, "transferred": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
