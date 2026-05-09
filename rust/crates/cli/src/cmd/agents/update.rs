use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    /// Agent ID
    pub id: String,
    /// New display name for the agent
    #[arg(long)]
    pub name: Option<String>,
}

pub async fn run(args: UpdateArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let Some(name) = args.name else {
        return Err(CliError::Other("at least one field must be specified (e.g. --name)".into()));
    };
    let body = json!({ "name": name });
    ctx.client
        .do_request(reqwest::Method::PATCH, &format!("/api/v1/agents/{}", args.id), Some(&body), ResponseKind::Auto)
        .await?;
    let agent = ctx
        .client
        .do_request(reqwest::Method::GET, &format!("/api/v1/agents/{}", args.id), None, ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &agent, None).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
