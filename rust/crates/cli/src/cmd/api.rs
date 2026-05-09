use crate::client::ResponseKind;
use crate::client::sse::SseEvent;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use futures::StreamExt;
use serde_json::{Map, Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct ApiArgs {
    /// API endpoint path (e.g. /api/v1/agents)
    pub endpoint: String,

    /// HTTP method (default: GET, or POST if -f is used)
    #[arg(short = 'X', long = "method")]
    pub method: Option<String>,

    /// Request body field (key=value)
    #[arg(short = 'f', long = "field")]
    pub fields: Vec<String>,

    /// Stream SSE response
    #[arg(long)]
    pub stream: bool,

    /// Auto-paginate GET requests
    #[arg(long)]
    pub paginate: bool,
}

pub async fn run(args: ApiArgs, ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write) -> CliResult<()> {
    // Auto-detect method
    let method_str = match args.method.as_deref() {
        Some(m) => m.to_string(),
        None => {
            if args.fields.is_empty() {
                "GET".into()
            } else {
                "POST".into()
            }
        }
    };
    let method = reqwest::Method::from_bytes(method_str.as_bytes())
        .map_err(|e| CliError::Other(format!("invalid method {method_str:?}: {e}")))?;

    // Stream mode for SSE endpoints
    if args.stream {
        return api_stream(ctx, stdout, stderr, &args.endpoint).await;
    }

    // Build request body from -f fields
    let mut body_map: Option<Map<String, Value>> = None;
    if !args.fields.is_empty() {
        let mut m = Map::new();
        for f in &args.fields {
            let (key, value) = f
                .split_once('=')
                .ok_or_else(|| CliError::Other(format!("invalid field {f:?} (expected key=value)")))?;
            // Try to parse as JSON value (number, bool, null, array, object)
            match serde_json::from_str::<Value>(value) {
                Ok(v) => m.insert(key.to_string(), v),
                Err(_) => m.insert(key.to_string(), Value::String(value.to_string())),
            };
        }
        body_map = Some(m);
    }

    // Paginate
    if args.paginate && method == reqwest::Method::GET {
        return api_paginate(ctx, stdout, &args.endpoint, method).await;
    }

    // Execute request
    let body_value = body_map.as_ref().map(|m| Value::Object(m.clone()));
    let result = ctx.client.do_request(method.clone(), &args.endpoint, body_value.as_ref(), ResponseKind::Auto).await?;

    // Apply jq projection if set
    if !ctx.jq.is_empty() {
        let data =
            result.clone().unwrap_or_else(|| json!({ "method": method_str, "endpoint": args.endpoint, "ok": true }));
        return output::jq::apply(&data, &ctx.jq, stdout).map_err(|e| CliError::Other(e.to_string()));
    }

    match result {
        None => output::format_action(
            stdout,
            &ctx.format,
            &format!("{method_str} {} — ok", args.endpoint),
            &json!({ "method": method_str, "endpoint": args.endpoint, "ok": true }),
        )
        .map_err(|e| CliError::Other(e.to_string())),
        Some(data) => format_any(stdout, &ctx.format, &data),
    }
}

/// Connects to an SSE endpoint and streams events.
/// Matches `cli/cmd/api.go:apiStream`.
async fn api_stream(ctx: &CliContext, stdout: &mut dyn Write, stderr: &mut dyn Write, endpoint: &str) -> CliResult<()> {
    let stream = ctx.client.stream_sse(endpoint, 3).await?;
    tokio::pin!(stream);

    while let Some(item) = stream.next().await {
        let ev: SseEvent = item?;
        if ev.event == "overflow" || ev.event == "shutdown" {
            let _ = writeln!(stderr, "SSE: {}: {}", ev.event, ev.data);
            continue;
        }
        let _ = writeln!(stdout, "{}", ev.data);
    }
    Ok(())
}

/// Auto-fetches all pages for a GET endpoint.
/// Matches `cli/cmd/api.go:apiPaginate`.
async fn api_paginate(
    ctx: &CliContext,
    stdout: &mut dyn Write,
    endpoint: &str,
    method: reqwest::Method,
) -> CliResult<()> {
    let mut offset: u64 = 0;
    let limit: u64 = 100;
    let mut all_items: Vec<Value> = Vec::new();

    loop {
        let sep = if endpoint.contains('?') { '&' } else { '?' };
        let page_url = format!("{endpoint}{sep}limit={limit}&offset={offset}");

        let result =
            ctx.client.do_request(method.clone(), &page_url, None, ResponseKind::Auto).await?.unwrap_or(Value::Null);

        let items = extract_items(&result);
        if items.is_empty() {
            break;
        }
        let n = items.len();
        all_items.extend(items);
        if (n as u64) < limit {
            break;
        }
        offset += limit;
    }

    if all_items.is_empty() {
        return format_any(stdout, &ctx.format, &json!([]));
    }
    let data = Value::Array(all_items);
    output::format(stdout, &ctx.format, &[], &data, None).map_err(|e| CliError::Other(e.to_string()))
}

/// Tries to pull a slice of items from various response shapes.
/// Matches `cli/cmd/api.go:extractItems`.
fn extract_items(result: &Value) -> Vec<Value> {
    match result {
        Value::Array(arr) => arr.clone(),
        Value::Object(m) => {
            for key in ["data", "agents", "events", "groups", "items"] {
                if let Some(v) = m.get(key) {
                    return extract_items(v);
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Outputs an arbitrary JSON value.
/// Matches `cli/cmd/api.go:formatAny`.
fn format_any(w: &mut dyn Write, format: &str, data: &Value) -> CliResult<()> {
    match format {
        "json" | "yaml" => {
            // Pretty-print as JSON for both (Go's yaml path also falls back to JSON here).
            let bytes = serde_json::to_vec_pretty(data).map_err(|e| CliError::Other(e.to_string()))?;
            w.write_all(&bytes).map_err(|e| CliError::Other(e.to_string()))?;
            w.write_all(b"\n").map_err(|e| CliError::Other(e.to_string()))?;
            Ok(())
        }
        "quiet" => {
            let items = extract_items(data);
            if !items.is_empty() {
                output::quiet::write(w, &Value::Array(items), "id");
            }
            Ok(())
        }
        _ => {
            // Table: try structured output, fall back to JSON dump
            match data {
                Value::Object(_) => {
                    output::format(w, format, &[], data, None).map_err(|e| CliError::Other(e.to_string()))
                }
                Value::Array(arr) => {
                    let items: Vec<Value> = arr.iter().filter(|v| matches!(v, Value::Object(_))).cloned().collect();
                    if !items.is_empty() {
                        output::format(w, format, &[], &Value::Array(items), None)
                            .map_err(|e| CliError::Other(e.to_string()))
                    } else {
                        let bytes = serde_json::to_vec_pretty(data).map_err(|e| CliError::Other(e.to_string()))?;
                        w.write_all(&bytes).map_err(|e| CliError::Other(e.to_string()))?;
                        w.write_all(b"\n").map_err(|e| CliError::Other(e.to_string()))?;
                        Ok(())
                    }
                }
                _ => {
                    let bytes = serde_json::to_vec_pretty(data).map_err(|e| CliError::Other(e.to_string()))?;
                    w.write_all(&bytes).map_err(|e| CliError::Other(e.to_string()))?;
                    w.write_all(b"\n").map_err(|e| CliError::Other(e.to_string()))?;
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_items_from_array() {
        let v = json!([{"id": "a"}, {"id": "b"}]);
        let items = extract_items(&v);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn extract_items_from_data_envelope() {
        let v = json!({"data": [{"id": "x"}], "total": 1});
        let items = extract_items(&v);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], "x");
    }

    #[test]
    fn extract_items_from_agents_envelope() {
        let v = json!({"agents": [{"id": "a1"}, {"id": "a2"}]});
        let items = extract_items(&v);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn extract_items_from_events_envelope() {
        let v = json!({"events": [{"id": "e1"}]});
        let items = extract_items(&v);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn extract_items_from_groups_envelope() {
        let v = json!({"groups": [{"id": "g1"}, {"id": "g2"}, {"id": "g3"}]});
        let items = extract_items(&v);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn extract_items_from_items_envelope() {
        let v = json!({"items": [{"id": "i1"}]});
        let items = extract_items(&v);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn extract_items_empty_for_scalar() {
        let v = json!(42);
        assert!(extract_items(&v).is_empty());
    }

    #[test]
    fn extract_items_empty_for_no_known_key() {
        let v = json!({"unknown_key": [{"id": "x"}]});
        assert!(extract_items(&v).is_empty());
    }

    #[test]
    fn format_any_json_pretty_prints() {
        let mut buf = Vec::new();
        let data = json!({"id": "x", "name": "test"});
        format_any(&mut buf, "json", &data).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"id\": \"x\""));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn format_any_yaml_falls_back_to_json() {
        let mut buf = Vec::new();
        let data = json!({"id": "x"});
        format_any(&mut buf, "yaml", &data).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // Falls back to JSON pretty-print
        assert!(s.contains("\"id\""));
    }

    #[test]
    fn format_any_quiet_extracts_ids() {
        let mut buf = Vec::new();
        let data = json!([{"id": "a"}, {"id": "b"}]);
        format_any(&mut buf, "quiet", &data).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains('a'));
        assert!(s.contains('b'));
    }

    #[test]
    fn format_any_quiet_no_output_when_no_id_items() {
        let mut buf = Vec::new();
        let data = json!(42);
        format_any(&mut buf, "quiet", &data).unwrap();
        // scalar has no extractable items, no output
        assert!(buf.is_empty());
    }

    #[test]
    fn field_parsing_json_value() {
        // Simulate parsing "-f count=42" — should parse 42 as JSON number
        let value = "42";
        let parsed: Value = serde_json::from_str(value).unwrap();
        assert_eq!(parsed, json!(42));
    }

    #[test]
    fn field_parsing_string_fallback() {
        // Simulate parsing "-f name=my-agent" — not valid JSON, stays as string
        let value = "my-agent";
        let result: Result<Value, _> = serde_json::from_str(value);
        assert!(result.is_err());
        // Falls back to string
        let v = Value::String(value.to_string());
        assert_eq!(v, json!("my-agent"));
    }

    #[test]
    fn field_parsing_bool() {
        let value = "true";
        let parsed: Value = serde_json::from_str(value).unwrap();
        assert_eq!(parsed, json!(true));
    }

    #[test]
    fn field_parsing_null() {
        let value = "null";
        let parsed: Value = serde_json::from_str(value).unwrap();
        assert_eq!(parsed, json!(null));
    }
}
