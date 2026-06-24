use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::interactive;
use crate::output;
use base64::Engine;
use futures::StreamExt;
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, clap::Args)]
pub struct PromptArgs {
    /// Agent ID (or message text when --group is set)
    pub id: String,
    /// Prompt message (optional when --prompt-file or --images is used)
    pub message: Option<String>,
    /// Image file paths to attach
    #[arg(long = "images")]
    pub images: Vec<PathBuf>,
    /// Read prompt text from this file
    #[arg(long = "prompt-file")]
    pub prompt_file: Option<PathBuf>,
    /// Wait until the agent is idle before returning
    #[arg(long)]
    pub wait: bool,
    /// Stream events via SSE until agent is idle (NDJSON with -o json)
    #[arg(long)]
    pub stream: bool,
    /// Timeout for the deadline. Defaults: 10 minutes for `--wait`, 30 minutes for `--stream`.
    #[arg(long)]
    pub timeout: Option<String>,
    /// Broadcast prompt to all agents in a group
    #[arg(long)]
    pub group: Option<String>,
}

/// Mirrors `cli/cmd/agents/prompt.go:resolvePromptText`.
/// In group mode, `args.id` IS the message. In normal mode, `args.message` is optional.
/// `-` means read from stdin. `--prompt-file` takes precedence.
fn resolve_prompt_text(args: &PromptArgs, stdin: &mut dyn BufRead, group_mode: bool) -> CliResult<String> {
    let mut prompt_text = if group_mode {
        // In group mode, first positional arg is the message.
        args.id.clone()
    } else {
        // Normal mode: args.id is agent ID, args.message is optional message.
        args.message.clone().unwrap_or_default()
    };

    if prompt_text == "-" {
        // Mirrors Go's `term.IsTerminal` check in resolvePromptText.
        // Using process stdin (not the abstract BufRead) because BufRead can't detect TTY.
        if !crate::interactive::is_interactive() && is_terminal::IsTerminal::is_terminal(&std::io::stdin()) {
            return Err(CliError::Other(
                "stdin is a TTY in non-interactive mode; pipe input or use --prompt-file".into(),
            ));
        }
        let mut data = String::new();
        stdin.read_to_string(&mut data).map_err(|e| CliError::Other(format!("read prompt from stdin: {e}")))?;
        prompt_text = data;
    }

    if let Some(ref path) = args.prompt_file {
        let data = std::fs::read_to_string(path)
            .map_err(|e| CliError::Other(format!("read prompt file {}: {e}", path.display())))?;
        prompt_text = data;
    }

    Ok(prompt_text)
}

/// Mirrors `cli/cmd/agents/prompt.go:resolveStreamDeadline`.
/// When `--timeout` was not supplied, stream mode defaults to 30m (wait mode is 10m).
/// When the user supplied a value, honor it verbatim.
fn resolve_stream_deadline(user_timeout: Option<&str>) -> CliResult<(Duration, String)> {
    match user_timeout {
        None => Ok((Duration::from_secs(30 * 60), "30m".to_string())),
        Some(s) => {
            let dur = humantime::parse_duration(s).map_err(|e| CliError::Other(format!("parse timeout {s:?}: {e}")))?;
            if dur.is_zero() {
                return Ok((Duration::from_secs(30 * 60), "30m".to_string()));
            }
            Ok((dur, s.to_string()))
        }
    }
}

/// Parses a wait timeout string, defaulting to 10 minutes.
fn parse_wait_timeout(user_timeout: Option<&str>) -> CliResult<(Duration, String)> {
    let s = user_timeout.unwrap_or("10m");
    let dur = humantime::parse_duration(s).map_err(|e| CliError::Other(format!("parse timeout {s:?}: {e}")))?;
    if dur.is_zero() {
        return Ok((Duration::from_secs(10 * 60), "10m".to_string()));
    }
    Ok((dur, s.to_string()))
}

/// Returns true if the error is a transport-level failure (reqwest connection error or 502/503/504).
/// Mirrors `cli/cmd/agents/prompt.go:isPromptTransportError`.
fn is_transport_error(err: &CliError) -> bool {
    match err {
        CliError::Api(api) => matches!(api.status, 502..=504),
        CliError::Transport(_) => true,
        _ => false,
    }
}

