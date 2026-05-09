//! Unit 2.1 coverage for governed memory items.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt_with_axes, test_app_with_mock_provider};

struct GovernanceSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
    project_id: Uuid,
    owner_id: Uuid,
    owner_jwt: String,
    teammate_jwt: String,
    outsider_jwt: String,
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

async fn seed_governance(pool: &PgPool) -> GovernanceSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let teammate_id = Uuid::new_v4();
    let outsider_id = Uuid::new_v4();

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

    for user_id in [owner_id, teammate_id, outsider_id] {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(org_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed org member");
    }

    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', 'platform')")
        .bind(team_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed team");
    for user_id in [owner_id, teammate_id] {
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(team_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed team member");
    }

    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $3, $4, 'Control Plane', 'control-plane')",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .execute(pool)
    .await
    .expect("seed project");
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(project_id)
        .bind(teammate_id)
        .execute(pool)
        .await
        .expect("seed project member");

    let owner_jwt = mint_test_jwt_with_axes(org_id, owner_id, "member", Some(workspace_id), None, None);
    let teammate_jwt = mint_test_jwt_with_axes(org_id, teammate_id, "member", Some(workspace_id), None, None);
    let outsider_jwt = mint_test_jwt_with_axes(org_id, outsider_id, "member", Some(workspace_id), None, None);

    GovernanceSeed { org_id, workspace_id, team_id, project_id, owner_id, owner_jwt, teammate_jwt, outsider_jwt }
}

async fn create_memory(app: Router, jwt: &str, payload: Value) -> (StatusCode, Value) {
    json_request(app, Method::POST, "/api/v1/context/memory-items", jwt, Some(payload)).await
}

#[sqlx::test(migrations = "../db/migrations")]
async fn user_memory_skips_content_by_default_and_read_content_audits(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Deploy path",
            "content": "Use make prod-ext for production contract validation.",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let memory_id = body["data"]["id"].as_str().expect("memory id");
    assert!(body["data"].get("content").is_none(), "summary response must not serialize raw content: {body}");

    let (status, list_body) =
        json_request(app.clone(), Method::GET, "/api/v1/context/memory-items", &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {list_body}");
    assert_eq!(list_body["data"].as_array().expect("items").len(), 1);
    assert!(list_body["data"][0].get("content").is_none());

    let (status, get_body) = json_request(
        app.clone(),
        Method::GET,
        format!("/api/v1/context/memory-items/{memory_id}"),
        &seed.owner_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {get_body}");
    assert!(get_body["data"].get("content").is_none());

    let (status, content_body) = json_request(
        app,
        Method::GET,
        format!("/api/v1/context/memory-items/{memory_id}/content"),
        &seed.owner_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {content_body}");
    assert_eq!(content_body["data"]["content"], "Use make prod-ext for production contract validation.");
    assert_eq!(content_body["data"]["content_redacted"], false);

    let audit_details = sqlx::query_scalar::<_, Value>(
        "SELECT details FROM audit_log WHERE action = 'governance.context.memory.content_read'",
    )
    .fetch_one(&pool)
    .await
    .expect("content read audit");
    assert!(audit_details.get("content").is_none(), "audit details must not contain raw content");
    let leaked_resource_ids = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'governance.context.memory.content_read' AND resource_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("content read resource id leak count");
    assert_eq!(leaked_resource_ids, 0, "content_read audit must not expose memory item IDs");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn user_team_and_project_scope_visibility_is_membership_bound(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, user_body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Personal preference",
            "content": "Prefer concise deployment notes.",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {user_body}");
    let user_memory_id = user_body["data"]["id"].as_str().expect("user memory id");

    let (status, teammate_user_get) = json_request(
        app.clone(),
        Method::GET,
        format!("/api/v1/context/memory-items/{user_memory_id}"),
        &seed.teammate_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {teammate_user_get}");

    let (status, team_body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Team runbook",
            "content": "Team uses the governed Rust API path.",
            "scope_kind": "team",
            "scope_id": seed.team_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {team_body}");

    let (status, teammate_list) =
        json_request(app.clone(), Method::GET, "/api/v1/context/memory-items", &seed.teammate_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {teammate_list}");
    let teammate_titles = teammate_list["data"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect::<Vec<_>>();
    assert!(teammate_titles.contains(&"Team runbook"));
    assert!(!teammate_titles.contains(&"Personal preference"));

    let (status, outsider_list) =
        json_request(app.clone(), Method::GET, "/api/v1/context/memory-items", &seed.outsider_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {outsider_list}");
    assert!(outsider_list["data"].as_array().expect("items").is_empty());

    let (status, project_body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Project context",
            "content": "Control plane tasks need workspace-scoped evidence.",
            "scope_kind": "project",
            "scope_id": seed.project_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {project_body}");

    let (status, teammate_project_list) =
        json_request(app.clone(), Method::GET, "/api/v1/context/memory-items", &seed.teammate_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {teammate_project_list}");
    let teammate_titles = teammate_project_list["data"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect::<Vec<_>>();
    assert!(teammate_titles.contains(&"Project context"));

    sqlx::query(
        r#"INSERT INTO memory_items (
               organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
               title, content, ttl_expires_at
           )
           VALUES ($1, $2, $3, 'user', $3, 'Expired', 'expired content', now() - interval '1 hour')"#,
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(seed.owner_id)
    .execute(&pool)
    .await
    .expect("insert expired item");

    let other_org_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(other_org_id)
        .bind(format!("Other Org {other_org_id}"))
        .bind(format!("other-org-{other_org_id}"))
        .execute(&pool)
        .await
        .expect("seed other org");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(other_org_id)
        .bind(seed.owner_id)
        .execute(&pool)
        .await
        .expect("seed owner in other org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Other')")
        .bind(other_workspace_id)
        .bind(other_org_id)
        .execute(&pool)
        .await
        .expect("seed other workspace");
    sqlx::query(
        r#"INSERT INTO memory_items (
               organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
               title, content
           )
           VALUES ($1, $2, $3, 'user', $3, 'Other org', 'other org content')"#,
    )
    .bind(other_org_id)
    .bind(other_workspace_id)
    .bind(seed.owner_id)
    .execute(&pool)
    .await
    .expect("insert cross-org item");

    let (status, owner_list) =
        json_request(app, Method::GET, "/api/v1/context/memory-items", &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {owner_list}");
    let owner_titles = owner_list["data"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["title"].as_str())
        .collect::<Vec<_>>();
    assert!(!owner_titles.contains(&"Expired"));
    assert!(!owner_titles.contains(&"Other org"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn secret_memory_requires_redacted_flow_and_rejection_is_audited(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let secret = assembled(&["AK", "IA", "1234567890ABCDEF"]);

    let (status, body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Secret candidate",
            "content": format!("AWS_ACCESS_KEY_ID={secret}"),
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let rejection_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'governance.context.memory.rejected'",
    )
    .fetch_one(&pool)
    .await
    .expect("rejection audit count");
    assert_eq!(rejection_count, 1);

    let (status, redacted_body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Redacted secret",
            "content": format!("AWS_ACCESS_KEY_ID={secret}"),
            "scope_kind": "user",
            "redacted": true
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {redacted_body}");
    assert_eq!(redacted_body["data"]["content_redacted"], true);
    assert_eq!(redacted_body["data"]["sensitivity"], "secret_detected");
    let memory_id = redacted_body["data"]["id"].as_str().expect("memory id");

    let (status, content_body) = json_request(
        app,
        Method::GET,
        format!("/api/v1/context/memory-items/{memory_id}/content"),
        &seed.owner_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {content_body}");
    assert!(
        !content_body["data"]["content"].as_str().expect("content").contains(&secret),
        "redacted flow must not persist raw secret material"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn workspace_axis_is_required_for_memory_writes(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let no_workspace_jwt = mint_test_jwt_with_axes(seed.org_id, seed.owner_id, "member", None, None, None);

    let (status, list_body) =
        json_request(app.clone(), Method::GET, "/api/v1/context/memory-items", &no_workspace_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {list_body}");
    assert!(list_body["data"].as_array().expect("items").is_empty());

    let (status, create_body) = create_memory(
        app,
        &no_workspace_jwt,
        json!({
            "title": "No workspace",
            "content": "This request lacks the workspace execution boundary.",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {create_body}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn switch_context_workspace_axis_enables_memory_write(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let org_only_jwt = mint_test_jwt_with_axes(seed.org_id, seed.owner_id, "member", None, None, None);

    let (status, switch_body) = json_request(
        app.clone(),
        Method::POST,
        "/api/v1/auth/switch-context",
        &org_only_jwt,
        Some(json!({
            "orgId": seed.org_id,
            "workspaceId": seed.workspace_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {switch_body}");
    let workspace_jwt = switch_body["accessToken"].as_str().expect("workspace access token");

    let (status, create_body) = create_memory(
        app,
        workspace_jwt,
        json!({
            "title": "Workspace token",
            "content": "switch-context minted a workspace-scoped token.",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {create_body}");
    assert_eq!(create_body["data"]["workspace_id"], seed.workspace_id.to_string());
    assert!(create_body["data"].get("content").is_none());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn mutation_routes_audit_and_preserve_owner_write_boundary(pool: PgPool) {
    let seed = seed_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, body) = create_memory(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "title": "Mutable",
            "content": "Initial memory",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let memory_id = body["data"]["id"].as_str().expect("memory id");

    let (status, update_body) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/context/memory-items/{memory_id}"),
        &seed.owner_jwt,
        Some(json!({
            "title": "Mutable updated",
            "content": "Updated memory"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {update_body}");
    assert_eq!(update_body["data"]["title"], "Mutable updated");

    let ttl = (Utc::now() + Duration::days(7)).to_rfc3339();
    let (status, ttl_body) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/memory-items/{memory_id}/extend-ttl"),
        &seed.owner_jwt,
        Some(json!({ "ttl_expires_at": ttl })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ttl_body}");
    assert!(ttl_body["data"]["ttl_expires_at"].is_string());

    let (status, rejected_reclassify) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/memory-items/{memory_id}/reclassify-scope"),
        &seed.owner_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {rejected_reclassify}");
    assert_eq!(rejected_reclassify["error"]["code"], "UNPROCESSABLE_ENTITY");

    let (status, reclassify_body) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/memory-items/{memory_id}/reclassify-scope"),
        &seed.owner_jwt,
        Some(json!({
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "confirm_expansion": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {reclassify_body}");
    assert_eq!(reclassify_body["data"]["scope_kind"], "team");

    let (status, teammate_update) = json_request(
        app.clone(),
        Method::PATCH,
        format!("/api/v1/context/memory-items/{memory_id}"),
        &seed.teammate_jwt,
        Some(json!({ "title": "teammate takeover" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {teammate_update}");

    let (status, revoke_body) = json_request(
        app.clone(),
        Method::POST,
        format!("/api/v1/context/memory-items/{memory_id}/revoke"),
        &seed.owner_jwt,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {revoke_body}");
    assert_eq!(revoke_body["data"]["state"], "revoked");

    let (status, list_body) =
        json_request(app, Method::GET, "/api/v1/context/memory-items", &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {list_body}");
    assert!(list_body["data"].as_array().expect("items").is_empty());

    let retained_state = sqlx::query_scalar::<_, String>("SELECT state FROM memory_items WHERE id = $1")
        .bind(Uuid::parse_str(memory_id).expect("memory uuid"))
        .fetch_one(&pool)
        .await
        .expect("revoked memory retained");
    assert_eq!(retained_state, "revoked");

    let actions = sqlx::query_scalar::<_, String>(
        "SELECT action FROM audit_log WHERE action LIKE 'governance.context.memory.%' ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("audit actions");
    for expected in [
        "governance.context.memory.created",
        "governance.context.memory.updated",
        "governance.context.memory.ttl_extended",
        "governance.context.memory.scope_expansion_rejected",
        "governance.context.memory.reclassified",
        "governance.context.memory.revoked",
    ] {
        assert!(actions.iter().any(|action| action == expected), "missing audit action {expected}: {actions:?}");
    }

    let leaked_resource_ids = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_log WHERE action LIKE 'governance.context.memory.%' AND resource_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("audit resource id leak count");
    assert_eq!(
        leaked_resource_ids, 0,
        "memory governance audit must not expose item IDs before scoped audit projection"
    );
}
