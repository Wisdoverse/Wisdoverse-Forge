//! Event repository — tenant-scoped database queries for the events table.

use chrono::{DateTime, Utc};

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::Event;
use serde_json::Value;
use sqlx::PgPool;

/// Database access layer for events. All queries enforce tenant isolation
/// via `WHERE organization_id = $N`.
pub struct EventRepository {
    pool: PgPool,
}

impl EventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new event.
    pub async fn insert(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        event_type: &str,
        payload: Value,
        session_id: Option<&str>,
    ) -> AppResult<Event> {
        let event = sqlx::query_as::<_, Event>(
            r#"INSERT INTO events (organization_id, agent_id, event_type, payload, session_id)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(event_type)
        .bind(payload)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(event)
    }

    /// List events for an agent (paginated, newest first).
    pub async fn list_by_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"SELECT * FROM events
               WHERE organization_id = $1 AND agent_id = $2
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    /// List all events for an agent in projection order.
    pub async fn list_by_agent_chronological(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"SELECT * FROM events
               WHERE organization_id = $1 AND agent_id = $2
               ORDER BY created_at ASC, id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    /// List events for org (paginated, newest first).
    pub async fn list_by_org(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"SELECT * FROM events
               WHERE organization_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    /// Replay events for an agent since a given timestamp (chronological order, ASC).
    ///
    /// Unlike `list_by_agent` (DESC), this returns events in time order for catch-up replay.
    pub async fn replay(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> AppResult<Vec<Event>> {
        let events = match since {
            Some(since) => {
                sqlx::query_as::<_, Event>(
                    r#"SELECT * FROM events
                       WHERE organization_id = $1 AND agent_id = $2 AND created_at >= $3
                       ORDER BY created_at ASC
                       LIMIT $4"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(agent_id.as_uuid())
                .bind(since)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Event>(
                    r#"SELECT * FROM events
                       WHERE organization_id = $1 AND agent_id = $2
                       ORDER BY created_at ASC
                       LIMIT $3"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(agent_id.as_uuid())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(events)
    }

    /// Cursor-based replay for an agent. Returns events strictly after the
    /// `(after_ts, after_id)` tuple in chronological order.
    ///
    /// `fetch_limit` should be `limit + 1` so the caller can derive `hasMore`
    /// by checking whether the returned slice exceeds `limit`. A zero UUID as
    /// `after_id` effectively means "any event at or after `after_ts`".
    ///
    /// Precision note: `event_to_claude_event_json` emits `timestamp_millis()`
    /// to the client. Browsers store that watermark at millisecond precision
    /// via `Date`, but Postgres `TIMESTAMPTZ` holds microseconds. A naive
    /// `created_at > $after_ts` would re-include the last-seen row whenever
    /// its true microsecond stamp exceeds the rounded millisecond sent back —
    /// that inflates `hasMore` and replays events the client already has.
    /// `date_trunc('milliseconds', created_at)` aligns the comparison with
    /// the precision the client actually stores. Pinned by
    /// `replay_cursor_compares_at_millisecond_precision` in `event_tests.rs`.
    pub async fn replay_cursor(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        after_ts: DateTime<Utc>,
        after_id: uuid::Uuid,
        fetch_limit: i64,
    ) -> AppResult<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"SELECT * FROM events
               WHERE organization_id = $1 AND agent_id = $2
                 AND (date_trunc('milliseconds', created_at), id) > ($3, $4)
               ORDER BY date_trunc('milliseconds', created_at) ASC, id ASC
               LIMIT $5"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(after_ts)
        .bind(after_id)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    /// Count events for agent.
    pub async fn count_by_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<i64> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE organization_id = $1 AND agent_id = $2")
                .bind(scope.org_id().as_uuid())
                .bind(agent_id.as_uuid())
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }
}
