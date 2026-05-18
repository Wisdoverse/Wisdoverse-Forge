//! Turn projection service and cursor pagination for the chat read path.

use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use agentforge_core::{AgentId, AppResult, TenantScope};

use crate::domain::turn::{Turn, TurnCursor, TurnListPage, TurnProjectionEvent, build_turns, turn_projection_event};
use crate::repositories::event::EventRepository;

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

        let projection_events: Vec<TurnProjectionEvent> = events.iter().map(turn_projection_event).collect();
        let built = build_turns(&projection_events, Utc::now().timestamp_millis());
        if built.unknown_event_type_count > 0 || built.deduplicated_event_count > 0 {
            tracing::debug!(
                unknown_event_type_count = built.unknown_event_type_count,
                deduplicated_event_count = built.deduplicated_event_count,
                "turn projection skipped unknown or duplicate events"
            );
        }
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
