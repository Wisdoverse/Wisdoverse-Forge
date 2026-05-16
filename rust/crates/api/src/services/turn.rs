//! Turn projection and cursor pagination for the chat read path.

use std::collections::HashSet;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::Event;

use crate::domain::turn::{TurnCursor, TurnListPage};
use crate::repositories::event::EventRepository;

const MAX_INPUT_PREVIEW: usize = 200;
const MAX_OUTPUT_PREVIEW: usize = 500;
const INTERRUPTED_AFTER_MS: i64 = 120_000;

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

#[derive(Debug)]
pub struct BuildTurnsResult {
    pub turns: Vec<Turn>,
    pub unknown_event_type_count: usize,
    pub deduplicated_event_count: usize,
}

struct BuildContext {
    turns: Vec<Turn>,
    current_turn: Option<usize>,
    sequence: i64,
    unknown_event_type_count: usize,
}

pub struct TurnService {
    repo: EventRepository,
}

impl TurnService {
    pub fn new(repo: EventRepository) -> Self {
        Self { repo }
    }

    pub async fn list_page(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        cursor: Option<&str>,
        limit: i64,
    ) -> AppResult<TurnPage> {
        let page = TurnListPage::new(limit);
        let cursor = cursor.map(TurnCursor::decode).transpose()?;
        let events = self.repo.list_by_agent_chronological(scope, agent_id).await?;
        let last_event = events.last().map(|event| LastEventCursor {
            timestamp: event.created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
            id: event.id.as_uuid().to_string(),
        });

        let built = build_turns(&events, Utc::now().timestamp_millis());
        let total_turn_count = built.turns.len();
        let eligible_turns: Vec<Turn> = match cursor {
            Some(cursor) => {
                built.turns.into_iter().filter(|turn| cursor.is_turn_before(turn.started_at, &turn.id)).collect()
            }
            None => built.turns,
        };

        let page_start = page.start_index(eligible_turns.len());
        let turns = eligible_turns[page_start..].to_vec();
        let has_more = page.has_more(eligible_turns.len());
        let cursor = if has_more {
            turns.first().map(|turn| TurnCursor::new(turn.started_at, turn.id.clone()).encode()).transpose()?
        } else {
            None
        };

        Ok(TurnPage { turns, cursor, has_more, total_turn_count, last_event })
    }
}

pub fn default_turn_limit() -> i64 {
    TurnListPage::DEFAULT_LIMIT
}

