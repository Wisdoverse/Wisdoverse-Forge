//! Inbox notification service.

use agentforge_core::{AppResult, TenantScope};

use crate::domain::inbox::{InboxListLimit, InboxNotificationProjection, inbox_notification_projection};
pub(crate) use crate::domain::inbox::{inbox_data_response, inbox_ok_response};
use crate::repositories::inbox::InboxRepository;

pub(crate) struct InboxService {
    repo: InboxRepository,
}

impl InboxService {
    pub(crate) fn new(repo: InboxRepository) -> Self {
        Self { repo }
    }

    pub(crate) async fn list(
        &self,
        scope: &TenantScope,
        limit: Option<i64>,
    ) -> AppResult<Vec<InboxNotificationProjection>> {
        let limit = limit.map(InboxListLimit::new).unwrap_or_default().value();
        let rows = self.repo.list_for_user(scope.org_id(), scope.user_id(), limit).await?;
        Ok(rows.into_iter().map(inbox_notification_projection).collect())
    }

    pub(crate) async fn mark_read(&self, scope: &TenantScope, id: &str) -> AppResult<()> {
        self.repo.mark_read(scope.org_id(), scope.user_id(), id).await
    }

    pub(crate) async fn mark_all_read(&self, scope: &TenantScope) -> AppResult<()> {
        self.repo.mark_all_read(scope.org_id(), scope.user_id()).await
    }
}
