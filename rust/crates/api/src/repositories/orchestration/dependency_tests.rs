//! Runtime tests for the prerequisite containment query.

use super::*;
use crate::test_support::tenant_scope_for_ids;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../db/migrations")]
async fn dependent_task_ids_match_params_containment(pool: PgPool) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Dep Org', 'dep-org')")
        .bind(org_id)
        .execute(&pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'dep@example.com')")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed user");
    let prereq = Uuid::new_v4();
    let missing = Uuid::new_v4();
    let single = Uuid::new_v4();
    let multi = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
         VALUES ($1, $2, 'Prereq', 'completed', 'normal', $3)",
    )
    .bind(prereq)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed prereq");
    for (id, deps) in [(single, format!("[\"{prereq}\"]")), (multi, format!("[\"{prereq}\", \"{missing}\"]"))] {
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by, params)
             VALUES ($1, $2, 'Dependent', 'blocked', 'normal', $3, jsonb_build_object('dependency_ids', $4::jsonb))",
        )
        .bind(id)
        .bind(org_id)
        .bind(user_id)
        .bind(deps)
        .execute(&pool)
        .await
        .expect("seed dependent");
    }

    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = OrchestrationTaskRepository::new(pool.clone());
    let ids = repo.dependent_task_ids(&scope, prereq).await.expect("candidates");
    assert!(ids.contains(&single), "single-dependency dependent is a candidate");
    assert!(ids.contains(&multi), "multi-dependency dependent is a candidate");
}
