use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Executor, FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::entities::OrchestrationTask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOwnerNotificationKind {
    Blocked,
    Completed,
    Failed,
}

impl TaskOwnerNotificationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct InboxNotificationRow {
    pub id: String,
    pub user_id: Uuid,
    pub notification_type: String,
    pub task_id: Option<Uuid>,
    pub task_title: String,
    pub message: String,
    pub task_href: Option<String>,
    pub read: bool,
    pub updated_at: DateTime<Utc>,
}

struct UpsertTaskOwnerNotification<'a> {
    organization_id: Uuid,
    user_id: Uuid,
    id: String,
    notification_type: &'static str,
    task_id: Uuid,
    task_title: String,
    message: String,
    task_href: Option<&'a str>,
}

pub async fn list_user_inbox_notifications(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<InboxNotificationRow>, sqlx::Error> {
    sqlx::query_as::<_, InboxNotificationRow>(
        r#"SELECT id,
                  user_id,
                  notification_type,
                  task_id,
                  task_title,
                  message,
                  task_href,
                  read,
                  updated_at
           FROM inbox_notifications
           WHERE organization_id = $1 AND user_id = $2
           ORDER BY updated_at DESC
           LIMIT $3"#,
    )
    .bind(organization_id)
    .bind(user_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await
}

pub async fn mark_inbox_notification_read(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE inbox_notifications
           SET read = TRUE
           WHERE organization_id = $1 AND user_id = $2 AND id = $3"#,
    )
    .bind(organization_id)
    .bind(user_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_all_inbox_notifications_read(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE inbox_notifications
           SET read = TRUE
           WHERE organization_id = $1 AND user_id = $2 AND read = FALSE"#,
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_task_owner_lifecycle_notification(
    pool: &PgPool,
    task: &OrchestrationTask,
    assigned_agent_name: Option<&str>,
    kind: TaskOwnerNotificationKind,
) -> Result<(), sqlx::Error> {
    let record = task_owner_lifecycle_notification_record(task, assigned_agent_name, kind);
    upsert_task_owner_notification(pool, record).await
}

pub async fn upsert_task_owner_lifecycle_notification_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    task: &OrchestrationTask,
    assigned_agent_name: Option<&str>,
    kind: TaskOwnerNotificationKind,
) -> Result<(), sqlx::Error> {
    let record = task_owner_lifecycle_notification_record(task, assigned_agent_name, kind);
    upsert_task_owner_notification(&mut **tx, record).await
}

fn task_owner_lifecycle_notification_record<'a>(
    task: &OrchestrationTask,
    assigned_agent_name: Option<&str>,
    kind: TaskOwnerNotificationKind,
) -> UpsertTaskOwnerNotification<'a> {
    let task_title = task_title(task);
    let detail = lifecycle_detail(task, kind);
    let message = task_owner_notification_message(kind, assigned_agent_name, &detail);
    UpsertTaskOwnerNotification {
        organization_id: task.organization_id.as_uuid(),
        user_id: task.created_by.as_uuid(),
        id: task_owner_notification_id(task.id, kind),
        notification_type: kind.as_str(),
        task_id: task.id,
        task_title,
        message,
        task_href: Some("/tasks"),
    }
}

pub fn task_owner_notification_id(task_id: Uuid, kind: TaskOwnerNotificationKind) -> String {
    format!("task-owner:{task_id}:{}", kind.as_str())
}

fn task_owner_notification_message(
    kind: TaskOwnerNotificationKind,
    assigned_agent_name: Option<&str>,
    detail: &str,
) -> String {
    let actor = assigned_agent_name.filter(|name| !name.trim().is_empty()).unwrap_or("Assigned agent");
    match kind {
        TaskOwnerNotificationKind::Blocked => format!("{actor} is blocked and needs owner input: {detail}"),
        TaskOwnerNotificationKind::Completed => format!("{actor} completed this task: {detail}"),
        TaskOwnerNotificationKind::Failed => format!("{actor} failed to complete this task: {detail}"),
    }
}

async fn upsert_task_owner_notification(
    executor: impl Executor<'_, Database = Postgres>,
    record: UpsertTaskOwnerNotification<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO inbox_notifications
              (id, organization_id, user_id, notification_type, task_id, task_title, message, task_href, read)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE)
           ON CONFLICT (id) DO UPDATE
           SET organization_id = EXCLUDED.organization_id,
               user_id = EXCLUDED.user_id,
               notification_type = EXCLUDED.notification_type,
               task_id = EXCLUDED.task_id,
               task_title = EXCLUDED.task_title,
               message = EXCLUDED.message,
               task_href = EXCLUDED.task_href,
               read = FALSE,
               updated_at = NOW()"#,
    )
    .bind(record.id)
    .bind(record.organization_id)
    .bind(record.user_id)
    .bind(record.notification_type)
    .bind(record.task_id)
    .bind(record.task_title)
    .bind(record.message)
    .bind(record.task_href)
    .execute(executor)
    .await?;
    Ok(())
}

fn task_title(task: &OrchestrationTask) -> String {
    task.params
        .as_ref()
        .and_then(|params| params.get("task"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&task.title)
        .to_string()
}

fn lifecycle_detail(task: &OrchestrationTask, kind: TaskOwnerNotificationKind) -> String {
    match kind {
        TaskOwnerNotificationKind::Blocked => task
            .blocked_reason
            .clone()
            .or_else(|| task.error.as_ref().map(error_message))
            .unwrap_or_else(|| "No unblock reason was provided".to_string()),
        TaskOwnerNotificationKind::Completed => {
            task.result.as_ref().map(result_message).unwrap_or_else(|| "No result summary was provided".to_string())
        }
        TaskOwnerNotificationKind::Failed => {
            task.error.as_ref().map(error_message).unwrap_or_else(|| "No failure reason was provided".to_string())
        }
    }
}

fn error_message(error: &Value) -> String {
    error.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| error.to_string())
}

fn result_message(result: &Value) -> String {
    result
        .get("message")
        .or_else(|| result.get("stdout"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| result.to_string())
}
