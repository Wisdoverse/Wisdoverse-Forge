use agentforge_api::repositories::dev_environment::DevEnvironmentRepository;
use agentforge_api::services::dev_environment::{DevEnvironmentRuntime, DevEnvironmentService};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::TenantScope;
use agentforge_platform::types::{ContainerConfig, ContainerState};
use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

async fn seed_scope(pool: &PgPool) -> TenantScope {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind("dev@example.com")
        .execute(pool)
        .await
        .expect("seed user");

    tenant_scope_for_ids(org_id, user_id)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn update_status_can_clear_container_id_on_stop(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let repo = DevEnvironmentRepository::new(pool);
    let env = repo.create(&scope, "dev-env", None, &json!({"image": "ubuntu:22.04"})).await.unwrap();

    let running = repo.update_status(&scope, env.id.as_uuid(), "running", Some("ctr-dev")).await.unwrap();
    assert_eq!(running.status, "running");
    assert_eq!(running.container_id.as_deref(), Some("ctr-dev"));

    let stopped = repo.update_status(&scope, env.id.as_uuid(), "stopped", None).await.unwrap();
    assert_eq!(stopped.status, "stopped");
    assert!(stopped.container_id.is_none());
}

#[derive(Default)]
struct CountingRuntime(AtomicUsize);

#[async_trait]
impl DevEnvironmentRuntime for CountingRuntime {
    async fn create_container(&self, _config: ContainerConfig) -> agentforge_core::AppResult<String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("test-container".into())
    }
    async fn start_container(&self, _container_id: &str) -> agentforge_core::AppResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn stop_container(&self, _container_id: &str, _timeout_secs: i64) -> agentforge_core::AppResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn remove_container(&self, _container_id: &str, _force: bool) -> agentforge_core::AppResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn inspect_container(&self, _container_id: &str) -> agentforge_core::AppResult<ContainerState> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ContainerState::Running)
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn create_checks_workspace_mount_and_project_authority_before_persisting(pool: PgPool) {
    let owner_scope = seed_scope(&pool).await;
    let org_id = owner_scope.org_id().as_uuid();
    let owner_id = owner_scope.user_id().as_uuid();
    let member_id = Uuid::new_v4();
    let other_org_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let foreign_project_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(owner_id)
        .execute(&pool)
        .await
        .expect("seed owner membership");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(member_id)
        .bind(format!("member-{member_id}@example.com"))
        .execute(&pool)
        .await
        .expect("seed member");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .expect("seed member membership");
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Foreign', $2)")
        .bind(other_org_id)
        .bind(format!("foreign-{other_org_id}"))
        .execute(&pool)
        .await
        .expect("seed foreign org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Foreign')")
        .bind(other_workspace_id)
        .bind(other_org_id)
        .execute(&pool)
        .await
        .expect("seed foreign workspace");
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Foreign', 'foreign')")
        .bind(other_workspace_id)
        .bind(other_org_id)
        .execute(&pool)
        .await
        .expect("seed foreign team");
    sqlx::query("INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug) VALUES ($1, $2, $3, $3, 'Foreign', 'foreign')")
        .bind(foreign_project_id)
        .bind(other_org_id)
        .bind(other_workspace_id)
        .execute(&pool)
        .await
        .expect("seed foreign project");

    let runtime = Arc::new(CountingRuntime::default());
    let service = DevEnvironmentService::from_runtime(pool.clone(), Some(runtime.clone()), Vec::new());
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dev_environments WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .expect("count initial environments");
    let symbolic_mount = json!({"image":"ubuntu:22.04", "mounts":[{"source":"workspace", "target":"/workspace"}]});

    let member_scope = tenant_scope_for_ids(org_id, member_id);
    assert!(service.create(&member_scope, "member-mount", None, &symbolic_mount).await.is_err());
    let owner_foreign_project = service
        .create(&owner_scope, "foreign-project", Some(foreign_project_id), &json!({"image":"ubuntu:22.04"}))
        .await;
    assert!(owner_foreign_project.is_err(), "org owner must not reference a foreign-org project");

    let after_denials: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dev_environments WHERE organization_id = $1")
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .expect("count after denied creates");
    assert_eq!(after_denials, before, "denied create attempts must not persist records");
    assert_eq!(runtime.0.load(Ordering::SeqCst), 0, "authorization failures must not call runtime");

    let created = service
        .create(&owner_scope, "owner-project", None, &json!({"image":"ubuntu:22.04"}))
        .await
        .expect("owner can create a dev environment");
    assert_eq!(created.created_by.as_uuid(), owner_id);
    service.start(&owner_scope, created.id.as_uuid()).await.expect("owner can start a dev environment");
    assert_eq!(runtime.0.load(Ordering::SeqCst), 2, "authorized start reaches mocked create and start calls");
}
