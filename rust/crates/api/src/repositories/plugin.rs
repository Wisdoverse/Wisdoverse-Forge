//! Plugin repository — database queries for the plugins table and the
//! per-agent `agent_plugins` join table.

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::Plugin;
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::configuration::ConfigurationRepositoryPolicy;

/// One row per (agent, plugin) combination — what's enabled for a specific
/// agent and with what override config. Joined with `plugins` so the listing
/// endpoint can return everything the UI needs in a single query.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AgentPluginRow {
    pub plugin_id: Uuid,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    /// Plugin-level enabled flag (org default).
    pub plugin_enabled: bool,
    /// Per-agent override; `None` means "no override, follow plugin default".
    pub enabled: Option<bool>,
    /// Per-agent config override; `None` means "use plugin's default config".
    pub config: Option<serde_json::Value>,
}

/// Database access layer for plugins.
pub struct PluginRepository {
    pool: PgPool,
}

impl PluginRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List plugins for the org (includes global plugins where organization_id IS NULL).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Plugin>> {
        let plugins = sqlx::query_as::<_, Plugin>(
            r#"SELECT * FROM plugins
               WHERE organization_id = $1 OR organization_id IS NULL
               ORDER BY name ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(plugins)
    }

    /// Get a plugin by ID (tenant-scoped or global).
    pub async fn get_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<Plugin> {
        sqlx::query_as::<_, Plugin>(
            r#"SELECT * FROM plugins
               WHERE id = $1 AND (organization_id = $2 OR organization_id IS NULL)"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ConfigurationRepositoryPolicy::plugin_not_found(id))
    }

    /// Create a new plugin for the org.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        version: &str,
        description: Option<&str>,
        config: &serde_json::Value,
    ) -> AppResult<Plugin> {
        sqlx::query_as::<_, Plugin>(
            r#"INSERT INTO plugins (organization_id, name, version, description, config)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(version)
        .bind(description)
        .bind(config)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update a plugin (tenant-scoped).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        config: Option<&serde_json::Value>,
        enabled: Option<bool>,
    ) -> AppResult<Plugin> {
        sqlx::query_as::<_, Plugin>(
            r#"UPDATE plugins
               SET config = COALESCE($3, config),
                   enabled = COALESCE($4, enabled),
                   updated_at = now()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(config)
        .bind(enabled)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ConfigurationRepositoryPolicy::plugin_not_found(id))
    }

    /// List plugins joined with the agent's per-agent overrides.
    /// Includes every org/global plugin so the UI can render the full list
    /// with toggle state — not-yet-overridden rows have `enabled = NULL`.
    pub async fn list_for_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Vec<AgentPluginRow>> {
        // Verify the agent belongs to the caller's org first; this prevents
        // a cross-tenant probe via a guessed UUID.
        let agent_org: Option<Uuid> =
            sqlx::query_scalar("SELECT organization_id FROM agents WHERE id = $1 AND organization_id = $2")
                .bind(agent_id.as_uuid())
                .bind(scope.org_id().as_uuid())
                .fetch_optional(&self.pool)
                .await?;
        if agent_org.is_none() {
            return Err(ConfigurationRepositoryPolicy::agent_not_found(agent_id));
        }

        let rows = sqlx::query_as::<_, AgentPluginRow>(
            r#"SELECT
                 p.id           AS plugin_id,
                 p.name         AS name,
                 p.version      AS version,
                 p.description  AS description,
                 p.enabled      AS plugin_enabled,
                 ap.enabled     AS enabled,
                 ap.config      AS config
               FROM plugins p
               LEFT JOIN agent_plugins ap
                      ON ap.plugin_id = p.id AND ap.agent_id = $1
               WHERE p.organization_id = $2 OR p.organization_id IS NULL
               ORDER BY p.name ASC"#,
        )
        .bind(agent_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Set or update the per-agent enabled flag and optional config override.
    /// Idempotent — uses ON CONFLICT to update existing rows.
    pub async fn set_for_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        plugin_id: Uuid,
        enabled: bool,
        config: Option<&serde_json::Value>,
    ) -> AppResult<()> {
        // Tenant guard: agent must belong to caller's org and plugin must be visible.
        let ok: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                 SELECT 1 FROM agents a
                  WHERE a.id = $1 AND a.organization_id = $2
               ) AND EXISTS (
                 SELECT 1 FROM plugins p
                  WHERE p.id = $3 AND (p.organization_id = $2 OR p.organization_id IS NULL)
               )"#,
        )
        .bind(agent_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(plugin_id)
        .fetch_one(&self.pool)
        .await?;
        if !ok {
            return Err(ConfigurationRepositoryPolicy::agent_or_plugin_not_found(agent_id, plugin_id));
        }

        sqlx::query(
            r#"INSERT INTO agent_plugins (agent_id, plugin_id, enabled, config)
                    VALUES ($1, $2, $3, $4)
               ON CONFLICT (agent_id, plugin_id) DO UPDATE
                  SET enabled = EXCLUDED.enabled,
                      config  = EXCLUDED.config,
                      updated_at = now()"#,
        )
        .bind(agent_id.as_uuid())
        .bind(plugin_id)
        .bind(enabled)
        .bind(config)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remove a per-agent override (revert to plugin default).
    pub async fn remove_for_agent(&self, scope: &TenantScope, agent_id: AgentId, plugin_id: Uuid) -> AppResult<()> {
        // Tenant guard via JOIN to agents in the same statement.
        let result = sqlx::query(
            r#"DELETE FROM agent_plugins ap
                USING agents a
                WHERE ap.agent_id = $1
                  AND ap.plugin_id = $2
                  AND a.id = ap.agent_id
                  AND a.organization_id = $3"#,
        )
        .bind(agent_id.as_uuid())
        .bind(plugin_id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ConfigurationRepositoryPolicy::agent_plugin_row_not_found(agent_id, plugin_id));
        }
        Ok(())
    }

    /// Delete a plugin by ID (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM plugins
               WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ConfigurationRepositoryPolicy::plugin_not_found(id));
        }
        Ok(())
    }
}
