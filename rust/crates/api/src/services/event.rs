//! Event service — business logic, validation, and pagination enforcement.

use chrono::{DateTime, Utc};

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::Event;
use serde_json::Value;

use crate::domain::observability::{EventListPage, EventReplayPage, EventType};
use crate::repositories::agent::event::EventRepository;

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
        let event_type = EventType::parse(event_type)?;
        self.repo.insert(scope, agent_id, event_type.value(), payload, session_id).await
    }

    /// List events for an agent with pagination. Limit is capped at 100.
    pub async fn list_by_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Event>> {
        let page = EventListPage::new(limit, offset);
        self.repo.list_by_agent(scope, agent_id, page.limit(), page.offset()).await
    }

    /// Replay events for an agent since a timestamp. Limit is capped at 2000 (MAX_EVENTS).
    pub async fn replay(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> AppResult<Vec<Event>> {
        let page = EventReplayPage::new(limit);
        self.repo.replay(scope, agent_id, since, page.limit()).await
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
        let page = EventReplayPage::new(limit);
        let mut events = self.repo.replay_cursor(scope, agent_id, after_ts, after_id, page.fetch_limit()).await?;
        let has_more = page.has_more(&events);
        if has_more {
            events.truncate(page.limit() as usize);
        }
        Ok((events, has_more))
    }

    /// List events for org with pagination. Limit is capped at 100.
    pub async fn list_by_org(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Event>> {
        let page = EventListPage::new(limit, offset);
        self.repo.list_by_org(scope, page.limit(), page.offset()).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::observability::{EventListPage, EventType};

    #[test]
    fn limit_clamping() {
        assert_eq!(EventListPage::new(0, 0).limit(), 1);
        assert_eq!(EventListPage::new(1, 0).limit(), 1);
        assert_eq!(EventListPage::new(50, 0).limit(), 50);
        assert_eq!(EventListPage::new(100, 0).limit(), 100);
        assert_eq!(EventListPage::new(200, 0).limit(), 100);
        assert_eq!(EventListPage::new(-5, 0).limit(), 1);
    }

    #[test]
    fn offset_floor() {
        assert_eq!(EventListPage::new(10, -10).offset(), 0);
        assert_eq!(EventListPage::new(10, 0).offset(), 0);
        assert_eq!(EventListPage::new(10, 50).offset(), 50);
    }

    #[test]
    fn empty_event_type_rejected() {
        assert!(EventType::parse("").is_err());
        assert!(EventType::parse("pre_tool_use").is_ok());
    }
}
