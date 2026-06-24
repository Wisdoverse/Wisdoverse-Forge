use crate::client::ResponseKind;
use crate::client::sse::SseEvent;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::{self, Column, Pagination};
use chrono::DateTime;
use futures::StreamExt;
use serde_json::Value;
use std::io::Write;

pub const COLUMNS: &[Column] = &[
    Column { header: "ID", field: "id" },
    Column { header: "TYPE", field: "type" },
    Column { header: "AGENT", field: "agentId" },
    Column { header: "TIMESTAMP", field: "createdAt" },
];

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct EventsArgs {
    #[command(subcommand)]
    pub command: EventsSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum EventsSubcommand {
    /// List events
    List(ListArgs),
    /// Watch events in real time (SSE by default)
    Watch(WatchArgs),
    /// Get event statistics
    Stats(StatsArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Filter by agent ID
    #[arg(long = "agent-id")]
    pub agent_id: Option<String>,
    /// Maximum number of events to return
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
    /// Offset for pagination
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
    /// Return events before this timestamp (RFC3339 or unix milliseconds)
    #[arg(long)]
    pub before: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct WatchArgs {
    /// Filter by agent ID
    #[arg(long = "agent-id")]
    pub agent_id: Option<String>,
    /// Comma-separated event types to filter (e.g. tool_start,agent_idle)
    #[arg(long)]
    pub types: Option<String>,
    /// Use degraded polling instead of SSE (events may be missed)
    #[arg(long = "poll-fallback")]
    pub poll_fallback: bool,
    /// Polling interval (only with --poll-fallback)
    #[arg(long = "poll-interval", default_value = "2s")]
    pub poll_interval: String,
}

#[derive(Debug, clap::Args)]
pub struct StatsArgs {
    /// Filter stats by agent ID
    #[arg(long = "agent-id")]
    pub agent_id: Option<String>,
}

pub async fn dispatch(
    args: EventsArgs,
    ctx: &CliContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    match args.command {
        EventsSubcommand::List(a) => list(a, ctx, stdout).await,
        EventsSubcommand::Watch(a) => watch(a, ctx, stdout, stderr).await,
        EventsSubcommand::Stats(a) => stats(a, ctx, stdout).await,
    }
}

async fn list(args: ListArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(a) = &args.agent_id {
        q.append_pair("agentId", a);
    }
    q.append_pair("limit", &args.limit.to_string());
    q.append_pair("offset", &args.offset.to_string());
    if let Some(before) = &args.before {
        // Server expects unix milliseconds; accept both RFC3339 and numeric.
        if let Ok(ts) = DateTime::parse_from_rfc3339(before) {
            q.append_pair("before", &ts.timestamp_millis().to_string());
        } else {
            q.append_pair("before", before);
        }
    }
    let path = format!("/api/v1/events?{}", q.finish());

    let (items, total, limit, offset) = ctx.client.do_request_list(reqwest::Method::GET, &path, None).await?;

    let pag = Pagination {
        total: total as usize,
        limit: if limit > 0 { limit as usize } else { args.limit as usize },
        offset: if offset > 0 { offset as usize } else { args.offset as usize },
    };
    let data = Value::Array(items);
    output::format_with_jq(stdout, &ctx.format, COLUMNS, &data, Some(&pag), &ctx.jq)
        .map_err(|e| CliError::Other(e.to_string()))
}

async fn stats(args: StatsArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(a) = &args.agent_id {
        q.append_pair("agentId", a);
    }
    let path = format!("/api/v1/events/stats?{}", q.finish());
    let result =
        ctx.client.do_request(reqwest::Method::GET, &path, None, ResponseKind::Auto).await?.unwrap_or(Value::Null);
    output::format_with_jq(stdout, &ctx.format, &[], &result, None, &ctx.jq).map_err(|e| CliError::Other(e.to_string()))
}

async fn watch(args: WatchArgs, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    if args.poll_fallback {
        return watch_poll(args, ctx, stdout, stderr).await;
    }
    watch_sse(args, ctx, stdout, stderr).await
}

async fn watch_sse(args: WatchArgs, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(a) = &args.agent_id {
        q.append_pair("agent-id", a);
    }
    if let Some(t) = &args.types {
        q.append_pair("types", t);
    }
    let qs = q.finish();
    let path = if qs.is_empty() { "/api/v1/events/stream".to_string() } else { format!("/api/v1/events/stream?{qs}") };

    let stream = ctx.client.stream_sse(&path, 3).await?;
    tokio::pin!(stream);

    while let Some(item) = stream.next().await {
        let ev: SseEvent = match item {
            Ok(e) => e,
            Err(e) => return Err(e),
        };
        // Skip control events
        if ev.event == "overflow" || ev.event == "shutdown" {
            let _ = writeln!(stderr, "SSE: {}: {}", ev.event, ev.data);
            continue;
        }
        if ctx.format == "json" {
            let _ = writeln!(stdout, "{}", ev.data);
        } else {
            let _ = writeln!(stdout, "[{}] {}", ev.event, ev.data);
        }
    }
    Ok(())
}

async fn watch_poll(
    args: WatchArgs,
    ctx: &CliContext,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    let interval = humantime::parse_duration(&args.poll_interval)
        .map_err(|e| CliError::Other(format!("parse poll-interval {:?}: {e}", args.poll_interval)))?;

    // Cursor-based poll: first poll gets latest; subsequent polls use replay endpoint.
    let mut last_ts: Option<String> = None;
    let mut last_id: Option<String> = None;

    loop {
        let (events_v, _total, _limit, _offset) = if last_ts.is_some() {
            // Subsequent poll: use replay endpoint.
            let after_ts = last_ts.clone().unwrap_or_default();
            // Try to interpret as unix ms and convert to RFC3339Nano.
            let after_ts_fmt = if let Ok(ms) = after_ts.parse::<i64>() {
                DateTime::from_timestamp_millis(ms)
                    .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
                    .unwrap_or(after_ts)
            } else {
                after_ts
            };
            let mut q = url::form_urlencoded::Serializer::new(String::new());
            q.append_pair("after_ts", &after_ts_fmt);
            q.append_pair("after_id", last_id.as_deref().unwrap_or(""));
            q.append_pair("limit", "20");
            let path = if let Some(id) = &args.agent_id {
                format!("/api/v1/agents/{id}/events/replay?{}", q.finish())
            } else {
                format!("/api/v1/events/replay?{}", q.finish())
            };
            // Response shape: `{events: [...]}` — use do_request_list.
            match ctx.client.do_request_list(reqwest::Method::GET, &path, None).await {
                Ok(tuple) => tuple,
                Err(e) => {
                    let _ = writeln!(stderr, "watch error: {e}");
                    tokio::time::sleep(interval).await;
                    continue;
                }
            }
        } else {
            // First poll: list endpoint.
            let mut q = url::form_urlencoded::Serializer::new(String::new());
            q.append_pair("limit", "20");
            if let Some(a) = &args.agent_id {
                q.append_pair("agentId", a);
            }
            let path = format!("/api/v1/events?{}", q.finish());
            match ctx.client.do_request_list(reqwest::Method::GET, &path, None).await {
                Ok(tuple) => tuple,
                Err(e) => {
                    let _ = writeln!(stderr, "watch error: {e}");
                    tokio::time::sleep(interval).await;
                    continue;
                }
            }
        };

        if !events_v.is_empty() {
            let data = Value::Array(events_v.clone());
            if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
                output::format(stdout, &ctx.format, COLUMNS, &data, None)
                    .map_err(|e| CliError::Other(e.to_string()))?;
            } else {
                for ev in &events_v {
                    let ts = ev.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
                    let ev_type = ev.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let aid = ev.get("agentId").and_then(|v| v.as_str()).unwrap_or("");
                    let _ = writeln!(stdout, "[{ts}] {ev_type} agent={aid}");
                }
            }
            // Update cursor from last event.
            if let Some(last) = events_v.last() {
                last_ts = extract_timestamp(last);
                last_id = last.get("id").and_then(|v| v.as_str()).map(String::from);
            }
        }

        tokio::time::sleep(interval).await;
    }
}

fn extract_timestamp(m: &Value) -> Option<String> {
    // Numeric timestamp (from server: unix milliseconds)
    if let Some(n) = m.get("timestamp").and_then(|v| v.as_f64()) {
        return Some((n as i64).to_string());
    }
    if let Some(s) = m.get("timestamp").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = m.get("createdAt").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    None
}
