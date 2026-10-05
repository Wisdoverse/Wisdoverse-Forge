use agentforge_core::{AgentId, AppResult, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::gateway::{GatewayTerminalAttachTarget, terminal_authorization_denied};
use crate::repositories::agent::AgentRepository;
use crate::repositories::user::UserRepository;

pub(crate) struct GatewayTerminalService {
    pool: PgPool,
    agents: AgentRepository,
    users: UserRepository,
}

impl GatewayTerminalService {
    pub(crate) fn new(pool: PgPool, agents: AgentRepository) -> Self {
        let users = UserRepository::new(pool.clone());
        Self { pool, agents, users }
    }

    pub(crate) fn from_pool(pool: PgPool) -> Self {
        Self::new(pool.clone(), AgentRepository::new(pool))
    }

    async fn authorize(&self, scope: &TenantScope, agent_id: Uuid) -> AppResult<()> {
        if self.users.active_session_floor(scope.user_id()).await?.is_none()
            || self.users.find_membership_role(scope.user_id(), scope.org_id().as_uuid()).await?.is_none()
        {
            return Err(terminal_authorization_denied());
        }
        super::agent::authorize_agent_action(&self.agents, scope, AgentId::from(agent_id), "edit").await
    }

    /// Recheck caller authority and container identity before every terminal I/O.
    pub(crate) async fn current_access(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
        container_id: &str,
    ) -> AppResult<bool> {
        self.authorize(scope, agent_id).await?;
        Ok(self.agents.find_by_id(scope, AgentId::from(agent_id)).await?.container_id.as_deref() == Some(container_id))
    }

    pub(crate) async fn attach_target(&self, scope: &TenantScope, agent_id: Uuid) -> GatewayTerminalAttachTarget {
        if let Err(err) = self.authorize(scope, agent_id).await {
            return GatewayTerminalAttachTarget::lookup_failed(&err.kind);
        }
        match self.agents.find_by_id(scope, AgentId::from(agent_id)).await {
            Ok(agent) => agent
                .container_id
                .map(GatewayTerminalAttachTarget::ready)
                .unwrap_or_else(GatewayTerminalAttachTarget::missing_container),
            Err(err) => GatewayTerminalAttachTarget::lookup_failed(&err.kind),
        }
    }

    /// Fence each browser-terminal input against container replacement. The
    /// short DB lease covers the write-to-Docker handoff; normal hook events
    /// keep `agents.status=working` once the CLI begins actual work.
    pub(crate) async fn admit_input(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
        expected_container_id: &str,
    ) -> AppResult<bool> {
        self.authorize(scope, agent_id).await?;
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id).await?;
        let admitted = AgentRepository::renew_interactive_lease_in_tx(
            &mut tx,
            scope,
            AgentId::from(agent_id),
            expected_container_id,
        )
        .await?;
        tx.commit().await?;
        Ok(admitted)
    }
}

#[cfg(test)]
mod sqlx_tests {
    use super::*;
    use crate::domain::agent::NewAgent;
    use crate::repositories::agent::AgentRepository;
    use agentforge_core::CliToolKind;

