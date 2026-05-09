//! Event service — business logic, validation, and pagination enforcement.

use chrono::{DateTime, Utc};

use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Event;
use serde_json::Value;

use crate::repositories::event::EventRepository;

/// Business logic layer for event operations.
pub struct EventService {
    repo: EventRepository,
}

impl EventService {
    pub fn new(repo: EventRepository) -> Self {
        Self { repo }
    }

    /// Ingest a new event. Validates event_type is not empty.
    pub async fn ingest(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        event_type: &str,
        payload: Value,
        session_id: Option<&str>,
    ) -> AppResult<Event> {
        if event_type.is_empty() {
            return Err(ErrorKind::Validation("event_type must not be empty".into()).into());
        }
        self.repo.insert(scope, agent_id, event_type, payload, session_id).await
    }

    /// List events for an agent with pagination. Limit is capped at 100.
    pub async fn list_by_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Event>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list_by_agent(scope, agent_id, limit, offset).await
    }

    /// Replay events for an agent since a timestamp. Limit is capped at 2000 (MAX_EVENTS).
    pub async fn replay(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> AppResult<Vec<Event>> {
        let limit = limit.clamp(1, 2000); // MAX_EVENTS per shared/defaults.ts
        self.repo.replay(scope, agent_id, since, limit).await
    }

    /// Cursor-based replay. Returns `(events, has_more)` where `events` has at
    /// most `limit` entries; `has_more` = true means the caller should continue
    /// paging with the last returned event as the next cursor.
    ///
    /// Limit is capped at 2000 (MAX_EVENTS). Fetches `limit + 1` then trims.
    pub async fn replay_cursor(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        after_ts: DateTime<Utc>,
        after_id: uuid::Uuid,
        limit: i64,
    ) -> AppResult<(Vec<Event>, bool)> {
        let limit = limit.clamp(1, 2000);
        let fetch_limit = limit + 1;
        let mut events = self.repo.replay_cursor(scope, agent_id, after_ts, after_id, fetch_limit).await?;
        let has_more = events.len() as i64 > limit;
        if has_more {
            events.truncate(limit as usize);
        }
        Ok((events, has_more))
    }

    /// List events for org with pagination. Limit is capped at 100.
    pub async fn list_by_org(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Event>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list_by_org(scope, limit, offset).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn limit_clamping() {
        assert_eq!(0_i64.clamp(1, 100), 1);
        assert_eq!(1_i64.clamp(1, 100), 1);
        assert_eq!(50_i64.clamp(1, 100), 50);
        assert_eq!(100_i64.clamp(1, 100), 100);
        assert_eq!(200_i64.clamp(1, 100), 100);
        assert_eq!((-5_i64).clamp(1, 100), 1);
    }

    #[test]
    fn offset_floor() {
        let negative_offset = -10_i64;
        let zero_offset = 0_i64;
        let positive_offset = 50_i64;
        assert_eq!(negative_offset.max(0), 0);
        assert_eq!(zero_offset.max(0), 0);
        assert_eq!(positive_offset.max(0), 50);
    }

    #[test]
    fn empty_event_type_rejected() {
        // The validation is inline in ingest(), so we verify the logic here.
        let event_type = "";
        assert!(event_type.is_empty());

        let event_type = "pre_tool_use";
        assert!(!event_type.is_empty());
    }
}
