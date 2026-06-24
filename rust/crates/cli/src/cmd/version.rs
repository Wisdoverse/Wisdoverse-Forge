use crate::build_info::BuildInfo;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::Value;
use std::io::Write;

pub async fn run(info: &BuildInfo, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    let mut result = serde_json::Map::new();
    result.insert("cliVersion".into(), Value::String(info.version.clone()));
    if !info.commit.is_empty() {
        result.insert("commit".into(), Value::String(info.commit.clone()));
    }
    if !info.date.is_empty() {
        result.insert("buildDate".into(), Value::String(info.date.clone()));
    }

    // Best-effort: fetch server version from health endpoint.
    let server_version =
        match ctx.client.do_request(reqwest::Method::GET, "/api/v1/health", None, ResponseKind::Auto).await {
            Ok(Some(v)) => v
                .get("version")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| "(unknown)".into()),
            Ok(None) => "(unknown)".into(),
            Err(e) => {
                let _ = writeln!(stderr, "Server version: (unavailable: {e})");
                "(unavailable)".into()
            }
        };
    result.insert("serverVersion".into(), Value::String(server_version.clone()));

    let value = Value::Object(result);
    match ctx.format.as_str() {
        "json" | "yaml" | "quiet" => {
            output::format(stdout, &ctx.format, &[], &value, None).map_err(|e| CliError::Other(e.to_string()))
        }
        _ => {
            let _ = writeln!(stdout, "CLI version:    {}", info.version);
            if !info.commit.is_empty() {
                let _ = writeln!(stdout, "Commit:         {}", info.commit);
            }
            if !info.date.is_empty() {
                let _ = writeln!(stdout, "Build date:     {}", info.date);
            }
            let _ = writeln!(stdout, "Server version: {server_version}");
            Ok(())
        }
    }
}
