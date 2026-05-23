//! Turn read-model pagination and cursor policies.

use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_core::{AppResult, ErrorKind};
use agentforge_db::entities::Event;

const DEFAULT_TURN_LIMIT: i64 = 50;
const MAX_TURN_LIMIT: i64 = 100;
const MAX_INPUT_PREVIEW: usize = 200;
const MAX_OUTPUT_PREVIEW: usize = 500;
const INTERRUPTED_AFTER_MS: i64 = 120_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastEventCursor {
    pub timestamp: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnPage {
    pub turns: Vec<Turn>,
    pub cursor: Option<String>,
    pub has_more: bool,
    pub total_turn_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event: Option<LastEventCursor>,
}

pub(crate) fn turn_page_response(page: &TurnPage) -> Value {
    json!({
        "ok": true,
        "turns": page.turns,
        "cursor": page.cursor,
        "hasMore": page.has_more,
        "totalTurnCount": page.total_turn_count,
        "lastEvent": page.last_event,
    })
}

/// Validated page size for turn read-model pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TurnListPage {
    limit: usize,
}

impl TurnListPage {
    pub(crate) const DEFAULT_LIMIT: i64 = DEFAULT_TURN_LIMIT;

    pub(crate) fn new(limit: i64) -> Self {
        Self { limit: limit.clamp(1, MAX_TURN_LIMIT) as usize }
    }

    pub(crate) fn start_index(self, eligible_count: usize) -> usize {
        eligible_count.saturating_sub(self.limit)
    }

    pub(crate) fn has_more(self, eligible_count: usize) -> bool {
        self.start_index(eligible_count) > 0
    }
}

/// Stable cursor for fetching turns that come before the current page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TurnCursor {
    started_at: i64,
    id: String,
}

impl TurnCursor {
    pub(crate) fn new(started_at: i64, id: impl Into<String>) -> Self {
        Self { started_at, id: id.into() }
    }

    pub(crate) fn is_turn_before(&self, started_at: i64, id: &str) -> bool {
        started_at < self.started_at || (started_at == self.started_at && id < self.id.as_str())
    }

