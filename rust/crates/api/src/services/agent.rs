//! Agent service — business logic, validation, and state machine enforcement.

use agentforge_core::{AgentId, AgentStatus, AppError, AppResult, CliToolKind, ErrorKind, TenantScope};
use agentforge_db::entities::{Agent, AgentCollaborator};
use uuid::Uuid;

use crate::repositories::agent::{AgentListItem, AgentRepository, CreateAgentParams};

/// Business logic layer for agent operations.
pub struct AgentService {
    repo: AgentRepository,
}

impl AgentService {
    pub fn new(repo: AgentRepository) -> Self {
        Self { repo }
    }

    /// Build the NATS subject for agent sidecar commands.
    pub(crate) fn command_subject(agent_id: &str) -> String {
        format!("sidecar.{agent_id}.cmd")
    }

    /// List agents with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Agent>> {
        let limit = clamp_limit(limit);
        let offset = floor_offset(offset);
        self.repo.list(scope, limit, offset).await
    }

    /// List agents enriched with owner + project names, for frontend display.
    pub async fn list_with_owner(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<AgentListItem>> {
        let limit = clamp_limit(limit);
        let offset = floor_offset(offset);
        self.repo.list_with_owner(scope, limit, offset).await
    }

    /// Get a single agent by ID.
    pub async fn get(&self, scope: &TenantScope, id: AgentId) -> AppResult<Agent> {
        self.repo.find_by_id(scope, id).await
    }

    /// Get a single agent by ID enriched with owner + project names.
    pub async fn get_with_owner(&self, scope: &TenantScope, id: AgentId) -> AppResult<AgentListItem> {
        self.repo.find_with_owner_by_id(scope, id).await
    }

    /// Create a new agent with optional metadata. `cli_tool` distinguishes
    /// Container CLI agents (`claude`/`codex`/`gemini`/`opencode`) from pure
    /// provider+prompt agents (cli_tool = None).
    pub async fn create(&self, scope: &TenantScope, params: CreateAgentParams<'_>) -> AppResult<Agent> {
        if let Err(msg) = validate_agent_name(params.name) {
            return Err(ErrorKind::Validation(msg.into()).into());
        }
        let mut params = params;
        if let Some(tool) = params.cli_tool {
            let tool = CliToolKind::parse_legacy(tool)
                .map_err(|err| AppError::from(ErrorKind::Validation(err.to_string())))?;
            params.cli_tool = Some(tool.as_str());
        }
        self.repo.create(scope, params).await
    }

    /// Update agent fields (name, model, provider, system_prompt).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: AgentId,
        name: Option<&str>,
        model: Option<&str>,
        provider: Option<&str>,
        system_prompt: Option<&str>,
    ) -> AppResult<Agent> {
        if let Err(msg) = validate_agent_name(name) {
            return Err(ErrorKind::Validation(msg.into()).into());
        }
        self.repo.update(scope, id, name, model, provider, system_prompt).await
    }

    /// Update agent status with state machine validation.
    ///
    /// Valid transitions follow the CLAUDE.md agent status diagram:
    /// - Idle -> Working, Offline
    /// - Working -> Idle, Offline
    /// - Offline -> Idle, Working
    pub async fn update_status(&self, scope: &TenantScope, id: AgentId, new_status: AgentStatus) -> AppResult<Agent> {
        let agent = self.repo.find_by_id(scope, id).await?;

        // Same status is a no-op
        if agent.status == new_status {
            return Ok(agent);
        }

        // Validate state transition
        if !Self::is_valid_transition(agent.status, new_status) {
            return Err(ErrorKind::Validation(format!(
                "invalid status transition: {:?} -> {:?}",
                agent.status, new_status
            ))
            .into());
        }

        self.repo.update_status(scope, id, new_status).await
    }

    /// Delete an agent.
    pub async fn delete(&self, scope: &TenantScope, id: AgentId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Check whether a status transition is valid.
    fn is_valid_transition(from: AgentStatus, to: AgentStatus) -> bool {
        is_valid_transition(from, to)
    }

    // --- Collaborator operations ---

    /// List collaborators for an agent.
    pub async fn list_collaborators(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Vec<AgentCollaborator>> {
        self.repo.list_collaborators(scope, agent_id).await
    }

    /// Add a collaborator with validated permission.
    pub async fn add_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        validate_permission(permission)?;
        self.repo.add_collaborator(scope, agent_id, user_id, permission).await
    }

    /// Update a collaborator's permission.
    pub async fn update_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        validate_permission(permission)?;
        self.repo.update_collaborator(scope, agent_id, user_id, permission).await
    }

    /// Remove a collaborator from an agent.
    pub async fn remove_collaborator(&self, scope: &TenantScope, agent_id: AgentId, user_id: Uuid) -> AppResult<()> {
        self.repo.remove_collaborator(scope, agent_id, user_id).await
    }
}

/// Check whether a status transition is valid per the agent state machine.
///
/// Valid transitions:
/// - Idle -> Working, Offline
/// - Working -> Idle, Offline
/// - Offline -> Idle, Working
pub(crate) fn is_valid_transition(from: AgentStatus, to: AgentStatus) -> bool {
    matches!(
        (from, to),
        (AgentStatus::Idle, AgentStatus::Working)
            | (AgentStatus::Idle, AgentStatus::Offline)
            | (AgentStatus::Working, AgentStatus::Idle)
            | (AgentStatus::Working, AgentStatus::Offline)
            | (AgentStatus::Offline, AgentStatus::Idle)
            | (AgentStatus::Offline, AgentStatus::Working)
    )
}

/// Validate collaborator permission: must be "view", "edit", or "admin".
pub(crate) fn validate_permission(permission: &str) -> AppResult<()> {
    match permission {
        "view" | "edit" | "admin" => Ok(()),
        _ => Err(ErrorKind::Validation("permission must be 'view', 'edit', or 'admin'".into()).into()),
    }
}

/// Validate an optional agent name. Must be 255 characters or less.
pub(crate) fn validate_agent_name(name: Option<&str>) -> Result<(), &'static str> {
    if let Some(name) = name
        && name.len() > 255
    {
        return Err("name must be 255 characters or less");
    }
    Ok(())
}

/// Clamp a limit value to the valid range [1, 100].
pub(crate) fn clamp_limit(limit: i64) -> i64 {
    limit.clamp(1, 100)
}

/// Floor an offset value to 0.
pub(crate) fn floor_offset(offset: i64) -> i64 {
    offset.max(0)
}
