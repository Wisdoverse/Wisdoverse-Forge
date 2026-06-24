use agentforge_api::repositories::dev_environment::DevEnvironmentRepository;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::TenantScope;
use serde_json::json;
use sqlx::PgPool;
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
        .bind(format!("dev-env-{user_id}@example.com"))
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