    pub(crate) fn encode(&self) -> AppResult<String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encode turn cursor: {err}")))?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(crate) fn decode(raw: &str) -> AppResult<Self> {
        let bytes =
            URL_SAFE_NO_PAD.decode(raw).map_err(|_| ErrorKind::Validation("invalid turn cursor".to_string()))?;
        serde_json::from_slice::<Self>(&bytes)
            .map_err(|_| ErrorKind::Validation("invalid turn cursor".to_string()).into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TurnProjectionEvent {
    pub(crate) id: Uuid,
    pub(crate) event_type: String,
    pub(crate) payload: Value,
    pub(crate) session_id: Option<String>,
    pub(crate) created_at_ms: i64,
}

pub(crate) fn turn_projection_event(event: &Event) -> TurnProjectionEvent {
    TurnProjectionEvent {
        id: event.id.as_uuid(),
        event_type: event.event_type.clone(),
        payload: event.payload.clone(),
        session_id: event.session_id.clone(),
        created_at_ms: event.created_at.timestamp_millis(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnStep {
    pub id: String,
    pub tool_name: String,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub has_full_content: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub started_at: i64,
    pub status: String,
    pub is_subagent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<StepMetadata>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub sequence: i64,
    #[serde(rename = "type")]
    pub turn_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    pub steps: Vec<TurnStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_tool: Option<String>,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub raw_event_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
}

#[derive(Debug)]
pub(crate) struct BuildTurnsResult {
    pub(crate) turns: Vec<Turn>,
    pub(crate) unknown_event_type_count: usize,
    pub(crate) deduplicated_event_count: usize,
}

struct BuildContext {
    turns: Vec<Turn>,
    current_turn: Option<usize>,
    sequence: i64,
    unknown_event_type_count: usize,
}

pub(crate) fn build_turns(events: &[TurnProjectionEvent], now_ms: i64) -> BuildTurnsResult {
    let mut sorted: Vec<&TurnProjectionEvent> = events.iter().collect();
    sorted.sort_by(|a, b| a.created_at_ms.cmp(&b.created_at_ms).then_with(|| a.id.cmp(&b.id)));

    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(sorted.len());
    for event in sorted {
        if seen.insert(event.id) {
            deduped.push(event);
        }
    }

    let mut ctx = BuildContext { turns: Vec::new(), current_turn: None, sequence: 0, unknown_event_type_count: 0 };

    for event in deduped.iter().copied() {
        if !is_known_event_type(&event.event_type) {
            ctx.unknown_event_type_count += 1;
            continue;
        }
        process_event(&mut ctx, event);
    }

    if let Some(index) = ctx.current_turn
        && ctx.turns[index].turn_type == "assistant"
        && !is_terminal_status(&ctx.turns[index].status)
    {
        let last_activity = last_activity_time(&ctx.turns[index], now_ms);
        if now_ms - last_activity > INTERRUPTED_AFTER_MS {
            ctx.turns[index].status = "interrupted".to_string();
            finalize_turn(&mut ctx.turns[index], last_activity);
        }
    }

    BuildTurnsResult {
        turns: ctx.turns,
        unknown_event_type_count: ctx.unknown_event_type_count,
        deduplicated_event_count: events.len().saturating_sub(deduped.len()),
    }
}

fn process_event(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    match event.event_type.as_str() {
        "user_prompt_submit" => handle_prompt_submit(ctx, event),
        "pre_tool_use" => handle_pre_tool_use(ctx, event),
        "post_tool_use" => handle_post_tool_use(ctx, event),
        "stop" => handle_stop(ctx, event),
        "session_start" => handle_session_start(ctx, event),
        "session_end" => handle_session_end(ctx, event),
        "subagent_stop" | "notification" | "pre_compact" | "terminal_output" | "text_stream" => {
            if let Some(turn) = current_assistant_mut(ctx) {
                turn.raw_event_count += 1;
            }
        }
        _ => {}
    }
}

fn handle_prompt_submit(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    finalize_open_current_turn(ctx, event.created_at_ms);

    ctx.sequence += 1;
    ctx.turns.push(create_turn(TurnSeed {
        id: event.id.to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "user",
        status: "complete",
        started_at: event.created_at_ms,
        cli_tool: payload_str(&event.payload, "cliTool").map(str::to_owned),
    }));
    let user_index = ctx.turns.len() - 1;
    ctx.turns[user_index].prompt = payload_str(&event.payload, "prompt").map(str::to_owned);
    ctx.turns[user_index].images = payload_string_array(&event.payload, "images");
    ctx.turns[user_index].completed_at = Some(event.created_at_ms);
    ctx.turns[user_index].duration_ms = Some(0);

    ctx.sequence += 1;
    ctx.turns.push(create_turn(TurnSeed {
        id: format!("{}-assistant", event.id),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "assistant",
        status: "thinking",
        started_at: event.created_at_ms,
        cli_tool: payload_str(&event.payload, "cliTool").map(str::to_owned),
    }));
    ctx.current_turn = Some(ctx.turns.len() - 1);
}

fn handle_pre_tool_use(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    if current_assistant_mut(ctx).is_none() {
        ctx.sequence += 1;
        ctx.turns.push(create_turn(TurnSeed {
            id: format!("{}-assistant", event.id),
            session_id: event.session_id.clone().unwrap_or_default(),
            sequence: ctx.sequence,
            turn_type: "assistant",
            status: "tool_use",
            started_at: event.created_at_ms,
            cli_tool: payload_str(&event.payload, "cliTool").map(str::to_owned),
        }));
        ctx.current_turn = Some(ctx.turns.len() - 1);
    }

    let Some(turn) = current_assistant_mut(ctx) else {
        return;
    };
    turn.status = "tool_use".to_string();
    if turn.thinking.is_none() {
        turn.thinking = payload_str(&event.payload, "assistantText").map(str::to_owned);
    }

    let tool = payload_str(&event.payload, "tool").unwrap_or("unknown");
    let tool_input = payload_value(&event.payload, "toolInput").unwrap_or(&Value::Null);
    let step_id = payload_str(&event.payload, "toolUseId").map(str::to_owned).unwrap_or_else(|| event.id.to_string());
    turn.steps.push(TurnStep {
        id: step_id,
        tool_name: tool.to_string(),
        input: truncate_text(&stringify_value(tool_input), MAX_INPUT_PREVIEW),
        output: None,
        has_full_content: true,
        success: None,
        duration_ms: None,
        started_at: event.created_at_ms,
        status: "pending".to_string(),
        is_subagent: matches!(tool, "Task" | "dispatch_agent"),
        metadata: extract_step_metadata(tool, tool_input),
    });
    turn.raw_event_count += 1;
}

fn handle_post_tool_use(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    let Some(turn) = current_assistant_mut(ctx) else {
        return;
    };
    let Some(tool_use_id) = payload_str(&event.payload, "toolUseId") else {
        turn.raw_event_count += 1;
        return;
    };

    if let Some(step) = turn.steps.iter_mut().find(|step| step.id == tool_use_id) {
        let output = payload_value(&event.payload, "toolResponse").unwrap_or(&Value::Null);
        let success = payload_bool(&event.payload, "success").unwrap_or(true);
        step.output = Some(truncate_text(&stringify_value(output), MAX_OUTPUT_PREVIEW));
        step.success = Some(success);
        step.duration_ms = payload_i64(&event.payload, "duration");
        step.status = if success { "complete" } else { "error" }.to_string();
    }
    turn.raw_event_count += 1;
}

fn handle_stop(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    let Some(turn) = current_assistant_mut(ctx) else {
        return;
    };
    if let Some(response) = payload_str(&event.payload, "response") {
        turn.response = Some(response.to_string());
    }
    finalize_turn(turn, event.created_at_ms);
}

fn handle_session_start(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    finalize_open_current_turn(ctx, event.created_at_ms);

    ctx.sequence += 1;
    let mut turn = create_turn(TurnSeed {
        id: event.id.to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "system",
        status: "complete",
        started_at: event.created_at_ms,
        cli_tool: None,
    });
    let source = payload_str(&event.payload, "source").unwrap_or("startup");
    turn.response = Some(format!("Session {source}"));
    turn.completed_at = Some(event.created_at_ms);
    turn.duration_ms = Some(0);
    ctx.turns.push(turn);
    ctx.current_turn = None;
}

fn handle_session_end(ctx: &mut BuildContext, event: &TurnProjectionEvent) {
    if let Some(index) = ctx.current_turn
        && !is_terminal_status(&ctx.turns[index].status)
    {
        ctx.turns[index].status = "interrupted".to_string();
        finalize_turn(&mut ctx.turns[index], event.created_at_ms);
    }

    ctx.sequence += 1;
    let mut turn = create_turn(TurnSeed {
        id: event.id.to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "system",
        status: "complete",
        started_at: event.created_at_ms,
        cli_tool: None,
    });
    let reason = payload_str(&event.payload, "reason").unwrap_or("other");
    turn.response = Some(format!("Session ended: {reason}"));
    turn.completed_at = Some(event.created_at_ms);
    turn.duration_ms = Some(0);
    ctx.turns.push(turn);
    ctx.current_turn = None;
}

struct TurnSeed {
    id: String,
    session_id: String,
    sequence: i64,
    turn_type: &'static str,
    status: &'static str,
    started_at: i64,
    cli_tool: Option<String>,
}

fn create_turn(seed: TurnSeed) -> Turn {
    Turn {
        id: seed.id,
        session_id: seed.session_id,
        sequence: seed.sequence,
        turn_type: seed.turn_type.to_string(),
        status: seed.status.to_string(),
        prompt: None,
        images: None,
        thinking: None,
        response: None,
        steps: Vec::new(),
        cli_tool: seed.cli_tool,
        started_at: seed.started_at,
        completed_at: None,
        duration_ms: None,
        raw_event_count: 1,
        is_favorite: None,
    }
}

fn current_assistant_mut(ctx: &mut BuildContext) -> Option<&mut Turn> {
    let index = ctx.current_turn?;
    let turn = ctx.turns.get_mut(index)?;
    (turn.turn_type == "assistant").then_some(turn)
}

fn finalize_open_current_turn(ctx: &mut BuildContext, timestamp: i64) {
    if let Some(index) = ctx.current_turn
        && !is_terminal_status(&ctx.turns[index].status)
    {
        finalize_turn(&mut ctx.turns[index], timestamp);
    }
}

fn finalize_turn(turn: &mut Turn, timestamp: i64) {
    if matches!(turn.status.as_str(), "thinking" | "tool_use") {
        turn.status = "complete".to_string();
    }
    for step in &mut turn.steps {
        if step.status == "pending" {
            step.status = "timeout".to_string();
        }
    }
    turn.completed_at = Some(timestamp);
    turn.duration_ms = Some(timestamp - turn.started_at);
}

fn last_activity_time(turn: &Turn, fallback: i64) -> i64 {
    if let Some(step) = turn.steps.last() {
        return step.started_at + step.duration_ms.unwrap_or(0);
    }
    if turn.started_at > 0 { turn.started_at } else { fallback }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "complete" | "error")
}

fn is_known_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "user_prompt_submit"
            | "pre_tool_use"
            | "post_tool_use"
            | "stop"
            | "subagent_stop"
            | "session_start"
            | "session_end"
            | "notification"
            | "pre_compact"
            | "terminal_output"
            | "text_stream"
    )
}

fn payload_value<'a>(payload: &'a Value, camel: &str) -> Option<&'a Value> {
    let snake = camel_to_snake(camel);
    payload.get(camel).or_else(|| payload.get(&snake))
}