pub fn build_turns(events: &[Event], now_ms: i64) -> BuildTurnsResult {
    let mut sorted: Vec<&Event> = events.iter().collect();
    sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.as_uuid().cmp(&b.id.as_uuid())));

    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(sorted.len());
    for event in sorted {
        if seen.insert(event.id.as_uuid()) {
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

fn process_event(ctx: &mut BuildContext, event: &Event) {
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

fn handle_prompt_submit(ctx: &mut BuildContext, event: &Event) {
    finalize_open_current_turn(ctx, event_ms(event));

    ctx.sequence += 1;
    ctx.turns.push(create_turn(TurnSeed {
        id: event.id.as_uuid().to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "user",
        status: "complete",
        started_at: event_ms(event),
        cli_tool: payload_str(&event.payload, "cliTool").map(str::to_owned),
    }));
    let user_index = ctx.turns.len() - 1;
    ctx.turns[user_index].prompt = payload_str(&event.payload, "prompt").map(str::to_owned);
    ctx.turns[user_index].images = payload_string_array(&event.payload, "images");
    ctx.turns[user_index].completed_at = Some(event_ms(event));
    ctx.turns[user_index].duration_ms = Some(0);

    ctx.sequence += 1;
    ctx.turns.push(create_turn(TurnSeed {
        id: format!("{}-assistant", event.id.as_uuid()),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "assistant",
        status: "thinking",
        started_at: event_ms(event),
        cli_tool: payload_str(&event.payload, "cliTool").map(str::to_owned),
    }));
    ctx.current_turn = Some(ctx.turns.len() - 1);
}

fn handle_pre_tool_use(ctx: &mut BuildContext, event: &Event) {
    if current_assistant_mut(ctx).is_none() {
        ctx.sequence += 1;
        ctx.turns.push(create_turn(TurnSeed {
            id: format!("{}-assistant", event.id.as_uuid()),
            session_id: event.session_id.clone().unwrap_or_default(),
            sequence: ctx.sequence,
            turn_type: "assistant",
            status: "tool_use",
            started_at: event_ms(event),
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
    let step_id =
        payload_str(&event.payload, "toolUseId").map(str::to_owned).unwrap_or_else(|| event.id.as_uuid().to_string());
    turn.steps.push(TurnStep {
        id: step_id,
        tool_name: tool.to_string(),
        input: truncate_text(&stringify_value(tool_input), MAX_INPUT_PREVIEW),
        output: None,
        has_full_content: true,
        success: None,
        duration_ms: None,
        started_at: event_ms(event),
        status: "pending".to_string(),
        is_subagent: matches!(tool, "Task" | "dispatch_agent"),
        metadata: extract_step_metadata(tool, tool_input),
    });
    turn.raw_event_count += 1;
}

fn handle_post_tool_use(ctx: &mut BuildContext, event: &Event) {
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

fn handle_stop(ctx: &mut BuildContext, event: &Event) {
    let Some(turn) = current_assistant_mut(ctx) else {
        return;
    };
    if let Some(response) = payload_str(&event.payload, "response") {
        turn.response = Some(response.to_string());
    }
    finalize_turn(turn, event_ms(event));
}

fn handle_session_start(ctx: &mut BuildContext, event: &Event) {
    finalize_open_current_turn(ctx, event_ms(event));

    ctx.sequence += 1;
    let mut turn = create_turn(TurnSeed {
        id: event.id.as_uuid().to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "system",
        status: "complete",
        started_at: event_ms(event),
        cli_tool: None,
    });
    let source = payload_str(&event.payload, "source").unwrap_or("startup");
    turn.response = Some(format!("Session {source}"));
    turn.completed_at = Some(event_ms(event));
    turn.duration_ms = Some(0);
    ctx.turns.push(turn);
    ctx.current_turn = None;
}

fn handle_session_end(ctx: &mut BuildContext, event: &Event) {
    if let Some(index) = ctx.current_turn
        && !is_terminal_status(&ctx.turns[index].status)
    {
        ctx.turns[index].status = "interrupted".to_string();
        finalize_turn(&mut ctx.turns[index], event_ms(event));
    }

    ctx.sequence += 1;
    let mut turn = create_turn(TurnSeed {
        id: event.id.as_uuid().to_string(),
        session_id: event.session_id.clone().unwrap_or_default(),
        sequence: ctx.sequence,
        turn_type: "system",
        status: "complete",
        started_at: event_ms(event),
        cli_tool: None,
    });
    let reason = payload_str(&event.payload, "reason").unwrap_or("other");
    turn.response = Some(format!("Session ended: {reason}"));
    turn.completed_at = Some(event_ms(event));
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

fn event_ms(event: &Event) -> i64 {
    event.created_at.timestamp_millis()
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
    use agentforge_core::{EventId, OrgId};

    fn event(id: &str, event_type: &str, ms: i64, payload: Value) -> Event {
        let uuid = uuid::Uuid::parse_str(id).unwrap_or_else(|_| uuid::Uuid::new_v4());
        Event {
            id: EventId::from(uuid),
            organization_id: OrgId::from(uuid::Uuid::new_v4()),
            agent_id: AgentId::from(uuid::Uuid::new_v4()),
            run_id: None,
            event_type: event_type.to_string(),
            payload,
            session_id: Some("cli-session".to_string()),
            created_at: chrono::DateTime::from_timestamp_millis(ms).expect("valid timestamp"),
        }
    }

    #[test]
    fn build_turns_projects_prompt_tool_and_stop() {
        let events = vec![
            event(
                "00000000-0000-0000-0000-000000000001",
                "user_prompt_submit",
                1_000,
                serde_json::json!({"prompt": "hello", "cliTool": "claude"}),
            ),
            event(
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
            event(
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
            event("00000000-0000-0000-0000-000000000004", "stop", 4_000, serde_json::json!({"response": "done"})),
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
}
