use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct EnrollLocalArgs {
    /// CLI tool available on the local machine (claude, codex, gemini, opencode)
    #[arg(long)]
    pub tool: String,
    /// Agent display name
    #[arg(long)]
    pub name: Option<String>,
    /// Optional model override passed to the local CLI
    #[arg(long)]
    pub model: Option<String>,
    /// Workspace ID to associate the agent with
    #[arg(long)]
    pub workspace: Option<String>,
    /// Project ID to associate the agent with
    #[arg(long)]
    pub project: Option<String>,
    /// Local working directory where the sidecar command will be launched
    #[arg(long)]
    pub cwd: Option<String>,
    /// Print only the shell exports and sidecar command in table mode
    #[arg(long)]
    pub shell: bool,
}

pub async fn run(args: EnrollLocalArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut body = serde_json::Map::new();
    body.insert("cliTool".into(), Value::String(args.tool));
    if let Some(name) = args.name {
        body.insert("name".into(), Value::String(name));
    }
    if let Some(model) = args.model {
        body.insert("model".into(), Value::String(model));
    }
    if let Some(workspace) = args.workspace {
        body.insert("workspaceId".into(), Value::String(workspace));
    }
    if let Some(project) = args.project {
        body.insert("projectId".into(), Value::String(project));
    }
    if let Some(cwd) = args.cwd {
        body.insert("cwd".into(), Value::String(cwd));
    }

    let data = ctx
        .client
        .do_request(
            reqwest::Method::POST,
            "/api/v1/agents/local-enroll",
            Some(&Value::Object(body)),
            ResponseKind::Auto,
        )
        .await?
        .unwrap_or(Value::Null);

    if matches!(ctx.format.as_str(), "json" | "yaml") || ctx.format.starts_with("jsonpath=") {
        output::format(stdout, &ctx.format, &[], &data, None).map_err(|e| CliError::Other(e.to_string()))?;
        return Ok(());
    }

    let shell_exports = data
        .get("enrollment")
        .and_then(|enrollment| enrollment.get("shellExports"))
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::Other("local enrollment response missing shell exports".into()))?;

    if ctx.format == "quiet" || args.shell {
        writeln!(stdout, "{shell_exports}").map_err(|e| CliError::Other(e.to_string()))?;
        return Ok(());
    }

    let agent = data.get("agent").unwrap_or(&Value::Null);
    let agent_id = agent.get("id").and_then(Value::as_str).unwrap_or("unknown");
    let name = agent.get("name").and_then(Value::as_str).unwrap_or("Host CLI agent");
    let runtime_id = data
        .get("enrollment")
        .and_then(|enrollment| enrollment.get("runtimeId"))
        .and_then(Value::as_str)
        .unwrap_or("host-runtime");

    let summary = json!({
        "id": agent_id,
        "name": name,
        "runtimeId": runtime_id
    });
    let text = format!(
        "Host CLI agent enrolled: {name} ({agent_id})\nRuntime: {runtime_id}\n\nRun on the local machine:\n{shell_exports}"
    );
    output::format_action(stdout, &ctx.format, &text, &summary).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}
