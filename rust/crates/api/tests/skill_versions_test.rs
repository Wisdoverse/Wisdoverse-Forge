//! Unit 2.3 coverage for skill version snapshots and rollback.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt_with_axes, test_app_with_mock_provider};

struct SkillVersionsSeed {
    workspace_id: Uuid,
    other_workspace_id: Uuid,
    team_id: Uuid,
    owner_jwt: String,
    other_workspace_jwt: String,
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn assembled(parts: &[&str]) -> String {
    parts.concat()
}

async fn json_request(
    app: Router,
    method: Method,
    uri: impl AsRef<str>,
    jwt: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri.as_ref()).header(header::AUTHORIZATION, bearer(jwt));
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app.oneshot(builder.body(body).expect("request body")).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("response body");
    let value = if bytes.is_empty() { json!({}) } else { serde_json::from_slice(&bytes).expect("json response") };
    (status, value)
}

async fn seed_skill_versions(pool: &PgPool) -> SkillVersionsSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Other')")
        .bind(other_workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed other workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(owner_id)
        .bind(format!("owner-{owner_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed owner");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(owner_id)
        .execute(pool)
        .await
        .expect("seed org owner");
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', 'platform')")
        .bind(team_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(team_id)
        .bind(owner_id)
        .execute(pool)
        .await
        .expect("seed team membership");

    let owner_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(workspace_id), None, None);
    let other_workspace_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(other_workspace_id), None, None);

    SkillVersionsSeed { workspace_id, other_workspace_id, team_id, owner_jwt, other_workspace_jwt }
}

async fn create_skill(app: Router, jwt: &str, payload: Value) -> (StatusCode, Value) {
    json_request(app, Method::POST, "/api/v1/skills", jwt, Some(payload)).await
}

