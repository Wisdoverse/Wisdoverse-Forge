//! Unit 2.2 coverage for governed skills.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{mint_test_jwt_with_axes, test_app_with_mock_provider};

struct SkillGovernanceSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    other_workspace_id: Uuid,
    team_id: Uuid,
    project_id: Uuid,
    owner_id: Uuid,
    owner_jwt: String,
    other_workspace_jwt: String,
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

async fn seed_skill_governance(pool: &PgPool) -> SkillGovernanceSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
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
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Other')")
        .bind(other_workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed other workspace");

    for user_id in [owner_id, teammate_id, outsider_id] {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(user_id)
            .bind(if user_id == owner_id { "owner" } else { "member" })
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

    let owner_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(workspace_id), None, None);
    let other_workspace_jwt = mint_test_jwt_with_axes(org_id, owner_id, "owner", Some(other_workspace_id), None, None);
    let teammate_jwt = mint_test_jwt_with_axes(org_id, teammate_id, "member", Some(workspace_id), None, None);
    let outsider_jwt = mint_test_jwt_with_axes(org_id, outsider_id, "member", Some(workspace_id), None, None);

    SkillGovernanceSeed {
        org_id,
        workspace_id,
        other_workspace_id,
        team_id,
        project_id,
        owner_id,
        owner_jwt,
        other_workspace_jwt,
        teammate_jwt,
        outsider_jwt,
    }
}

async fn create_skill(app: Router, jwt: &str, payload: Value) -> (StatusCode, Value) {
    json_request(app, Method::POST, "/api/v1/skills", jwt, Some(payload)).await
}