    async fn seed(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
        let org_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let editor_id = Uuid::new_v4();
        let viewer_id = Uuid::new_v4();
        let bare_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Terminal test', $2)")
            .bind(org_id)
            .bind(format!("terminal-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        for (id, email, role) in [
            (owner_id, "dev@example.com".to_string(), "owner"),
            (editor_id, format!("editor-{editor_id}@example.com"), "member"),
            (viewer_id, format!("viewer-{viewer_id}@example.com"), "member"),
            (bare_id, format!("bare-{bare_id}@example.com"), "member"),
        ] {
            sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
                .bind(id)
                .bind(email)
                .execute(pool)
                .await
                .expect("seed user");
            sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
                .bind(org_id)
                .bind(id)
                .bind(role)
                .execute(pool)
                .await
                .expect("seed membership");
        }
        let owner_scope = crate::test_support::tenant_scope_for_ids(org_id, owner_id);
        let agent_id = AgentRepository::new(pool.clone())
            .create_aggregate(
                &owner_scope,
                NewAgent::container(
                    &owner_scope,
                    CliToolKind::Claude,
                    Some("terminal-authz"),
                    None,
                    None,
                    org_id,
                    None,
                    None,
                )
                .expect("build agent"),
            )
            .await
            .expect("create agent");
        sqlx::query(
            "UPDATE agents SET container_id = 'container-current', container_image_identity = $2 WHERE id = $1",
        )
        .bind(agent_id)
        .bind(serde_json::json!({
            "source": "agentforge-agent:claude",
            "imageId": format!("sha256:{}", "a".repeat(64)),
            "versionSource": "not-reported",
            "trust": "host-local"
        }))
        .execute(pool)
        .await
        .expect("set container identity");
        sqlx::query(
            "INSERT INTO agent_collaborators (agent_id, user_id, permission) VALUES ($1, $2, 'edit'), ($1, $3, 'view')",
        )
        .bind(agent_id)
        .bind(editor_id)
        .bind(viewer_id)
        .execute(pool)
        .await
        .expect("seed collaborators");
        (org_id, owner_id, editor_id, viewer_id, bare_id, agent_id)
    }

    fn scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
        crate::test_support::tenant_scope_for_ids(org_id, user_id)
    }

    async fn lease(pool: &PgPool, agent_id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT interactive_lease_expires_at FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(pool)
            .await
            .expect("read interactive lease")
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn terminal_authorization_and_container_fences_are_rechecked(pool: PgPool) {
        let (org_id, owner_id, editor_id, viewer_id, bare_id, agent_id) = seed(&pool).await;
        let service = GatewayTerminalService::from_pool(pool.clone());
        let owner_scope = scope(org_id, owner_id);
        let editor_scope = scope(org_id, editor_id);
        let viewer_scope = scope(org_id, viewer_id);
        let bare_scope = scope(org_id, bare_id);

        assert!(service.current_access(&owner_scope, agent_id, "container-current").await.expect("owner access"));
        assert!(service.current_access(&editor_scope, agent_id, "container-current").await.expect("editor access"));
        for denied in [&viewer_scope, &bare_scope] {
            assert!(service.current_access(denied, agent_id, "container-current").await.is_err());
            assert!(service.admit_input(denied, agent_id, "container-current").await.is_err());
        }
        assert!(lease(&pool, agent_id).await.is_none(), "denied callers must not create a lease");

        assert!(service.admit_input(&editor_scope, agent_id, "container-current").await.expect("editor input"));
        let active_lease = lease(&pool, agent_id).await.expect("admitted input creates lease");

        sqlx::query("DELETE FROM agent_collaborators WHERE agent_id = $1 AND user_id = $2")
            .bind(agent_id)
            .bind(editor_id)
            .execute(&pool)
            .await
            .expect("revoke collaborator");
        assert!(service.current_access(&editor_scope, agent_id, "container-current").await.is_err());
        assert!(service.admit_input(&editor_scope, agent_id, "container-current").await.is_err());
        assert_eq!(lease(&pool, agent_id).await, Some(active_lease), "revocation must not renew lease");

        sqlx::query("INSERT INTO agent_collaborators (agent_id, user_id, permission) VALUES ($1, $2, 'edit')")
            .bind(agent_id)
            .bind(editor_id)
            .execute(&pool)
            .await
            .expect("restore editor");
        sqlx::query("DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(editor_id)
            .execute(&pool)
            .await
            .expect("remove org membership");
        assert!(service.current_access(&editor_scope, agent_id, "container-current").await.is_err());
        assert!(service.admit_input(&editor_scope, agent_id, "container-current").await.is_err());
        assert_eq!(lease(&pool, agent_id).await, Some(active_lease), "membership removal must not renew lease");

        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(org_id)
            .bind(editor_id)
            .execute(&pool)
            .await
            .expect("restore membership");
        sqlx::query("UPDATE agents SET container_id = 'container-replaced' WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("replace container");
        assert!(
            !service.current_access(&editor_scope, agent_id, "container-current").await.expect("current access lookup")
        );
        assert!(
            !service.admit_input(&editor_scope, agent_id, "container-current").await.expect("stale input rejected")
        );
        assert_eq!(lease(&pool, agent_id).await, Some(active_lease), "stale container must not renew lease");
    }
}
