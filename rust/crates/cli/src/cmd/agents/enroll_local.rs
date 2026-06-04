use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ShellFormat {
    Bash,
    #[value(name = "powershell", alias = "pwsh")]
    PowerShell,
}

impl ShellFormat {
    fn native() -> Self {
        if cfg!(windows) { Self::PowerShell } else { Self::Bash }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::PowerShell => "PowerShell",
        }
    }
}

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
    /// Shell syntax for the local sidecar launch block
    #[arg(long, value_enum)]
    pub shell_format: Option<ShellFormat>,
    /// Print only the local sidecar launch block in table mode
    #[arg(long)]
    pub shell: bool,
}

pub async fn run(args: EnrollLocalArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let shell_format = args.shell_format.unwrap_or_else(ShellFormat::native);
    let shell_only = args.shell;
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

    let idempotency_key = enrollment_idempotency_key(&body)?;
    let data = ctx
        .client
        .do_request_with_headers(
            reqwest::Method::POST,
            "/api/v1/agents/local-enroll",
            Some(&Value::Object(body)),
            ResponseKind::Auto,
            &[("Idempotency-Key", idempotency_key.as_str())],
        )
        .await?
        .unwrap_or(Value::Null);

    if matches!(ctx.format.as_str(), "json" | "yaml") || ctx.format.starts_with("jsonpath=") {
        output::format(stdout, &ctx.format, &[], &data, None).map_err(|e| CliError::Other(e.to_string()))?;
        return Ok(());
    }

    let launch_command = launch_command_from_response(&data, shell_format)
        .or_else(|| {
            data.get("enrollment")
                .and_then(|enrollment| enrollment.get("shellExports"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .ok_or_else(|| CliError::Other("local enrollment response missing shell exports".into()))?;

    if ctx.format == "quiet" || shell_only {
        writeln!(stdout, "{launch_command}").map_err(|e| CliError::Other(e.to_string()))?;
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
        "Host CLI agent enrolled: {name} ({agent_id})\nRuntime: {runtime_id}\n\nRun on the local machine with {}:\n{launch_command}",
        shell_format.label()
    );
    output::format_action(stdout, &ctx.format, &text, &summary).map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}

fn launch_command_from_response(data: &Value, shell_format: ShellFormat) -> Option<String> {
    let enrollment = data.get("enrollment")?;
    let env = enrollment.get("env")?.as_object()?;
    if env.is_empty() {
        return None;
    }
    let sidecar_command = enrollment.get("sidecarCommand").and_then(Value::as_str).unwrap_or("agentforge-sidecar");
    let mut entries =
        env.iter().filter_map(|(key, value)| value.as_str().map(|value| (key.as_str(), value))).collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    Some(format_launch_command(&entries, sidecar_command, shell_format))
}

fn enrollment_idempotency_key(body: &serde_json::Map<String, Value>) -> CliResult<String> {
    let bytes =
        serde_json::to_vec(body).map_err(|e| CliError::Other(format!("build enrollment idempotency key: {e}")))?;
    let digest = Sha256::digest(bytes);
    Ok(format!("cli-enroll-{}", hex::encode(&digest[..16])))
}

fn format_launch_command(entries: &[(&str, &str)], sidecar_command: &str, shell_format: ShellFormat) -> String {
    entries
        .iter()
        .map(|(key, value)| match shell_format {
            ShellFormat::Bash => format!("export {key}={}", bash_quote(value)),
            ShellFormat::PowerShell => format!("$env:{key} = {}", powershell_quote(value)),
        })
        .chain(std::iter::once(sidecar_command.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bash_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_bash_launch_command_from_enrollment_env() {
        let data = json!({
            "enrollment": {
                "env": {
                    "AGENTFORGE_AGENT_ID": "agent-1",
                    "AGENTFORGE_SERVER_URL": "https://forge.example.com"
                },
                "sidecarCommand": "agentforge-sidecar"
            }
        });

        let command = launch_command_from_response(&data, ShellFormat::Bash).expect("launch command");

        assert_eq!(
            command,
            "export AGENTFORGE_AGENT_ID='agent-1'\nexport AGENTFORGE_SERVER_URL='https://forge.example.com'\nagentforge-sidecar"
        );
    }

    #[test]
    fn formats_powershell_launch_command_from_enrollment_env() {
        let data = json!({
            "enrollment": {
                "env": {
                    "AGENTFORGE_AGENT_ID": "agent-1",
                    "AGENTFORGE_SERVER_URL": "https://forge.example.com/team's"
                },
                "sidecarCommand": "agentforge-sidecar"
            }
        });

        let command = launch_command_from_response(&data, ShellFormat::PowerShell).expect("launch command");

        assert_eq!(
            command,
            "$env:AGENTFORGE_AGENT_ID = 'agent-1'\n$env:AGENTFORGE_SERVER_URL = 'https://forge.example.com/team''s'\nagentforge-sidecar"
        );
    }

    #[test]
    fn enrollment_idempotency_key_is_stable_and_header_safe() {
        let mut body = serde_json::Map::new();
        body.insert("cliTool".into(), Value::String("codex".into()));
        body.insert("name".into(), Value::String("Host Codex".into()));

        let key = enrollment_idempotency_key(&body).unwrap();

        assert_eq!(key, enrollment_idempotency_key(&body).unwrap());
        assert!(key.starts_with("cli-enroll-"));
        assert!(key.len() <= 256);
        assert!(key.chars().all(|ch| matches!(ch, '0'..='9' | 'a'..='z' | '-')));

        body.insert("name".into(), Value::String("Another Host Codex".into()));
        assert_ne!(key, enrollment_idempotency_key(&body).unwrap());
    }
}
