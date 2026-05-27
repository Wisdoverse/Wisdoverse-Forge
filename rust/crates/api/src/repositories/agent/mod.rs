//! Agent aggregate — tenant-scoped queries for the agents table plus the
//! agent's chat messages and event stream.

pub mod event;
pub mod mcp;
pub mod message;
pub mod workspace;

pub use event::EventRepository;
pub(crate) use mcp::{McpAgentInsertRecord, McpAgentRepository};
pub use message::MessageRepository;
pub use workspace::AgentWorkspaceRepository;

use agentforge_core::{AgentId, AgentStatus, AppResult, RuntimeKind, TenantScope};
use agentforge_db::entities::{Agent, AgentCollaborator};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::FromRow;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{AgentRepositoryPolicy, NewAgent};

/// Enriched agent row with owner and project info joined in.
///
/// Used by user-facing list/get endpoints so the frontend can display
/// `ownerUsername` / `ownerEmail` / `projectName` without extra round trips.
/// Timestamps serialize as epoch milliseconds to match the frontend
/// `ManagedAgent` TypeScript contract.
///
#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentListItem {
    pub id: Uuid,
    pub name: Option<String>,
    pub status: AgentStatus,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub cli_tool: Option<String>,
    pub runtime_kind: RuntimeKind,
    pub system_prompt: Option<String>,
    pub container_id: Option<String>,
    pub cli_session_id: Option<String>,
    pub cwd: Option<String>,
    pub current_tool: Option<String>,
    pub tokens_current: i64,
    pub tokens_cumulative: i64,
    pub git_status: Option<String>,
    pub runtime_id: Option<String>,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub user_id: Uuid,
    pub owner_username: Option<String>,
    pub owner_email: Option<String>,
    pub workspace_name: Option<String>,
    pub project_name: Option<String>,
    #[serde(serialize_with = "serialize_optional_ts_millis")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_optional_ts_millis")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_optional_ts_millis")]
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serialize_ts_millis")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_ts_millis")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CreateAgentParams<'a> {
    pub name: Option<&'a str>,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub cli_tool: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub workspace_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub system_prompt: Option<&'a str>,
}

/// Serialize `DateTime<Utc>` as epoch milliseconds (matches frontend numeric contract).
fn serialize_ts_millis<S>(ts: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_i64(ts.timestamp_millis())
}

/// Serialize `Option<DateTime<Utc>>` as epoch milliseconds or null.
fn serialize_optional_ts_millis<S>(ts: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match ts {
        Some(t) => serializer.serialize_i64(t.timestamp_millis()),
        None => serializer.serialize_none(),
    }
}

/// SELECT fragment used by every enriched agent query. Joins users + projects
/// so the frontend can display owner and project names without extra queries.
const AGENT_ENRICHED_SELECT: &str = r#"SELECT
    a.id,
    a.name,
    a.status,
    a.model,
    a.provider,
    a.cli_tool,
    a.runtime_kind,
    a.system_prompt,
    a.container_id,
    a.cli_session_id,
    a.cwd,
    a.current_tool,
    a.tokens_current,
    a.tokens_cumulative,
    a.git_status,
    a.runtime_id,
    a.organization_id,
    a.workspace_id,
    a.project_id,
    a.user_id,
    u.display_name AS owner_username,
    u.email        AS owner_email,
    w.name         AS workspace_name,
    p.name         AS project_name,
    a.started_at,
    a.ended_at,
    a.last_activity_at,
    a.created_at,
    a.updated_at
FROM agents a
LEFT JOIN users u    ON a.user_id    = u.id
LEFT JOIN workspaces w ON a.workspace_id = w.id
LEFT JOIN projects p ON a.project_id = p.id"#;

/// Database access layer for agents. All queries enforce tenant isolation
/// via `WHERE organization_id = $N`.
pub struct AgentRepository {
    pool: PgPool,
}

impl AgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List agents for the current tenant, ordered by most recent first.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Agent>> {
        let agents = sqlx::query_as::<_, Agent>(
            r#"SELECT * FROM agents
               WHERE organization_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(agents)
    }

