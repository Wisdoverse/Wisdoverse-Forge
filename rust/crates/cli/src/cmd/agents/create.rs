use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::interactive;
use crate::output;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, clap::Args)]
pub struct CreateArgs {
    /// CLI tool to use (e.g. claude, gemini, codex)
    #[arg(long)]
    pub tool: Option<String>,
    /// Agent display name
    #[arg(long)]
    pub name: Option<String>,
    /// Project ID to associate the agent with
    #[arg(long)]
    pub project: Option<String>,
    /// Working directory inside the container
    #[arg(long)]
    pub cwd: Option<String>,
    /// Wait until the agent is idle before returning
    #[arg(long)]
    pub wait: bool,
    /// YAML manifest for batch agent creation
    #[arg(long)]
    pub batch: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct BatchManifest {
    agents: Vec<BatchSpec>,
}

#[derive(Debug, Deserialize)]
struct BatchSpec {
    #[serde(default)]
    name: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    cwd: String,
}

pub async fn run(args: CreateArgs, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    if let Some(path) = args.batch {
        return run_batch(path, ctx, stdout, stderr).await;
    }

    let tool = args.tool.ok_or_else(|| CliError::Other("--tool is required".into()))?;
    let mut body = serde_json::Map::new();
    body.insert("cliTool".into(), Value::String(tool));
    if let Some(n) = args.name {
        body.insert("name".into(), Value::String(n));
    }
    if let Some(p) = args.project {
        body.insert("projectId".into(), Value::String(p));
    }
    if let Some(c) = args.cwd {
        body.insert("cwd".into(), Value::String(c));
    }
    let body = Value::Object(body);

    let mut agent = ctx
        .client
        .do_request(reqwest::Method::POST, "/api/v1/agents", Some(&body), ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    if args.wait {
        let id = agent
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CliError::Other("create agent: response missing id field".into()))?
            .to_string();
        if interactive::show_progress() {
            writeln!(stderr, "Waiting for agent {id} to become idle...").ok();
        }
        poll_until_status(ctx, &id, "idle", Duration::from_secs(300)).await?;
        agent = ctx
            .client
            .do_request(reqwest::Method::GET, &format!("/api/v1/agents/{id}"), None, ResponseKind::Auto)
            .await?
            .unwrap_or(Value::Null);
    }

    output::format(stdout, &ctx.format, COLUMNS, &agent, None).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}

async fn run_batch(path: PathBuf, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    let data = std::fs::read_to_string(&path)
        .map_err(|e| CliError::Other(format!("read batch file {}: {e}", path.display())))?;
    let manifest: BatchManifest =
        serde_yaml::from_str(&data).map_err(|e| CliError::Other(format!("parse batch YAML: {e}")))?;
    if manifest.agents.is_empty() {
        return Err(CliError::Other("batch manifest contains no agents".into()));
    }

    let mut results: Vec<Value> = Vec::new();
    let mut success: Vec<Value> = Vec::new();
    let mut failures = 0;

    for (i, spec) in manifest.agents.iter().enumerate() {
        if spec.tool.is_empty() {
            writeln!(stderr, "batch[{i}] {:?}: skipped — tool is required", spec.name).ok();
            results.push(json!({"name": spec.name, "ok": false, "error": "tool is required"}));
            failures += 1;
            continue;
        }
        let mut body = serde_json::Map::new();
        body.insert("cliTool".into(), Value::String(spec.tool.clone()));
        if !spec.name.is_empty() {
            body.insert("name".into(), Value::String(spec.name.clone()));
        }
        if !spec.project.is_empty() {
            body.insert("projectId".into(), Value::String(spec.project.clone()));
        }
        if !spec.cwd.is_empty() {
            body.insert("cwd".into(), Value::String(spec.cwd.clone()));
        }
        let body = Value::Object(body);

        match ctx.client.do_request(reqwest::Method::POST, "/api/v1/agents", Some(&body), ResponseKind::Auto).await {
            Err(e) => {
                writeln!(stderr, "batch[{i}] {:?}: {e}", spec.name).ok();
                results.push(json!({"name": spec.name, "ok": false, "error": e.to_string()}));
                failures += 1;
            }
            Ok(agent_opt) => {
                let agent = agent_opt.unwrap_or(Value::Null);
                if interactive::show_progress() {
                    let id = agent.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    writeln!(stderr, "batch[{i}] {:?}: created {id}", spec.name).ok();
                }
                results.push(json!({"name": spec.name, "ok": true, "agent": agent}));
                success.push(agent);
            }
        }
    }

    match ctx.format.as_str() {
        "json" => crate::output::json::write_envelope(stdout, &Value::Array(results.clone()), None)
            .map_err(|e| CliError::Other(e.to_string()))?,
        "yaml" => crate::output::yaml::write_envelope(stdout, &Value::Array(results.clone()), None)
            .map_err(|e| CliError::Other(e.to_string()))?,
        "quiet" => crate::output::quiet::write(stdout, &Value::Array(success.clone()), "id"),
        _ => {
            if !success.is_empty() {
                output::format(stdout, &ctx.format, COLUMNS, &Value::Array(success.clone()), None)
                    .map_err(|e| CliError::Other(e.to_string()))?;
            }
        }
    }

    if failures > 0 {
        if success.is_empty() {
            return Err(CliError::Other(format!("all {failures} agents failed to create")));
        }
        return Err(CliError::Other(format!("batch: {failures}/{} agents failed", manifest.agents.len())));
    }
    Ok(())
}

/// Polls GET /api/v1/agents/{id} with exponential backoff until the agent reaches
/// `target` status or the deadline expires.
/// Used by `create --wait` and Task 20's `wait` command fallback path.
pub(crate) async fn poll_until_status(ctx: &CliContext, id: &str, target: &str, timeout: Duration) -> CliResult<()> {
    let deadline = std::time::Instant::now() + timeout;
    let mut interval = Duration::from_secs(1);
    loop {
        let path = format!("/api/v1/agents/{id}");
        let agent =
            ctx.client.do_request(reqwest::Method::GET, &path, None, ResponseKind::Auto).await?.unwrap_or(Value::Null);
        if let Some(s) = agent.get("status").and_then(|v| v.as_str())
            && s == target
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(CliError::WaitTimeout(format!(
                "timeout waiting for agent status: agent {id} did not reach {target:?} within {timeout:?}"
            )));
        }
        tokio::time::sleep(interval).await;
        interval *= 2;
        if interval > Duration::from_secs(5) {
            interval = Duration::from_secs(5);
        }
    }
}
