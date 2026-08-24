//! Recurring task domain rules — scheduling bounds and wire views.

use agentforge_core::{AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::orchestration::TaskPriority;

/// Bounds and validation for scheduled tasks.
pub(crate) struct RecurringTaskPolicy;

impl RecurringTaskPolicy {
    pub(crate) const MIN_CADENCE: i32 = 15;
    pub(crate) const MAX_CADENCE: i32 = 43_200;
    pub(crate) const MAX_NAME: usize = 80;
    pub(crate) const MAX_TITLE: usize = 160;
    pub(crate) const MAX_DESCRIPTION: usize = 2_000;

    pub(crate) fn validate(name: &str, title: &str, description: &str, priority: &str, cadence: i32) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ErrorKind::Validation("Give the recurring task a short name.".to_string()).into());
        }
        if name.chars().count() > Self::MAX_NAME {
            return Err(ErrorKind::Validation(format!(
                "Keep the name to {MAX} characters or fewer.",
                MAX = Self::MAX_NAME
            ))
            .into());
        }
        let title = title.trim();
        if title.is_empty() {
            return Err(ErrorKind::Validation("Add the task title each run should use.".to_string()).into());
        }
        if title.chars().count() > Self::MAX_TITLE {
            return Err(ErrorKind::Validation(format!(
                "Keep the task title to {MAX} characters or fewer.",
                MAX = Self::MAX_TITLE
            ))
            .into());
        }
        if description.chars().count() > Self::MAX_DESCRIPTION {
            return Err(ErrorKind::Validation(format!(
                "Keep the task brief to {MAX} characters or fewer.",
                MAX = Self::MAX_DESCRIPTION
            ))
            .into());
        }
        TaskPriority::validate(priority)?;
        if !(Self::MIN_CADENCE..=Self::MAX_CADENCE).contains(&cadence) {
            return Err(ErrorKind::Validation(format!(
                "cadenceMinutes must be between {MIN} and {MAX}",
                MIN = Self::MIN_CADENCE,
                MAX = Self::MAX_CADENCE
            ))
            .into());
        }
        Ok(())
    }
}

/// Persistence-free mirror of a recurring_tasks row for API responses.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringTaskView {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) priority: String,
    pub(crate) requires_approval: bool,
    pub(crate) project_id: Uuid,
    pub(crate) group_id: Uuid,
    pub(crate) cadence_minutes: i32,
    pub(crate) next_run_at: DateTime<Utc>,
    pub(crate) enabled: bool,
    pub(crate) created_at: DateTime<Utc>,
}

/// Audit payload for schedule creation.
pub(crate) fn audit_created_payload(name: &str, cadence_minutes: i32) -> serde_json::Value {
    serde_json::json!({ "name": name, "cadence_minutes": cadence_minutes })
}

/// Audit payload for schedule deletion.
pub(crate) fn audit_deleted_payload(id: Uuid) -> serde_json::Value {
    serde_json::json!({ "id": id })
}

/// Audit payload for enable/disable.
pub(crate) fn audit_enabled_payload(enabled: bool) -> serde_json::Value {
    serde_json::json!({ "enabled": enabled })
}

/// Create-recurring-task request body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecurringTaskInput {
    pub(crate) name: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default = "default_recurring_priority")]
    pub(crate) priority: String,
    #[serde(default)]
    pub(crate) requires_approval: bool,
    pub(crate) project_id: Uuid,
    pub(crate) group_id: Uuid,
    pub(crate) cadence_minutes: i32,
}

fn default_recurring_priority() -> String {
    "normal".to_string()
}

/// PATCH body for enable/disable.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRecurringTaskInput {
    pub(crate) enabled: bool,
}

pub(crate) fn recurring_task_not_found(id: Uuid) -> ErrorKind {
    ErrorKind::NotFound(format!("Recurring task {id} was not found in this team space."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurring_policy_validates_bounds() {
        assert!(RecurringTaskPolicy::validate("Daily", "Daily summary", "", "normal", 1_440).is_ok());
        assert!(RecurringTaskPolicy::validate("Daily", "Daily summary", "", "normal", 5).is_err());
        assert!(RecurringTaskPolicy::validate("Daily", "Daily summary", "", "normal", 100_000).is_err());
        assert!(RecurringTaskPolicy::validate("", "Daily summary", "", "normal", 1_440).is_err());
        assert!(RecurringTaskPolicy::validate("Daily", "", "", "normal", 1_440).is_err());
    }
}