    /// List agents with owner + project names joined in. Used by user-facing
    /// `/api/v1/agents` so the UI can render owner info.
    pub async fn list_with_owner(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<AgentListItem>> {
        let query = format!(
            "{AGENT_ENRICHED_SELECT}\n\
             WHERE a.organization_id = $1\n\
             ORDER BY a.created_at DESC\n\
             LIMIT $2 OFFSET $3"
        );
        let rows = sqlx::query_as::<_, AgentListItem>(&query)
            .bind(scope.org_id().as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Get a single agent by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: AgentId) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = $1 AND organization_id = $2")
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Get a single agent by ID with owner + project joined in (tenant-scoped).
    pub async fn find_with_owner_by_id(&self, scope: &TenantScope, id: AgentId) -> AppResult<AgentListItem> {
        let query = format!(
            "{AGENT_ENRICHED_SELECT}\n\
             WHERE a.id = $1 AND a.organization_id = $2"
        );
        sqlx::query_as::<_, AgentListItem>(&query)
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Create a new agent with `idle` status.
    pub async fn create(&self, scope: &TenantScope, params: CreateAgentParams<'_>) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"INSERT INTO agents (organization_id, workspace_id, user_id, name, model, provider, cli_tool, cwd, project_id, status, system_prompt)
               VALUES (
                   $1,
                   COALESCE(
                       $2::uuid,
                       (SELECT id
                          FROM workspaces
                         WHERE organization_id = $1
                           AND deleted_at IS NULL
                         ORDER BY created_at ASC
                         LIMIT 1)
                   ),
                   $3, $4, $5, $6, $7, $8, $9, 'idle', $10
               )
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(params.workspace_id)
        .bind(scope.user_id().as_uuid())
        .bind(params.name)
        .bind(params.model)
        .bind(params.provider)
        .bind(params.cli_tool)
        .bind(params.cwd)
        .bind(params.project_id)
        .bind(params.system_prompt)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Create a new agent from a typed [`NewAgent`] aggregate factory, atomically
    /// writing the `agent.enrolled` audit event for host CLI agents.
    ///
    /// Returns the UUID that was assigned to the new agent row.
    ///
    /// The INSERT and (for `RuntimeKind::Cli`) the events INSERT run inside a
    /// single database transaction so that either both succeed or neither does.
    /// Container and API agents produce no audit event from this path.
    ///
    /// This is the new canonical creation path. `create(CreateAgentParams)` is
    /// kept for backward compatibility until Task 6.1 migrates both call sites.
    pub async fn create_aggregate(&self, scope: &TenantScope, new: NewAgent) -> AppResult<Uuid> {
        let mut tx = self.pool.begin().await?;

        // For host-cli agents the PK is derived from the UUIDv7 embedded in
        // runtime_id ("host-<uuid>").  For all other runtime kinds a fresh
        // UUIDv7 is generated so the caller has a stable ID immediately.
        let id = new
            .runtime_id()
            .and_then(|rid| rid.strip_prefix("host-"))
            .and_then(|tail| Uuid::parse_str(tail).ok())
            .unwrap_or_else(Uuid::now_v7);

        let status_str = new.initial_status().to_string();

        sqlx::query(
            r#"INSERT INTO agents (
                   id, organization_id, workspace_id, user_id,
                   name, model, provider, cli_tool, cwd, project_id,
                   status, system_prompt,
                   runtime_kind, runtime_id, hmac_secret, nats_connect_password
               ) VALUES (
                   $1, $2, $3, $4,
                   $5, $6, $7, $8, $9, $10,
                   $11::agent_status, $12,
                   $13, $14, $15, $16
               )"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(new.workspace_id())
        .bind(scope.user_id().as_uuid())
        .bind(new.name())
        .bind(new.model())
        .bind(new.provider())
        .bind(new.cli_tool())
        .bind(new.cwd())
        .bind(new.project_id())
        .bind(&status_str)
        .bind(new.system_prompt())
        .bind(new.runtime_kind().to_string())
        .bind(new.runtime_id())
        .bind(new.hmac_secret())
        .bind(new.nats_connect_password())
        .execute(&mut *tx)
        .await?;

        if new.runtime_kind() == RuntimeKind::Cli {
            let payload = json!({
                "runtime_kind": "cli",
                "cli_tool": new.cli_tool(),
                "project_id": new.project_id(),
                "actor_user_id": scope.user_id().as_uuid(),
            });
            sqlx::query(
                r#"INSERT INTO events (organization_id, agent_id, event_type, payload)
                   VALUES ($1, $2, 'agent.enrolled', $3)"#,
            )
            .bind(scope.org_id().as_uuid())
            .bind(id)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(id)
    }

    /// Update agent fields (name, model, provider, system_prompt). Only non-None values are updated.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: AgentId,
        name: Option<&str>,
        model: Option<&str>,
        provider: Option<&str>,
        system_prompt: Option<&str>,
    ) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"UPDATE agents SET
                   name = COALESCE($3, name),
                   model = COALESCE($4, model),
                   provider = COALESCE($5, provider),
                   system_prompt = CASE
                       WHEN $6::text IS NULL THEN system_prompt
                       WHEN $6 = '' THEN NULL
                       ELSE $6
                   END,
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(model)
        .bind(provider)
        .bind(system_prompt)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Update an agent's status (tenant-scoped).
    pub async fn update_status(&self, scope: &TenantScope, id: AgentId, status: AgentStatus) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"UPDATE agents SET status = $3, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(status)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Record the docker container backing this agent, the sidecar's HMAC
    /// envelope key, and the per-agent NATS connect password. Flips status to
    /// `idle` in one statement so the frontend tab logic (which reads
    /// `containerId`) stays consistent with the actual lifecycle.
    ///
    /// `hmac_secret` is injected into the container env as `HMAC_SECRET`; the
    /// backend result consumer (issue #39) fetches it back to verify
    /// `SignedEnvelope` signatures. `nats_connect_password` is embedded in the
    /// container's `NATS_URL` user-info and validated by the auth callout
    /// service (issue #38 phase 2). Both are per-container random UUIDs — the
    /// two are stored in separate columns so a DB read that leaks one does
    /// not also yield the other attacker surface (subject authorization vs.
    /// envelope forgery). Callers MUST treat both strings as secrets — do not
    /// log them.
    pub async fn set_container(
        &self,
        scope: &TenantScope,
        id: AgentId,
        container_id: &str,
        hmac_secret: &str,
        nats_connect_password: &str,
    ) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"UPDATE agents
                  SET container_id          = $3,
                      hmac_secret           = $4,
                      nats_connect_password = $5,
                      status = 'idle',
                      started_at = COALESCE(started_at, NOW()),
                      updated_at = NOW()
                WHERE id = $1 AND organization_id = $2
                RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(container_id)
        .bind(hmac_secret)
        .bind(nats_connect_password)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Enroll an externally started Host CLI runtime. It receives the same
    /// per-agent NATS and HMAC credentials as a spawned container, but keeps
    /// `container_id` empty so Docker lifecycle actions do not target it.
    pub async fn set_host_runtime(
        &self,
        scope: &TenantScope,
        id: AgentId,
        runtime_id: &str,
        hmac_secret: &str,
        nats_connect_password: &str,
    ) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"UPDATE agents
                  SET container_id          = NULL,
                      runtime_id            = $3,
                      hmac_secret           = $4,
                      nats_connect_password = $5,
                      status                = 'offline',
                      started_at            = COALESCE(started_at, NOW()),
                      ended_at              = NULL,
                      updated_at            = NOW()
                WHERE id = $1 AND organization_id = $2
                RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(runtime_id)
        .bind(hmac_secret)
        .bind(nats_connect_password)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Clear container + HMAC + NATS password references and flip status to
    /// `offline` after a stop. Column-order note: `nats_connect_password` is
    /// nulled FIRST, then `hmac_secret` — so an interruption mid-statement
    /// (unlikely but nonzero) still disables NATS auth before envelope
    /// verification. Dropping both secrets is deliberate — keeping stale
    /// keys after the container dies only widens the window in which leaked
    /// material stays verifiable. A fresh `start_agent` writes fresh UUIDs.
    pub async fn clear_container(&self, scope: &TenantScope, id: AgentId) -> AppResult<Agent> {
        sqlx::query_as::<_, Agent>(
            r#"UPDATE agents
                  SET nats_connect_password = NULL,
                      hmac_secret           = NULL,
                      container_id          = NULL,
                      status = 'offline',
                      ended_at = NOW(),
                      updated_at = NOW()
                WHERE id = $1 AND organization_id = $2
                RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(id))
    }

    /// Hard-delete an agent (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: AgentId) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM agents WHERE id = $1 AND organization_id = $2")
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AgentRepositoryPolicy::agent_not_found(id));
        }
        Ok(())
    }

    /// Count agents for the current tenant.
    pub async fn count(&self, scope: &TenantScope) -> AppResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents WHERE organization_id = $1")
            .bind(scope.org_id().as_uuid())
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    // --- Collaborator operations ---

    /// List collaborators for an agent (after verifying tenant ownership).
    pub async fn list_collaborators(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Vec<AgentCollaborator>> {
        // Verify agent belongs to tenant
        self.find_by_id(scope, agent_id).await?;

        let collabs = sqlx::query_as::<_, AgentCollaborator>(
            r#"SELECT * FROM agent_collaborators
               WHERE agent_id = $1
               ORDER BY created_at ASC"#,
        )
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(collabs)
    }

    /// Add a collaborator to an agent.
    pub async fn add_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        // Verify agent belongs to tenant
        self.find_by_id(scope, agent_id).await?;

        sqlx::query_as::<_, AgentCollaborator>(
            r#"INSERT INTO agent_collaborators (agent_id, user_id, permission)
               VALUES ($1, $2, $3)
               RETURNING *"#,
        )
        .bind(agent_id.as_uuid())
        .bind(user_id)
        .bind(permission)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                AgentRepositoryPolicy::collaborator_already_exists()
            }
            _ => e.into(),
        })
    }

    /// Update a collaborator's permission.
    pub async fn update_collaborator(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        user_id: Uuid,
        permission: &str,
    ) -> AppResult<AgentCollaborator> {
        // Verify agent belongs to tenant
        self.find_by_id(scope, agent_id).await?;

        sqlx::query_as::<_, AgentCollaborator>(
            r#"UPDATE agent_collaborators SET permission = $3
               WHERE agent_id = $1 AND user_id = $2
               RETURNING *"#,
        )
        .bind(agent_id.as_uuid())
        .bind(user_id)
        .bind(permission)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::collaborator_not_found(agent_id, user_id))
    }

    /// Remove a collaborator from an agent.
    pub async fn remove_collaborator(&self, scope: &TenantScope, agent_id: AgentId, user_id: Uuid) -> AppResult<()> {
        // Verify agent belongs to tenant
        self.find_by_id(scope, agent_id).await?;

        let result = sqlx::query("DELETE FROM agent_collaborators WHERE agent_id = $1 AND user_id = $2")
            .bind(agent_id.as_uuid())
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AgentRepositoryPolicy::collaborator_not_found(agent_id, user_id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Build an `AgentListItem` with deterministic field values for serde tests.
    fn sample_item() -> AgentListItem {
        AgentListItem {
            id: Uuid::nil(),
            name: Some("Frontend".into()),
            status: AgentStatus::Working,
            model: Some("claude".into()),
            provider: Some("anthropic".into()),
            cli_tool: Some("claude".into()),
            runtime_kind: RuntimeKind::Container,
            system_prompt: None,
            container_id: Some("c-123".into()),
            cli_session_id: Some("cli-42".into()),
            cwd: Some("/workspace".into()),
            current_tool: Some("Edit".into()),
            tokens_current: 100,
            tokens_cumulative: 9000,
            git_status: Some("clean".into()),
            runtime_id: Some("af-deadbeef".into()),
            organization_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            project_id: None,
            user_id: Uuid::nil(),
            owner_username: Some("alice".into()),
            owner_email: Some("alice@example.com".into()),
            workspace_name: Some("Default".into()),
            project_name: Some("Workshop".into()),
            started_at: None,
            ended_at: None,
            last_activity_at: Some(Utc.timestamp_millis_opt(1_700_000_050_000).unwrap()),
            created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            updated_at: Utc.timestamp_millis_opt(1_700_000_100_000).unwrap(),
        }
    }

    #[test]
    fn agent_list_item_serializes_owner_fields_as_camel_case() {
        let value = serde_json::to_value(sample_item()).unwrap();
        assert_eq!(value["ownerUsername"], "alice");
        assert_eq!(value["ownerEmail"], "alice@example.com");
        assert_eq!(value["projectName"], "Workshop");
    }

    #[test]
    fn agent_list_item_timestamps_are_epoch_milliseconds() {
        let value = serde_json::to_value(sample_item()).unwrap();
        assert_eq!(value["createdAt"], 1_700_000_000_000_i64);
        assert_eq!(value["updatedAt"], 1_700_000_100_000_i64);
        assert_eq!(value["startedAt"], serde_json::Value::Null);
        assert_eq!(value["endedAt"], serde_json::Value::Null);
    }

    #[test]
    fn agent_list_item_snake_case_keys_are_absent() {
        // Guard against regressions where someone drops the
        // `#[serde(rename_all = "camelCase")]` attribute.
        let value = serde_json::to_value(sample_item()).unwrap();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("owner_username"));
        assert!(!obj.contains_key("owner_email"));
        assert!(!obj.contains_key("project_name"));
        assert!(!obj.contains_key("container_id"));
        assert!(!obj.contains_key("cli_session_id"));
        assert!(!obj.contains_key("created_at"));
    }

    #[test]
    fn agent_list_item_handles_missing_owner() {
        let mut item = sample_item();
        item.owner_username = None;
        item.owner_email = None;
        let value = serde_json::to_value(item).unwrap();
        assert_eq!(value["ownerUsername"], serde_json::Value::Null);
        assert_eq!(value["ownerEmail"], serde_json::Value::Null);
    }

    /// Seed one org + workspace + user + membership. Returns `(TenantScope, user_uuid)`.
    async fn seed_user_with_org(pool: &sqlx::PgPool) -> (TenantScope, uuid::Uuid) {
        let org_uuid = uuid::Uuid::new_v4();
        let user_uuid = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_uuid)
            .bind(format!("Org {org_uuid}"))
            .bind(format!("org-{org_uuid}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_uuid)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_uuid)
            .bind(format!("u-{user_uuid}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org_uuid)
            .bind(user_uuid)
            .execute(pool)
            .await
            .expect("seed membership");
        let scope = crate::test_support::tenant_scope_for_ids(org_uuid, user_uuid);
        (scope, user_uuid)
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn system_prompt_round_trips_through_create_and_enriched_read(pool: sqlx::PgPool) {
        let (scope, _user) = seed_user_with_org(&pool).await;
        let repo = AgentRepository::new(pool);
        let created = repo
            .create(
                &scope,
                CreateAgentParams {
                    name: Some("agent-1"),
                    model: Some("claude-sonnet-4-6"),
                    provider: Some("anthropic"),
                    workspace_id: Some(scope.org_id().as_uuid()),
                    system_prompt: Some("you are helpful"),
                    ..Default::default()
                },
            )
            .await
            .expect("create");
        assert_eq!(created.system_prompt.as_deref(), Some("you are helpful"));

        let enriched = repo.find_with_owner_by_id(&scope, created.id).await.expect("find_with_owner");
        assert_eq!(
            enriched.system_prompt.as_deref(),
            Some("you are helpful"),
            "enriched read must include system_prompt"
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn update_system_prompt_empty_string_clears_value(pool: sqlx::PgPool) {
        let (scope, _user) = seed_user_with_org(&pool).await;
        let repo = AgentRepository::new(pool);
        let created = repo
            .create(
                &scope,
                CreateAgentParams {
                    name: Some("agent"),
                    model: Some("claude-sonnet-4-6"),
                    provider: Some("anthropic"),
                    workspace_id: Some(scope.org_id().as_uuid()),
                    system_prompt: Some("original"),
                    ..Default::default()
                },
            )
            .await
            .expect("create");
        let agent_id = created.id;

        // None = no change
        let after_none = repo.update(&scope, agent_id, None, None, None, None).await.expect("update none");
        assert_eq!(after_none.system_prompt.as_deref(), Some("original"), "None preserves existing value");

        // Empty string = clear
        let after_clear = repo.update(&scope, agent_id, None, None, None, Some("")).await.expect("update clear");
        assert_eq!(after_clear.system_prompt, None, "empty string clears system_prompt");

        // Non-empty string = replace
        let after_set = repo.update(&scope, agent_id, None, None, None, Some("new prompt")).await.expect("update set");
        assert_eq!(after_set.system_prompt.as_deref(), Some("new prompt"));
    }
}
