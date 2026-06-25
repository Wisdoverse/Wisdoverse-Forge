//! Inbox notification service.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::inbox_notifications::InboxNotificationRow;
use sqlx::PgPool;

use crate::domain::inbox::{InboxListLimit, InboxNotificationProjection};
pub(crate) use crate::domain::inbox::{inbox_data_response, inbox_ok_response};
use crate::repositories::inbox::InboxRepository;

/// Persistence adapter: project a stored inbox row onto the domain wire shape.
/// Lives in the service layer (not domain) so the domain projection stays free
/// of any SQLx row coupling.
impl From<InboxNotificationRow> for InboxNotificationProjection {
    fn from(row: InboxNotificationRow) -> Self {
        Self {
            id: row.id,
            notification_type: row.notification_type,
            task_id: row.task_id.map(|id| id.to_string()).unwrap_or_default(),
            task_title: row.task_title,
            message: row.message,
            task_href: row.task_href,
            owner_user_id: row.user_id,
            read: row.read,
            timestamp: row.updated_at.timestamp_millis(),
        }
    }
}

pub(crate) struct InboxService {
    repo: InboxRepository,
}

impl InboxService {
    pub(crate) fn new(repo: InboxRepository) -> Self {
        Self { repo }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(InboxRepository::new(pool))
    }

    pub(crate) async fn list(
        &self,
        scope: &TenantScope,
        limit: Option<i64>,
    ) -> AppResult<Vec<InboxNotificationProjection>> {
        let limit = limit.map(InboxListLimit::new).unwrap_or_default().value();
        let rows = self.repo.list(scope, limit).await?;
        Ok(rows.into_iter().map(InboxNotificationProjection::from).collect())
    }

    pub(crate) async fn mark_read(&self, scope: &TenantScope, id: &str) -> AppResult<()> {
        self.repo.mark_read(scope, id).await
    }

    pub(crate) async fn mark_all_read(&self, scope: &TenantScope) -> AppResult<()> {
        self.repo.mark_all_read(scope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn inbox_notification_projection_serializes_legacy_wire_shape() {
        let row = InboxNotificationRow {
            id: "n-1".to_string(),
            user_id: Uuid::nil(),
            notification_type: "blocked".to_string(),
            task_id: None,
            task_title: "Task".to_string(),
            message: "msg".to_string(),
            task_href: None,
            read: false,
            updated_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        };
        let projection = InboxNotificationProjection::from(row);
        let json = serde_json::to_value(&projection).unwrap();

        assert_eq!(json["id"], "n-1");
        assert_eq!(json["type"], "blocked");
        assert_eq!(json["taskId"], "");
        assert_eq!(json["taskTitle"], "Task");
        assert_eq!(json["ownerUserId"], Uuid::nil().to_string());
        assert_eq!(json["read"], false);
        assert_eq!(json["timestamp"], 1_700_000_000_000i64);
        assert!(json.get("taskHref").is_none());
    }
}