async fn version_rows(pool: &PgPool, skill_id: Uuid) -> Vec<(i32, Value)> {
    sqlx::query_as::<_, (i32, Value)>(
        "SELECT version, snapshot FROM skill_versions WHERE skill_id = $1 ORDER BY version ASC",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .expect("skill version rows")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn skill_updates_append_versions_and_restore_in_place(pool: PgPool) {
    let seed = seed_skill_versions(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "rollback-skill",
            "description": "initial description",
            "trigger_pattern": "rollback",
            "content": "version one content",
            "scope_kind": "org"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");
    assert_eq!(created["data"]["version"], 1);

    let (status, updated) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/skills/{skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "content": "version two content" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated}");
    assert_eq!(updated["data"]["version"], 2);

    let (status, updated_again) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/skills/{skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "name": "rollback-skill-renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated_again}");
    assert_eq!(updated_again["data"]["version"], 3);

    let rows = version_rows(&pool, skill_id).await;
    assert_eq!(rows.iter().map(|(version, _)| *version).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(rows[0].1["name"], "rollback-skill");
    assert_eq!(rows[0].1["content"], "version one content");
    assert_eq!(rows[1].1["content"], "version two content");

    let (status, stale_restore) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {stale_restore}");
    assert_eq!(version_rows(&pool, skill_id).await.len(), 2);

    let (status, restored) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 3 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {restored}");
    assert_eq!(restored["data"]["id"], skill_id.to_string());
    assert_eq!(restored["data"]["name"], "rollback-skill");
    assert_eq!(restored["data"]["content"], "version one content");
    assert_eq!(restored["data"]["version"], 4);

    let skill_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM skills WHERE id = $1")
        .bind(skill_id)
        .fetch_one(&pool)
        .await
        .expect("skill count");
    assert_eq!(skill_count, 1);

    let rows = version_rows(&pool, skill_id).await;
    assert_eq!(rows.iter().map(|(version, _)| *version).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(rows[2].1["name"], "rollback-skill-renamed");

    let (status, listed) =
        json_request(app.clone(), Method::GET, format!("/api/v1/skills/{skill_id}/versions"), &seed.owner_jwt, None)
            .await;
    assert_eq!(status, StatusCode::OK, "body: {listed}");
    let listed_versions = listed["data"]
        .as_array()
        .expect("version list")
        .iter()
        .map(|row| row["version"].as_i64().expect("version"))
        .collect::<Vec<_>>();
    assert_eq!(listed_versions, vec![3, 2, 1]);

    let audit = sqlx::query_as::<_, (String, Option<Uuid>, Value)>(
        "SELECT action, resource_id, details FROM audit_log WHERE resource_type = 'skill' ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("skill audit rows");
    assert!(audit.iter().any(|(action, resource_id, details)| {
        action == "governance.context.skill.updated"
            && *resource_id == Some(skill_id)
            && details["from_version"] == 1
            && details["resulting_version"] == 2
            && details["skill_version_id"].is_string()
    }));
    assert!(audit.iter().any(|(action, resource_id, details)| {
        action == "governance.context.skill.restored"
            && *resource_id == Some(skill_id)
            && details["target_version"] == 1
            && details["from_version"] == 3
            && details["resulting_version"] == 4
    }));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn revoked_skill_restore_is_422_and_does_not_append_history(pool: PgPool) {
    let seed = seed_skill_versions(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "revoked-rollback",
            "content": "safe content",
            "scope_kind": "org"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");

    let (status, deleted) =
        json_request(app.clone(), Method::DELETE, format!("/api/v1/skills/{skill_id}"), &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {deleted}");
    assert_eq!(version_rows(&pool, skill_id).await.len(), 1);

    let (status, body) = json_request(
        app,
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");
    assert_eq!(version_rows(&pool, skill_id).await.len(), 1);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn secret_snapshot_restore_is_rejected_with_redacted_audit(pool: PgPool) {
    let seed = seed_skill_versions(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let secret = assembled(&["AK", "IA", "1234567890ABCDEF"]);

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "secret-rollback",
            "content": "safe historical content",
            "scope_kind": "org"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");

    let (status, updated) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/skills/{skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "content": "current safe content" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated}");
    assert_eq!(updated["data"]["version"], 2);

    sqlx::query(
        r#"UPDATE skill_versions
              SET snapshot = jsonb_set(
                  jsonb_set(snapshot, '{content}', to_jsonb($2::text), false),
                  '{sensitivity}',
                  to_jsonb('internal'::text),
                  false
              )
            WHERE skill_id = $1 AND version = 1"#,
    )
    .bind(skill_id)
    .bind(format!("AWS_ACCESS_KEY_ID={secret}"))
    .execute(&pool)
    .await
    .expect("simulate newly detected secret in historical snapshot");

    let (status, body) = json_request(
        app,
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let (version, content): (i32, String) = sqlx::query_as("SELECT version, content FROM skills WHERE id = $1")
        .bind(skill_id)
        .fetch_one(&pool)
        .await
        .expect("current skill unchanged");
    assert_eq!(version, 2);
    assert_eq!(content, "current safe content");
    assert_eq!(version_rows(&pool, skill_id).await.len(), 1);

    let (resource_id, details): (Option<Uuid>, Value) = sqlx::query_as(
        "SELECT resource_id, details FROM audit_log WHERE action = 'governance.context.skill.mutation_rejected' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("secret rollback rejection audit");
    assert_eq!(resource_id, Some(skill_id));
    assert_eq!(details["operation"], "restore_version");
    assert_eq!(details["reason"], "secret_detected");
    assert_eq!(details["target_version"], 1);
    assert!(!details.to_string().contains(&secret), "rollback audit must not persist raw secret material");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn restore_version_scope_expansion_requires_confirmation(pool: PgPool) {
    let seed = seed_skill_versions(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "scope-rollback",
            "content": "user-scoped v1",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");

    let (status, updated) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/skills/{skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "content": "current safe content" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated}");
    assert_eq!(updated["data"]["version"], 2);

    sqlx::query(
        r#"UPDATE skill_versions
              SET snapshot = jsonb_set(
                  jsonb_set(snapshot, '{scope_kind}', to_jsonb('team'::text), false),
                  '{scope_id}',
                  to_jsonb($2::text),
                  false
              )
            WHERE skill_id = $1 AND version = 1"#,
    )
    .bind(skill_id)
    .bind(seed.team_id.to_string())
    .execute(&pool)
    .await
    .expect("simulate team-scoped historical snapshot");

    let (status, rejected) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {rejected}");

    let (version, scope_kind): (i32, Option<String>) =
        sqlx::query_as("SELECT version, scope_kind FROM skills WHERE id = $1")
            .bind(skill_id)
            .fetch_one(&pool)
            .await
            .expect("skill unchanged");
    assert_eq!(version, 2);
    assert_eq!(scope_kind.as_deref(), Some("user"));
    assert_eq!(version_rows(&pool, skill_id).await.len(), 1);

    let details = sqlx::query_scalar::<_, Value>(
        "SELECT details FROM audit_log WHERE action = 'governance.context.skill.mutation_rejected' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("restore scope rejection audit");
    assert_eq!(details["operation"], "restore_version");
    assert_eq!(details["reason"], "confirmation_required");
    assert_eq!(details["from_scope_kind"], "user");
    assert_eq!(details["to_scope_kind"], "team");

    let (status, restored) = json_request(
        app,
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.owner_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2, "confirm_expansion": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {restored}");
    assert_eq!(restored["data"]["version"], 3);
    assert_eq!(restored["data"]["scope_kind"], "team");
    assert_eq!(restored["data"]["scope_id"], seed.team_id.to_string());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cross_workspace_version_read_and_restore_are_forbidden(pool: PgPool) {
    let seed = seed_skill_versions(&pool).await;
    assert_ne!(seed.workspace_id, seed.other_workspace_id);
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "workspace-bound-history",
            "content": "safe content",
            "scope_kind": "org"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");

    let (status, updated) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/skills/{skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "content": "safe content v2" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated}");

    let (status, list_body) = json_request(
        app.clone(),
        Method::GET,
        format!("/api/v1/skills/{skill_id}/versions"),
        &seed.other_workspace_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {list_body}");

    let (status, restore_body) = json_request(
        app,
        Method::POST,
        format!("/api/v1/skills/{skill_id}/restore-version"),
        &seed.other_workspace_jwt,
        Some(json!({ "version": 1, "expected_current_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {restore_body}");
}
