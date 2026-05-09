use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::interactive::confirm::confirm_or_force;
use crate::output;
use serde_json::json;
use std::io::{BufRead, Write};

#[derive(Debug, clap::Args)]
pub struct DeleteArgs {
    /// Agent ID
    pub id: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

pub async fn run(
    args: DeleteArgs,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    let confirmed = confirm_or_force(args.force, &format!("Delete agent {}?", args.id), stderr, stdin)?;
    if !confirmed {
        writeln!(stderr, "Aborted.").ok();
        return Ok(());
    }
    ctx.client
        .do_request(reqwest::Method::DELETE, &format!("/api/v1/agents/{}", args.id), None, ResponseKind::Auto)
        .await?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Agent {} deleted.", args.id),
        &json!({ "id": args.id, "deleted": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