/// Per-agent result from a group broadcast.
#[derive(Debug, Clone, Serialize)]
struct GroupPromptResult {
    #[serde(rename = "agentId")]
    agent_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
}

/// Wraps an SSE event with the agent ID for fan-in output.
#[derive(Debug, Serialize)]
struct GroupSseEvent {
    #[serde(rename = "agentId")]
    agent_id: String,
    event: String,
    data: Value,
}

pub async fn run(
    args: PromptArgs,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    let group_mode = args.group.is_some();

    // Resolve prompt text.
    let prompt_text = resolve_prompt_text(&args, stdin, group_mode)?;

    if prompt_text.is_empty() && args.images.is_empty() {
        return Err(CliError::Other(
            "prompt message is required (pass as argument, use - for stdin, or --prompt-file)".into(),
        ));
    }

    // Build request body.
    let mut body = serde_json::Map::new();
    body.insert("prompt".into(), Value::String(prompt_text));

    if !args.images.is_empty() {
        let mut encoded = Vec::with_capacity(args.images.len());
        for img_path in &args.images {
            let data = std::fs::read(img_path)
                .map_err(|e| CliError::Other(format!("read image {}: {e}", img_path.display())))?;
            encoded.push(base64::engine::general_purpose::STANDARD.encode(&data));
        }
        body.insert("images".into(), Value::Array(encoded.into_iter().map(Value::String).collect()));
    }

    let body = Value::Object(body);

    // Group broadcast mode.
    if let Some(ref group_id) = args.group {
        let stream_flag = args.stream;
        let (stream_deadline, stream_label) = if stream_flag {
            resolve_stream_deadline(args.timeout.as_deref())?
        } else {
            (Duration::ZERO, String::new())
        };
        return run_group_prompt(ctx, stdout, stderr, group_id, &body, stream_flag, stream_deadline, &stream_label)
            .await;
    }

    // Single-agent mode.
    let id = args.id.clone();

    // POST prompt.
    ctx.client
        .do_request(reqwest::Method::POST, &format!("/api/v1/agents/{id}/prompt"), Some(&body), ResponseKind::Auto)
        .await?;

    // Stream mode takes priority over wait mode (matches Go: `if streamFlag` checked first).
    if args.stream {
        let (deadline, label) = resolve_stream_deadline(args.timeout.as_deref())?;
        return stream_agent_events(ctx, stdout, stderr, &id, deadline, &label).await;
    }

    if args.wait {
        if interactive::show_progress() {
            writeln!(stderr, "Waiting for agent {id} to become idle...").ok();
        }
        let (dur, wait_label) = parse_wait_timeout(args.timeout.as_deref())?;

        let mut q = url::form_urlencoded::Serializer::new(String::new());
        q.append_pair("status", "idle");
        q.append_pair("timeout", &wait_label);
        let path = format!("/api/v1/agents/{id}/wait?{}", q.finish());

        match ctx.client.do_request(reqwest::Method::GET, &path, None, ResponseKind::Auto).await {
            Ok(result_opt) => {
                let result = result_opt.unwrap_or(Value::Null);
                // Check in-band timeout.
                if let Some(true) = result.get("timedOut").and_then(|v| v.as_bool()) {
                    return Err(CliError::WaitTimeout(format!(
                        "timeout: agent {id} did not become idle within {wait_label}"
                    )));
                }
                output::format(stdout, &ctx.format, COLUMNS, &result, None)
                    .map_err(|e| CliError::Other(e.to_string()))?;
                return Ok(());
            }
            Err(err) => {
                // 408 → exit code 6 (timeout)
                if let CliError::Api(ref api) = err
                    && api.status == 408
                {
                    return Err(CliError::WaitTimeout(format!(
                        "timeout: agent {id} did not become idle within {wait_label}"
                    )));
                }
                // Transport errors (502/503/504) → fallback to poll + GET agent.
                if is_transport_error(&err) {
                    super::create::poll_until_status(ctx, &id, "idle", dur).await?;
                    let agent = ctx
                        .client
                        .do_request(reqwest::Method::GET, &format!("/api/v1/agents/{id}"), None, ResponseKind::Auto)
                        .await
                        .map_err(|e| CliError::Other(format!("get agent after wait: {e}")))?
                        .unwrap_or(Value::Null);
                    output::format(stdout, &ctx.format, COLUMNS, &agent, None)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                    return Ok(());
                }
                // All other errors (401/403/404/400) → propagate.
                return Err(CliError::Other(format!("wait: {err}")));
            }
        }
    }

    // Neither stream nor wait: just report the action.
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Prompt sent to agent {id}."),
        &json!({ "agentId": id, "queued": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}

/// Fetches workers for a group and broadcasts the prompt to all of them.
/// Mirrors `cli/cmd/agents/prompt.go:runGroupPrompt`.
#[allow(clippy::too_many_arguments)]
async fn run_group_prompt(
    ctx: &CliContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    group_id: &str,
    body: &Value,
    stream_flag: bool,
    stream_deadline: Duration,
    stream_deadline_label: &str,
) -> CliResult<()> {
    // Fetch group workers.
    let (workers, _, _, _) = ctx
        .client
        .do_request_list(reqwest::Method::GET, &format!("/api/v1/groups/{group_id}/workers"), None)
        .await
        .map_err(|e| CliError::Other(format!("get group workers: {e}")))?;

    if workers.is_empty() {
        return Err(CliError::Other(format!("group {group_id} has no workers")));
    }

    // Extract agent IDs and names.
    let agents: Vec<(String, String)> = workers
        .iter()
        .filter_map(|w| {
            let id = w.get("id")?.as_str()?.to_string();
            if id.is_empty() {
                return None;
            }
            let name = w.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some((id, name))
        })
        .collect();

    if agents.is_empty() {
        return Err(CliError::Other(format!("group {group_id} workers have no valid IDs")));
    }

    // Send prompts in parallel using JoinSet.
    let mut set = tokio::task::JoinSet::new();
    for (agent_id, name) in agents.iter().cloned() {
        let client = ctx.client.clone();
        let body = body.clone();
        set.spawn(async move {
            let path = format!("/api/v1/agents/{agent_id}/prompt");
            let result = client.do_request(reqwest::Method::POST, &path, Some(&body), ResponseKind::Auto).await;
            (agent_id, name, result)
        });
    }

    let total = agents.len();
    let mut results: Vec<GroupPromptResult> = Vec::with_capacity(total);

    while let Some(join_result) = set.join_next().await {
        let (agent_id, name, dispatch_result) = join_result.map_err(|e| CliError::Other(format!("join error: {e}")))?;
        match dispatch_result {
            Ok(_) => {
                results.push(GroupPromptResult { agent_id, name, ok: true, error: String::new() });
            }
            Err(e) => {
                writeln!(stderr, "group prompt {} ({}): {e}", agent_id, name).ok();
                results.push(GroupPromptResult { agent_id, name, ok: false, error: e.to_string() });
            }
        }
    }

    // If streaming, fan-in SSE from all successful agents.
    if stream_flag {
        return stream_group_events(ctx, stdout, stderr, &results, stream_deadline, stream_deadline_label).await;
    }

    // Report dispatch summary.
    let dispatched = results.iter().filter(|r| r.ok).count();
    let failed = results.iter().filter(|r| !r.ok).count();

    let summary_data = json!({
        "groupId": group_id,
        "total": total,
        "dispatched": dispatched,
        "failed": failed,
        "results": results,
    });

    match ctx.format.as_str() {
        "json" | "yaml" => {
            output::format(stdout, &ctx.format, &[], &summary_data, None)
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        _ => {
            output::format_action(
                stdout,
                &ctx.format,
                &format!("Prompt dispatched to {dispatched}/{total} agents in group {group_id}."),
                &summary_data,
            )
            .map_err(|e| CliError::Other(e.to_string()))?;
        }
    }
    Ok(())
}

/// Streams SSE events for a single agent until idle or timeout.
/// Mirrors `cli/cmd/agents/prompt.go:streamAgentEvents`.
async fn stream_agent_events(
    ctx: &CliContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    id: &str,
    deadline: Duration,
    deadline_label: &str,
) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.append_pair("agent-id", id);
    q.append_pair("types", "assistant_text,tool_start,tool_finish,agent_idle,permission_request,error");
    let path = format!("/api/v1/events/stream?{}", q.finish());

    let mut stream = ctx.client.stream_sse(&path, 3).await?;

    let sleeper = tokio::time::sleep(deadline);
    tokio::pin!(sleeper);

    loop {
        tokio::select! {
            _ = &mut sleeper => {
                return Err(CliError::WaitTimeout(format!(
                    "timeout: agent {id} did not become idle within {deadline_label}"
                )));
            }
            ev_opt = stream.next() => {
                match ev_opt {
                    None => return Ok(()),
                    Some(Err(e)) => return Err(e),
                    Some(Ok(ev)) => {
                        // Skip control events to stderr.
                        if ev.event == "overflow" || ev.event == "shutdown" {
                            writeln!(stderr, "SSE: {}: {}", ev.event, ev.data).ok();
                            continue;
                        }
                        if ctx.format == "json" {
                            writeln!(stdout, "{}", ev.data).ok();
                        } else {
                            writeln!(stdout, "[{}] {}", ev.event, ev.data).ok();
                        }
                        if ev.event == "agent_idle" {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

/// Tagged SSE event for the fan-in channel.
struct TaggedEvent {
    agent_id: String,
    event: crate::client::SseEvent,
}

/// Opens SSE connections for all successfully prompted agents and fans-in
/// their events to a single output stream.
/// Mirrors `cli/cmd/agents/prompt.go:streamGroupEvents`.
async fn stream_group_events(
    ctx: &CliContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    results: &[GroupPromptResult],
    deadline: Duration,
    deadline_label: &str,
) -> CliResult<()> {
    // Collect IDs of agents that were successfully prompted.
    let active_ids: Vec<String> = results.iter().filter(|r| r.ok).map(|r| r.agent_id.clone()).collect();

    if active_ids.is_empty() {
        return Err(CliError::Other("no agents were successfully prompted; nothing to stream".into()));
    }

    let total_agents = active_ids.len();
    let (tx, mut rx) = mpsc::channel::<TaggedEvent>(64);

    // Spawn one task per agent to forward events into the channel.
    for agent_id in active_ids.iter() {
        let client = ctx.client.clone();
        let tx = tx.clone();
        let agent_id_clone = agent_id.clone();
        // Build the SSE path before entering the async block so the
        // url::form_urlencoded::Serializer (not Send) doesn't cross the await.
        let sse_path = {
            let mut q = url::form_urlencoded::Serializer::new(String::new());
            q.append_pair("agent-id", agent_id);
            q.append_pair("types", "assistant_text,tool_start,tool_finish,agent_idle,permission_request,error");
            format!("/api/v1/events/stream?{}", q.finish())
        };
        tokio::spawn(async move {
            let path = sse_path;

            let mut stream = match client.stream_sse(&path, 3).await {
                Ok(s) => s,
                Err(_) => return,
            };

            while let Some(ev_result) = stream.next().await {
                match ev_result {
                    Ok(ev) => {
                        let is_idle = ev.event == "agent_idle";
                        let _ = tx.send(TaggedEvent { agent_id: agent_id_clone.clone(), event: ev }).await;
                        if is_idle {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Drop our sender clone so the channel closes when all spawned tasks finish.
    drop(tx);

    let sleeper = tokio::time::sleep(deadline);
    tokio::pin!(sleeper);

    let mut idle_count = 0usize;

    loop {
        tokio::select! {
            _ = &mut sleeper => {
                let remaining = total_agents - idle_count;
                return Err(CliError::WaitTimeout(format!(
                    "timeout: {remaining}/{total_agents} agents did not become idle within {deadline_label}"
                )));
            }
            msg = rx.recv() => {
                match msg {
                    None => break, // All senders dropped — all agents done.
                    Some(tagged) => {
                        let ev = &tagged.event;

                        // Skip control events to stderr.
                        if ev.event == "overflow" || ev.event == "shutdown" {
                            writeln!(stderr, "SSE [{}]: {}: {}", tagged.agent_id, ev.event, ev.data).ok();
                            continue;
                        }

                        if ctx.format == "json" {
                            // NDJSON: try to parse data as JSON, fall back to string.
                            let data_value: Value = serde_json::from_str(&ev.data)
                                .unwrap_or_else(|_| Value::String(ev.data.clone()));
                            let out = GroupSseEvent {
                                agent_id: tagged.agent_id.clone(),
                                event: ev.event.clone(),
                                data: data_value,
                            };
                            if let Ok(line) = serde_json::to_string(&out) {
                                writeln!(stdout, "{line}").ok();
                            }
                        } else {
                            writeln!(stdout, "[{}][{}] {}", tagged.agent_id, ev.event, ev.data).ok();
                        }

                        if ev.event == "agent_idle" {
                            idle_count += 1;
                            if idle_count >= total_agents {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn stream_default_timeout_is_30m() {
        let (dur, label) = resolve_stream_deadline(None).unwrap();
        assert_eq!(dur, Duration::from_secs(30 * 60));
        assert_eq!(label, "30m");
    }

    #[test]
    fn stream_explicit_10m_honors_user_value() {
        // Regression: user explicitly passes 10m and it MUST stay 10m, not get widened to 30m.
        let (dur, label) = resolve_stream_deadline(Some("10m")).unwrap();
        assert_eq!(dur, Duration::from_secs(10 * 60));
        assert_eq!(label, "10m");
    }

    #[test]
    fn stream_explicit_5m_honored() {
        let (dur, label) = resolve_stream_deadline(Some("5m")).unwrap();
        assert_eq!(dur, Duration::from_secs(5 * 60));
        assert_eq!(label, "5m");
    }

    #[test]
    fn wait_default_timeout_is_10m() {
        let (dur, label) = parse_wait_timeout(None).unwrap();
        assert_eq!(dur, Duration::from_secs(10 * 60));
        assert_eq!(label, "10m");
    }

    #[test]
    fn wait_explicit_2m_honored() {
        let (dur, label) = parse_wait_timeout(Some("2m")).unwrap();
        assert_eq!(dur, Duration::from_secs(2 * 60));
        assert_eq!(label, "2m");
    }
}

#[cfg(test)]
mod transport_detection_tests {
    use super::*;
    use crate::client::ApiError;

    #[test]
    fn transport_variant_detected() {
        assert!(is_transport_error(&CliError::Transport("connect refused".into())));
    }

    #[test]
    fn api_502_detected() {
        assert!(is_transport_error(&CliError::Api(ApiError {
            code: "HTTP_ERROR".into(),
            message: "bad gateway".into(),
            status: 502
        })));
    }

    #[test]
    fn api_503_detected() {
        assert!(is_transport_error(&CliError::Api(ApiError {
            code: "SERVICE_UNAVAILABLE".into(),
            message: "unavailable".into(),
            status: 503
        })));
    }

    #[test]
    fn api_504_detected() {
        assert!(is_transport_error(&CliError::Api(ApiError {
            code: "GATEWAY_TIMEOUT".into(),
            message: "timeout".into(),
            status: 504
        })));
    }

    #[test]
    fn api_500_not_detected() {
        assert!(!is_transport_error(&CliError::Api(ApiError {
            code: "INTERNAL".into(),
            message: "boom".into(),
            status: 500
        })));
    }

    #[test]
    fn other_variant_not_detected() {
        assert!(!is_transport_error(&CliError::Other("parse error".into())));
    }
}

#[cfg(test)]
mod prompt_error_tests {
    use super::*;
    use crate::client::{Client, ClientOptions};
    use crate::context::CliContext;
    use std::io::Cursor;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_ctx(server_uri: String) -> CliContext {
        let client = Client::new(ClientOptions {
            server: server_uri,
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        CliContext {
            client: Arc::new(client),
            format: "json".into(),
            jq: String::new(),
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn preserves_api_exit_codes_when_prompt_post_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/agents/missing/prompt"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "ok": false,
                "error": "NOT_FOUND",
                "message": "agent missing not found"
            })))
            .mount(&server)
            .await;

        let ctx = test_ctx(server.uri());
        let args = PromptArgs {
            id: "missing".into(),
            message: Some("hello".into()),
            images: Vec::new(),
            prompt_file: None,
            wait: false,
            stream: false,
            timeout: None,
            group: None,
        };

        let mut stdin = Cursor::new(Vec::<u8>::new());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let err = run(args, &ctx, &mut stdin, &mut stdout, &mut stderr).await.unwrap_err();

        match err {
            CliError::Api(api) => {
                assert_eq!(api.code, "NOT_FOUND");
                assert_eq!(api.status, 404);
                assert_eq!(api.exit_code(), 4);
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
