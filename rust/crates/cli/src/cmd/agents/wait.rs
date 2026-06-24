use super::COLUMNS;
use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use serde_json::{Value, json};
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct WaitArgs {
    /// Agent ID
    pub id: String,
    /// Target status to wait for
    #[arg(long, default_value = "idle")]
    pub status: String,
    /// Wait for event type (e.g. event=permission_request)
    #[arg(long = "wait-for")]
    pub wait_for: Option<String>,
    /// Maximum time to wait (e.g. 30s, 5m, 1h)
    #[arg(long, default_value = "5m")]
    pub timeout: String,
}

pub async fn run(args: WaitArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let dur = humantime::parse_duration(&args.timeout)
        .map_err(|e| CliError::Other(format!("parse timeout {:?}: {e}", args.timeout)))?;

    // Parse --wait-for: must be "event=<type>".
    let mut wait_event: Option<String> = None;
    if let Some(wf) = &args.wait_for {
        let (k, v) = wf
            .split_once('=')
            .ok_or_else(|| CliError::Other(format!("invalid --wait-for {wf:?} (expected event=<type>)")))?;
        if k != "event" {
            return Err(CliError::Other(format!("invalid --wait-for {wf:?} (expected event=<type>)")));
        }
        wait_event = Some(v.to_string());
    }

    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(ev) = &wait_event {
        q.append_pair("event", ev);
    } else {
        q.append_pair("status", &args.status);
    }
    q.append_pair("timeout", &args.timeout);
    let path = format!("/api/v1/agents/{}/wait?{}", args.id, q.finish());

    // Try the server-side waiter first.
    let waiter_result = ctx.client.do_request(reqwest::Method::GET, &path, None, ResponseKind::Auto).await;

    match waiter_result {
        Ok(result_opt) => {
            let result = result_opt.unwrap_or(Value::Null);
            // Check in-band timeout flag.
            if let Some(true) = result.get("timedOut").and_then(|v| v.as_bool()) {
                return Err(CliError::WaitTimeout(format!(
                    "timeout: agent {} did not reach target within {}",
                    args.id, args.timeout
                )));
            }
            output::format(stdout, &ctx.format, COLUMNS, &result, None).map_err(|e| CliError::Other(e.to_string()))?;
            Ok(())
        }
        Err(err) => {
            // 408 → exit code 6 (wait timeout)
            if let CliError::Api(api) = &err
                && api.status == 408
            {
                return Err(CliError::WaitTimeout(format!(
                    "timeout: agent {} did not reach target within {}",
                    args.id, args.timeout
                )));
            }
            // Transport errors (connection reset, 502/503/504) on status-wait only
            // → client-side poll fallback. Event-type waits can't fall back.
            // Mirrors Go's isPromptTransportError which checks BOTH api 502/503/504
            // AND net.OpError (any network-layer error).
            let is_transport = match &err {
                CliError::Api(api) => matches!(api.status, 502..=504),
                CliError::Transport(_) => true,
                _ => false,
            };
            if wait_event.is_none() && is_transport {
                super::create::poll_until_status(ctx, &args.id, &args.status, dur).await?;
                return output::format_action(
                    stdout,
                    &ctx.format,
                    &format!("Agent {} reached status {:?}.", args.id, args.status),
                    &json!({ "id": args.id, "status": args.status }),
                )
                .map_err(|e| CliError::Other(e.to_string()));
            }
            Err(err)
        }
    }
}
