//! Task template domain rules — validation and wire views.
//!
//! Templates are org-scoped starter briefs that any member can apply when
//! writing a task and manage from Settings. Only the template's creator or an
//! owner/admin may delete one.

use agentforge_core::{AppResult, ErrorKind, ProjectId, ScopedRead, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::orchestration::TaskPriority;

/// Content bounds and authorization for task templates.
pub(crate) struct TaskTemplatePolicy;

impl TaskTemplatePolicy {
    pub(crate) const MAX_NAME: usize = 80;
    pub(crate) const MAX_TITLE: usize = 160;
    pub(crate) const MAX_DESCRIPTION: usize = 2_000;

    pub(crate) fn validate(name: &str, title: &str, description: &str, priority: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ErrorKind::Validation("Give the template a short name.".to_string()).into());
        }
        if name.chars().count() > Self::MAX_NAME {
            return Err(ErrorKind::Validation(format!(
                "Keep the template name to {MAX} characters or fewer.",
                MAX = Self::MAX_NAME
            ))
            .into());
        }

        let title = title.trim();
        if title.is_empty() {
            return Err(ErrorKind::Validation("Add the task title this template writes.".to_string()).into());
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
        Ok(())
    }

    pub(crate) fn require_readable_project(proof: &ScopedRead, project_id: Uuid) -> AppResult<()> {
        if proof.contains_project(ProjectId::from(project_id)) {
            Ok(())
        } else {
            Err(ErrorKind::Forbidden("You do not have access to that project.".to_string()).into())
        }
    }
}

/// Persistence-free mirror of the `task_templates` row for API responses.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTemplateView {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) priority: String,
    pub(crate) requires_approval: bool,
    pub(crate) project_id: Option<Uuid>,
    pub(crate) created_by: UserId,
    pub(crate) created_at: DateTime<Utc>,
}

impl TaskTemplateView {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_row(
        id: Uuid,
        name: String,
        title: String,
        description: String,
        priority: String,
        requires_approval: bool,
        project_id: Option<Uuid>,
        created_by: UserId,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self { id, name, title, description, priority, requires_approval, project_id, created_by, created_at }
    }
}

/// Create-task-template request body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskTemplateInput {
    pub(crate) name: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default = "default_template_priority")]
    pub(crate) priority: String,
    #[serde(default)]
    pub(crate) requires_approval: bool,
    #[serde(default)]
    pub(crate) project_id: Option<Uuid>,
}

fn default_template_priority() -> String {
    "normal".to_string()
}

pub(crate) fn template_not_found(id: Uuid) -> ErrorKind {
    ErrorKind::NotFound(format!("Task template {id} was not found in this team space."))
}

pub(crate) fn template_project_invalid() -> ErrorKind {
    ErrorKind::Validation("Choose a live project in this organization.".into())
}

/// Audit payload for template create/delete (names the template).
pub(crate) fn audit_named_payload(name: &str) -> serde_json::Value {
    serde_json::json!({ "name": name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_requires_name_and_title() {
        assert!(TaskTemplatePolicy::validate("", "Title", "", "normal").is_err());
        assert!(TaskTemplatePolicy::validate("Name", "  ", "", "normal").is_err());
    }

    #[test]
    fn template_rejects_bad_priority() {
        assert!(TaskTemplatePolicy::validate("Name", "Title", "", "urgent-ish").is_err());
        assert!(TaskTemplatePolicy::validate("Name", "Title", "", "normal").is_ok());
    }

    #[test]
    fn project_template_requires_a_validated_read_membership() {
        let org = agentforge_core::OrgId::from(Uuid::new_v4());
        let user = UserId::from(Uuid::new_v4());
        let allowed = ProjectId::from(Uuid::new_v4());
        let denied = Uuid::new_v4();
        let proof = ScopedRead::from_validated_memberships(org, user, [], [], [allowed]);
        assert!(TaskTemplatePolicy::require_readable_project(&proof, allowed.as_uuid()).is_ok());
        assert!(TaskTemplatePolicy::require_readable_project(&proof, denied).is_err());
    }
}
