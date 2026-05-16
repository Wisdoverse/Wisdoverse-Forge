//! Plugin service — validation and management.

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::Plugin;
use uuid::Uuid;

use crate::domain::configuration::{PluginName, PluginVersion};
use crate::repositories::plugin::{AgentPluginRow, PluginRepository};

/// Business logic layer for plugin operations.
pub struct PluginService {
    repo: PluginRepository,
}

impl PluginService {
    pub fn new(repo: PluginRepository) -> Self {
        Self { repo }
    }

    /// List all plugins (org-scoped + global).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Plugin>> {
        self.repo.list(scope).await
    }

    /// Get a plugin by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Plugin> {
        self.repo.get_by_id(scope, id).await
    }

    /// Create a new plugin.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        version: Option<&str>,
        description: Option<&str>,
        config: Option<&serde_json::Value>,
    ) -> AppResult<Plugin> {
        let name = PluginName::parse(name)?;
        let version = PluginVersion::from_optional(version);
        let default_config = serde_json::json!({});
        let config = config.unwrap_or(&default_config);
        self.repo.create(scope, name.value(), version.value(), description, config).await
    }

    /// Update a plugin's config/enabled state.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        config: Option<&serde_json::Value>,
        enabled: Option<bool>,
    ) -> AppResult<Plugin> {
        self.repo.update(scope, id, config, enabled).await
    }

    /// Delete (uninstall) a plugin.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// List plugins joined with the agent's per-agent overrides.
    pub async fn list_for_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Vec<AgentPluginRow>> {
        self.repo.list_for_agent(scope, agent_id).await
    }

    /// Set or update the per-agent enabled flag and optional config.
    pub async fn set_for_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        plugin_id: Uuid,
        enabled: bool,
        config: Option<&serde_json::Value>,
    ) -> AppResult<()> {
        self.repo.set_for_agent(scope, agent_id, plugin_id, enabled, config).await
    }

    /// Remove the per-agent override (revert to plugin default).
    pub async fn remove_for_agent(&self, scope: &TenantScope, agent_id: AgentId, plugin_id: Uuid) -> AppResult<()> {
        self.repo.remove_for_agent(scope, agent_id, plugin_id).await
    }
}