fn payload_str<'a>(payload: &'a Value, camel: &str) -> Option<&'a str> {
    payload_value(payload, camel).and_then(Value::as_str)
}

fn payload_bool(payload: &Value, camel: &str) -> Option<bool> {
    payload_value(payload, camel).and_then(Value::as_bool)
}

fn payload_i64(payload: &Value, camel: &str) -> Option<i64> {
    payload_value(payload, camel).and_then(Value::as_i64)
}

fn payload_string_array(payload: &Value, camel: &str) -> Option<Vec<String>> {
    let values = payload_value(payload, camel)?.as_array()?;
    let strings: Vec<String> = values.iter().filter_map(Value::as_str).map(str::to_owned).collect();
    (!strings.is_empty()).then_some(strings)
}

fn camel_to_snake(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for ch in input.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn stringify_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_len).collect::<String>())
}

fn extract_step_metadata(tool: &str, input: &Value) -> Option<StepMetadata> {
    let field = |key: &str| payload_str(input, key).map(str::to_owned);
    match tool {
        "Read" | "Write" | "Edit" | "Glob" => field("filePath")
            .or_else(|| field("file_path"))
            .map(|file_path| StepMetadata { file_path: Some(file_path), command: None, language: None }),
        "Bash" => {
            field("command").map(|command| StepMetadata { file_path: None, command: Some(command), language: None })
        }
        "Grep" => field("pattern").map(|pattern| StepMetadata {
            file_path: None,
            command: Some(format!("grep: {pattern}")),
            language: None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_list_page_clamps_limit() {
        assert_eq!(TurnListPage::new(0).start_index(10), 9);
        assert_eq!(TurnListPage::new(50).start_index(60), 10);
        assert_eq!(TurnListPage::new(500).start_index(200), 100);
    }

    #[test]
    fn turn_list_page_computes_tail_window() {
        let page = TurnListPage::new(3);

        assert_eq!(page.start_index(10), 7);
        assert!(page.has_more(10));
        assert_eq!(page.start_index(2), 0);
        assert!(!page.has_more(2));
    }

    #[test]
    fn turn_page_response_preserves_frontend_envelope() {
        let page = TurnPage {
            turns: Vec::new(),
            cursor: Some("cursor".to_owned()),
            has_more: true,
            total_turn_count: 3,
            last_event: Some(LastEventCursor {
                timestamp: "2026-04-20T12:00:00.000Z".to_owned(),
                id: "evt".to_owned(),
            }),
        };
        let body = turn_page_response(&page);

        assert_eq!(body["ok"], true);
        assert!(body["turns"].is_array());
        assert_eq!(body["cursor"], "cursor");
        assert_eq!(body["hasMore"], true);
        assert_eq!(body["totalTurnCount"], 3);
        assert_eq!(body["lastEvent"]["id"], "evt");
    }

    #[test]
    fn turn_cursor_round_trips() {
        let raw = TurnCursor::new(123, "turn-1").encode().unwrap();
        let decoded = TurnCursor::decode(&raw).unwrap();

        assert_eq!(decoded, TurnCursor::new(123, "turn-1"));
    }

    #[test]
    fn turn_cursor_rejects_invalid_payloads() {
        assert!(TurnCursor::decode("not-base64").is_err());
        assert!(TurnCursor::decode(&URL_SAFE_NO_PAD.encode(b"{\"started_at\":1}")).is_err());
    }

    #[test]
    fn turn_cursor_compares_timestamp_then_id() {
        let cursor = TurnCursor::new(100, "turn-b");

        assert!(cursor.is_turn_before(99, "turn-z"));
        assert!(cursor.is_turn_before(100, "turn-a"));
        assert!(!cursor.is_turn_before(100, "turn-b"));
        assert!(!cursor.is_turn_before(101, "turn-a"));
    }

    fn projection_event(id: &str, event_type: &str, ms: i64, payload: Value) -> TurnProjectionEvent {
        TurnProjectionEvent {
            id: Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()),
            event_type: event_type.to_string(),
            payload,
            session_id: Some("cli-session".to_string()),
            created_at_ms: ms,
        }
    }

    #[test]
    fn build_turns_projects_prompt_tool_and_stop() {
        let events = vec![
            projection_event(
                "00000000-0000-0000-0000-000000000001",
                "user_prompt_submit",
                1_000,
                serde_json::json!({"prompt": "hello", "cliTool": "claude"}),
            ),
            projection_event(
                "00000000-0000-0000-0000-000000000002",
                "pre_tool_use",
                2_000,
                serde_json::json!({
                    "tool": "Read",
                    "toolUseId": "tool-1",
                    "toolInput": {"file_path": "/tmp/a.rs"},
                    "assistantText": "I'll inspect it"
                }),
            ),
            projection_event(
                "00000000-0000-0000-0000-000000000003",
                "post_tool_use",
                3_000,
                serde_json::json!({
                    "toolUseId": "tool-1",
                    "toolResponse": {"result": "ok"},
                    "success": true,
                    "duration": 90
                }),
            ),
            projection_event(
                "00000000-0000-0000-0000-000000000004",
                "stop",
                4_000,
                serde_json::json!({"response": "done"}),
            ),
        ];

        let result = build_turns(&events, 4_000);

        assert_eq!(result.turns.len(), 2);
        assert_eq!(result.turns[0].turn_type, "user");
        assert_eq!(result.turns[0].prompt.as_deref(), Some("hello"));
        assert_eq!(result.turns[1].turn_type, "assistant");
        assert_eq!(result.turns[1].status, "complete");
        assert_eq!(result.turns[1].response.as_deref(), Some("done"));
        assert_eq!(result.turns[1].steps[0].tool_name, "Read");
        assert_eq!(result.turns[1].steps[0].status, "complete");
        assert_eq!(result.turns[1].steps[0].metadata.as_ref().unwrap().file_path.as_deref(), Some("/tmp/a.rs"));
    }

    #[test]
    fn turn_projection_event_copies_protocol_fields_from_event_row() {
        use agentforge_core::{AgentId, OrgId};
        use agentforge_db::entities::Event;
        use chrono::TimeZone;

        let event_uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap();
        let created_at = chrono::Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let event = Event {
            id: event_uuid.into(),
            organization_id: OrgId::new(),
            agent_id: AgentId::new(),
            run_id: None,
            event_type: "user_prompt_submit".to_string(),
            payload: serde_json::json!({"prompt": "hello"}),
            session_id: Some("cli-session-1".to_string()),
            created_at,
        };

        let projection = turn_projection_event(&event);

        assert_eq!(projection.id, event_uuid);
        assert_eq!(projection.event_type, "user_prompt_submit");
        assert_eq!(projection.payload, serde_json::json!({"prompt": "hello"}));
        assert_eq!(projection.session_id.as_deref(), Some("cli-session-1"));
        assert_eq!(projection.created_at_ms, 1_700_000_000_000);
    }

    #[test]
    fn build_turns_counts_unknown_and_deduplicated_events() {
        let duplicate_id = "00000000-0000-0000-0000-000000000005";
        let events = vec![
            projection_event(duplicate_id, "unknown_event", 1_000, serde_json::json!({})),
            projection_event(duplicate_id, "unknown_event", 1_000, serde_json::json!({})),
        ];

        let result = build_turns(&events, 1_000);

        assert!(result.turns.is_empty());
        assert_eq!(result.unknown_event_type_count, 1);
        assert_eq!(result.deduplicated_event_count, 1);
    }
}
