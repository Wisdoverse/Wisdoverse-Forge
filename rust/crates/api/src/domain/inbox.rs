//! User inbox notification domain projections and policies.

use agentforge_db::inbox_notifications::InboxNotificationRow;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

const DEFAULT_INBOX_LIMIT: i64 = 50;
const MAX_INBOX_LIMIT: i64 = 200;

pub(crate) fn inbox_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn inbox_ok_response() -> Value {
    json!({ "ok": true })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InboxListLimit {
    value: i64,
}

impl InboxListLimit {
    pub(crate) fn new(limit: i64) -> Self {
        Self { value: limit.clamp(1, MAX_INBOX_LIMIT) }
    }

    pub(crate) fn value(self) -> i64 {
        self.value
    }
}

impl Default for InboxListLimit {
    fn default() -> Self {
        Self { value: DEFAULT_INBOX_LIMIT }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InboxNotificationProjection {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) notification_type: String,
    pub(crate) task_id: String,
    pub(crate) task_title: String,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) task_href: Option<String>,
    pub(crate) owner_user_id: Uuid,
    pub(crate) read: bool,
    pub(crate) timestamp: i64,
}

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

pub(crate) fn inbox_notification_projection(row: InboxNotificationRow) -> InboxNotificationProjection {
    row.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_list_limit_defaults_and_clamps() {
        assert_eq!(InboxListLimit::default().value(), 50);
        assert_eq!(InboxListLimit::new(0).value(), 1);
        assert_eq!(InboxListLimit::new(500).value(), 200);
        assert_eq!(InboxListLimit::new(25).value(), 25);
    }
}