async fn rerun_skills_governance_migration(pool: &PgPool) {
    sqlx::raw_sql(include_str!("../../db/migrations/047_skills_governance_extension.sql"))
        .execute(pool)
        .await
        .expect("rerun skills governance migration");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_skill_rows_backfill_to_active_org_scope(pool: PgPool) {
    let seed = seed_skill_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, created) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "legacy-review",
            "description": "legacy skill",
            "trigger_pattern": "review",
            "content": "Review carefully"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {created}");
    let legacy_skill_id = Uuid::parse_str(created["data"]["id"].as_str().expect("skill id")).expect("skill uuid");

    sqlx::raw_sql(
        r#"
        ALTER TABLE skills
            DROP CONSTRAINT IF EXISTS skills_state_check,
            DROP CONSTRAINT IF EXISTS skills_version_check,
            DROP CONSTRAINT IF EXISTS skills_sensitivity_check,
            DROP CONSTRAINT IF EXISTS skills_provenance_object_check,
            DROP CONSTRAINT IF EXISTS skills_required_inputs_array_check,
            DROP CONSTRAINT IF EXISTS skills_tools_array_check,
            DROP CONSTRAINT IF EXISTS skills_examples_array_check,
            DROP CONSTRAINT IF EXISTS skills_success_evidence_array_check;
        ALTER TABLE skills
            ALTER COLUMN state DROP NOT NULL,
            ALTER COLUMN version DROP NOT NULL,
            ALTER COLUMN sensitivity DROP NOT NULL,
            ALTER COLUMN provenance DROP NOT NULL,
            ALTER COLUMN required_inputs DROP NOT NULL,
            ALTER COLUMN tools DROP NOT NULL,
            ALTER COLUMN examples DROP NOT NULL,
            ALTER COLUMN success_evidence DROP NOT NULL;
        "#,
    )
    .execute(&pool)
    .await
    .expect("allow simulated governed column drift");
    sqlx::query(
        r#"UPDATE skills
              SET state = NULL,
                  version = NULL,
                  sensitivity = NULL,
                  provenance = '[]'::jsonb,
                  required_inputs = '{}'::jsonb,
                  tools = '{}'::jsonb,
                  examples = '{}'::jsonb,
                  success_evidence = '{}'::jsonb
            WHERE id = $1"#,
    )
    .bind(legacy_skill_id)
    .execute(&pool)
    .await
    .expect("simulate governed column drift");

    rerun_skills_governance_migration(&pool).await;

    let (status, body) = json_request(app, Method::GET, "/api/v1/skills", &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let skill = body["data"]
        .as_array()
        .expect("skills")
        .iter()
        .find(|item| item["id"] == legacy_skill_id.to_string())
        .expect("legacy skill visible");

    assert_eq!(skill["scope_kind"], "org");
    assert_eq!(skill["scope_id"], seed.org_id.to_string());
    assert_eq!(skill["workspace_id"], seed.workspace_id.to_string());
    assert_eq!(skill["state"], "active");
    assert_eq!(skill["version"], 1);
    assert_eq!(skill["owner_user_id"], seed.owner_id.to_string());
    assert_eq!(skill["sensitivity"], "internal");
    assert_eq!(skill["provenance"], json!({}));
    assert_eq!(skill["required_inputs"], json!([]));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn skill_scope_visibility_and_candidate_filtering_are_membership_bound(pool: PgPool) {
    let seed = seed_skill_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, member_org_body) = create_skill(
        app.clone(),
        &seed.teammate_jwt,
        json!({
            "name": "member-org-active",
            "content": "Members must not publish org-wide active skills by default"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {member_org_body}");

    let (status, user_body) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "personal-skill",
            "description": "owner only",
            "content": "Use concise notes",
            "scope_kind": "user"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {user_body}");

    let (status, team_body) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "team-skill",
            "description": "team shared",
            "content": "Use the governed Rust API path",
            "scope_kind": "team",
            "scope_id": seed.team_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {team_body}");

    let (status, candidate_body) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "candidate-skill",
            "content": "Pending approval",
            "scope_kind": "team",
            "scope_id": seed.team_id,
            "state": "candidate"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {candidate_body}");
    let candidate_id = candidate_body["data"]["id"].as_str().expect("candidate id");

    let (status, project_body) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "project-skill",
            "description": "project shared",
            "content": "Project work needs evidence links",
            "scope_kind": "project",
            "scope_id": seed.project_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {project_body}");

    let (status, teammate_list) =
        json_request(app.clone(), Method::GET, "/api/v1/skills", &seed.teammate_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {teammate_list}");
    let teammate_names = teammate_list["data"]
        .as_array()
        .expect("skills")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    assert!(teammate_names.contains(&"team-skill"));
    assert!(teammate_names.contains(&"project-skill"));
    assert!(!teammate_names.contains(&"personal-skill"));
    assert!(!teammate_names.contains(&"candidate-skill"));

    let (status, other_workspace_list) =
        json_request(app.clone(), Method::GET, "/api/v1/skills", &seed.other_workspace_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {other_workspace_list}");
    assert_ne!(seed.workspace_id, seed.other_workspace_id);
    let other_workspace_names = other_workspace_list["data"]
        .as_array()
        .expect("skills")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    assert!(!other_workspace_names.contains(&"team-skill"));
    assert!(!other_workspace_names.contains(&"project-skill"));
    assert!(!other_workspace_names.contains(&"personal-skill"));

    let (status, candidate_get) =
        json_request(app.clone(), Method::GET, format!("/api/v1/skills/{candidate_id}"), &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {candidate_get}");

    let (status, outsider_list) = json_request(app, Method::GET, "/api/v1/skills", &seed.outsider_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {outsider_list}");
    let outsider_names = outsider_list["data"]
        .as_array()
        .expect("skills")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    assert!(!outsider_names.contains(&"team-skill"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn skill_delete_soft_revokes_and_cross_tenant_mutation_is_audited(pool: PgPool) {
    let seed = seed_skill_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;

    let (status, body) = create_skill(
        app.clone(),
        &seed.owner_jwt,
        json!({
            "name": "revocable-skill",
            "content": "Delete should revoke",
            "scope_kind": "org"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let skill_id = body["data"]["id"].as_str().expect("skill id");
    let skill_uuid = Uuid::parse_str(skill_id).expect("skill uuid");

    let (status, revoke_body) =
        json_request(app.clone(), Method::DELETE, format!("/api/v1/skills/{skill_id}"), &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {revoke_body}");

    let (state, enabled): (String, bool) = sqlx::query_as("SELECT state, enabled FROM skills WHERE id = $1")
        .bind(skill_uuid)
        .fetch_one(&pool)
        .await
        .expect("revoked skill retained");
    assert_eq!(state, "revoked");
    assert!(!enabled);

    let (status, list_body) = json_request(app.clone(), Method::GET, "/api/v1/skills", &seed.owner_jwt, None).await;
    assert_eq!(status, StatusCode::OK, "body: {list_body}");
    let names = list_body["data"]
        .as_array()
        .expect("skills")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    assert!(!names.contains(&"revocable-skill"));

    let other_org_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    let other_skill_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(other_org_id)
        .bind(format!("Other Org {other_org_id}"))
        .bind(format!("other-org-{other_org_id}"))
        .execute(&pool)
        .await
        .expect("seed other org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(other_user_id)
        .bind(format!("other-{other_user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("seed other user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(other_org_id)
        .bind(other_user_id)
        .execute(&pool)
        .await
        .expect("seed other membership");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Other')")
        .bind(other_workspace_id)
        .bind(other_org_id)
        .execute(&pool)
        .await
        .expect("seed other workspace");
    sqlx::query(
        "INSERT INTO skills (id, organization_id, workspace_id, scope_kind, scope_id, name, content, owner_user_id)
         VALUES ($1, $2, $3, 'org', $2, 'other-skill', 'other org content', $4)",
    )
    .bind(other_skill_id)
    .bind(other_org_id)
    .bind(other_workspace_id)
    .bind(other_user_id)
    .execute(&pool)
    .await
    .expect("insert other skill");

    let (status, cross_body) = json_request(
        app,
        Method::PATCH,
        format!("/api/v1/skills/{other_skill_id}"),
        &seed.owner_jwt,
        Some(json!({ "name": "takeover" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {cross_body}");

    let actions = sqlx::query_scalar::<_, String>(
        "SELECT action FROM audit_log WHERE action LIKE 'governance.context.skill.%' ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("skill audit actions");
    for expected in [
        "governance.context.skill.created",
        "governance.context.skill.revoked",
        "governance.context.skill.mutation_rejected",
    ] {
        assert!(actions.iter().any(|action| action == expected), "missing audit action {expected}: {actions:?}");
    }

    let tracked_audits = sqlx::query_as::<_, (Option<Uuid>, Value)>(
        "SELECT resource_id, details FROM audit_log WHERE resource_type = 'skill' ORDER BY created_at ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("skill audit details");
    assert!(
        tracked_audits
            .iter()
            .any(|(resource_id, details)| *resource_id == Some(skill_uuid)
                && details["skill_id"] == skill_uuid.to_string()),
        "created/revoked audit must identify the governed skill: {tracked_audits:?}"
    );
    assert!(
        tracked_audits.iter().any(|(resource_id, details)| {
            *resource_id == Some(other_skill_id) && details["attempted_skill_id"] == other_skill_id.to_string()
        }),
        "cross-boundary rejection audit must identify attempted skill id: {tracked_audits:?}"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn secret_skill_content_is_rejected_and_audited(pool: PgPool) {
    let seed = seed_skill_governance(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let secret = assembled(&["AK", "IA", "1234567890ABCDEF"]);

    let (status, body) = create_skill(
        app,
        &seed.owner_jwt,
        json!({
            "name": "secret-skill",
            "content": format!("AWS_ACCESS_KEY_ID={secret}")
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "body: {body}");

    let audit_details = sqlx::query_scalar::<_, Value>(
        "SELECT details FROM audit_log WHERE action = 'governance.context.skill.mutation_rejected'",
    )
    .fetch_one(&pool)
    .await
    .expect("secret rejection audit");
    assert_eq!(audit_details["reason"], "secret_detected");
    assert!(!audit_details.to_string().contains(&secret), "skill rejection audit must not persist raw secret material");
}
