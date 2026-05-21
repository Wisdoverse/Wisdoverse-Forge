//! Inbox notification repository.

use agentforge_core::{AppResult, OrgId, UserId};
use agentforge_db::inbox_notifications::{
    InboxNotificationRow, list_user_inbox_notifications, mark_all_inbox_notifications_read,
    mark_inbox_notification_read,
};
use sqlx::PgPool;

pub(crate) struct InboxRepository {
    pool: PgPool,
}

impl InboxRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn list_for_user(
        &self,
        org_id: OrgId,
        user_id: UserId,
        limit: i64,
    ) -> AppResult<Vec<InboxNotificationRow>> {
        list_user_inbox_notifications(&self.pool, org_id.as_uuid(), user_id.as_uuid(), limit).await.map_err(Into::into)
    }

    pub(crate) async fn mark_read(&self, org_id: OrgId, user_id: UserId, id: &str) -> AppResult<()> {
        mark_inbox_notification_read(&self.pool, org_id.as_uuid(), user_id.as_uuid(), id).await.map_err(Into::into)
    }

    pub(crate) async fn mark_all_read(&self, org_id: OrgId, user_id: UserId) -> AppResult<()> {
        mark_all_inbox_notifications_read(&self.pool, org_id.as_uuid(), user_id.as_uuid()).await.map_err(Into::into)
    }
}
