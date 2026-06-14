//! Navigation routes — the frontend's tree-pane contract.
//!
//! URL shapes predate the Rust cutover (the "legacy nav" in prior plans);
//! the route surface stays stable while persistence, validation, and response
//! projection live in repository, service, and domain modules.

use axum::extract::{Path, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::legacy_navigation::{
    LegacyNavigationService, legacy_delete_response, legacy_org_response, legacy_orgs_response,
    legacy_project_response, legacy_projects_response, legacy_team_response, legacy_teams_response,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyOrgUpdateRequest {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTeamCreateRequest {
    name: String,
    slug: Option<String>,
    visibility: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTeamUpdateRequest {
    name: Option<String>,
    slug: Option<String>,
    visibility: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProjectCreateRequest {
    name: String,
    slug: Option<String>,
    color: Option<String>,
    description: Option<String>,
    /// Optional git repository to clone into the new project's workspace dir.
    /// When present, the create transaction also enqueues the first clone
    /// attempt via the transactional outbox (M2).
    repository_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProjectUpdateRequest {
    name: Option<String>,
    slug: Option<String>,
    color: Option<String>,
    description: Option<String>,
}

fn make_service(state: &AppState) -> LegacyNavigationService {
    state.legacy_navigation_service()
}

async fn list_orgs(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let orgs = make_service(&state).list_orgs(&auth.scope).await?;
    Ok(Json(legacy_orgs_response(orgs)))
}

async fn get_org(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let org = make_service(&state).get_org(&auth.scope, org_id).await?;
    Ok(Json(legacy_org_response(org)))
}

async fn update_org(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Json(req): Json<LegacyOrgUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let org = make_service(&state).update_org(&auth.scope, org_id, req.name).await?;
    Ok(Json(legacy_org_response(org)))
}

async fn list_teams(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let teams = make_service(&state).list_teams(&auth.scope, org_id).await?;
    Ok(Json(legacy_teams_response(teams)))
}

async fn create_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Json(req): Json<LegacyTeamCreateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let team = make_service(&state)
        .create_team(&auth.scope, org_id, req.name, req.slug, req.visibility, req.description)
        .await?;
    Ok(Json(legacy_team_response(team)))
}

async fn update_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<LegacyTeamUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let team = make_service(&state)
        .update_team(&auth.scope, org_id, team_id, req.name, req.slug, req.visibility, req.description)
        .await?;
    Ok(Json(legacy_team_response(team)))
}

async fn delete_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).delete_team(&auth.scope, org_id, team_id).await?;
    Ok(Json(legacy_delete_response()))
}

async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let projects = make_service(&state).list_projects(&auth.scope, team_id).await?;
    Ok(Json(legacy_projects_response(projects)))
}

async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    Json(req): Json<LegacyProjectCreateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let project = make_service(&state)
        .create_project(&auth.scope, team_id, req.name, req.slug, req.color, req.description, req.repository_url)
        .await?;
    Ok(Json(legacy_project_response(project)))
}

async fn update_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<LegacyProjectUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let project = make_service(&state)
        .update_project(&auth.scope, team_id, project_id, req.name, req.slug, req.color, req.description)
        .await?;
    Ok(Json(legacy_project_response(project)))
}

async fn delete_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).delete_project(&auth.scope, team_id, project_id).await?;
    Ok(Json(legacy_delete_response()))
}

pub fn legacy_navigation_routes() -> Router<AppState> {
    Router::new()
        .route("/orgs", get(list_orgs))
        .route("/orgs/{id}", get(get_org).patch(update_org))
        .route("/orgs/{org_id}/teams", get(list_teams).post(create_team))
        .route("/orgs/{org_id}/teams/{team_id}", patch(update_team).delete(delete_team))
        .route("/teams/{team_id}/projects", get(list_projects).post(create_project))
        .route("/teams/{team_id}/projects/{project_id}", patch(update_project).delete(delete_project))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_create_request_accepts_navigation_shape() {
        let req: LegacyTeamCreateRequest = serde_json::from_str(
            r#"{"name":"Engineering","slug":"engineering","visibility":"open","description":"Builds product"}"#,
        )
        .unwrap();

        assert_eq!(req.name, "Engineering");
        assert_eq!(req.slug.as_deref(), Some("engineering"));
        assert_eq!(req.visibility.as_deref(), Some("open"));
    }

    #[test]
    fn team_update_request_accepts_partial_navigation_shape() {
        let req: LegacyTeamUpdateRequest =
            serde_json::from_str(r#"{"name":"Platform","description":"Runs the platform"}"#).unwrap();

        assert_eq!(req.name.as_deref(), Some("Platform"));
        assert_eq!(req.description.as_deref(), Some("Runs the platform"));
        assert!(req.slug.is_none());
    }

    #[test]
    fn project_create_request_accepts_navigation_shape() {
        let req: LegacyProjectCreateRequest = serde_json::from_str(
            r##"{"name":"Wisdoverse Forge","slug":"agentforge","color":"#007AFF","description":"Control plane"}"##,
        )
        .unwrap();

        assert_eq!(req.name, "Wisdoverse Forge");
        assert_eq!(req.slug.as_deref(), Some("agentforge"));
        assert_eq!(req.color.as_deref(), Some("#007AFF"));
    }

    #[test]
    fn project_update_request_accepts_partial_navigation_shape() {
        let req: LegacyProjectUpdateRequest =
            serde_json::from_str(r##"{"name":"Forge","color":"#6366f1","description":"Workbench"}"##).unwrap();

        assert_eq!(req.name.as_deref(), Some("Forge"));
        assert_eq!(req.color.as_deref(), Some("#6366f1"));
        assert_eq!(req.description.as_deref(), Some("Workbench"));
        assert!(req.slug.is_none());
    }
}

/// Test-only constructors for the legacy DTOs. The structs themselves keep
/// private fields so callers outside this module can't synthesize a response by
/// accident. The `testing` module uses these helpers to build shape fixtures for
/// contract tests.
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub mod test_only {
    use agentforge_core::{AppResult, TeamId};
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::domain::navigation::{LegacyOrg, LegacyProject, LegacyTeam};
    use crate::repositories::resource::navigation::LegacyNavigationRepository;
    use crate::test_support::tenant_scope_for_ids;

    pub fn sample_org(role: &str, plan: &str) -> serde_json::Value {
        serde_json::to_value(LegacyOrg {
            id: Uuid::nil(),
            name: "Example Org".to_string(),
            slug: "example-org".to_string(),
            plan: plan.to_string(),
            role: role.to_string(),
        })
        .expect("LegacyOrg serializes")
    }

    pub fn sample_team() -> serde_json::Value {
        serde_json::to_value(LegacyTeam {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            name: "Engineering".to_string(),
            slug: "engineering".to_string(),
            visibility: "private".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
            can_create_project: true,
        })
        .expect("LegacyTeam serializes")
    }

    pub fn sample_project() -> serde_json::Value {
        serde_json::to_value(LegacyProject {
            id: Uuid::nil(),
            team_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            name: "Wisdoverse Forge".to_string(),
            slug: "agentforge".to_string(),
            color: "#007AFF".to_string(),
            description: String::new(),
            can_manage: true,
            can_delete: true,
        })
        .expect("LegacyProject serializes")
    }

    pub async fn list_teams_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
    ) -> AppResult<Vec<serde_json::Value>> {
        let scope = tenant_scope_for_ids(org_id, user_id);
        let repo = LegacyNavigationRepository::new(pool.clone());
        let rows = repo.list_teams(&scope, org_id).await?;
        Ok(rows
            .into_iter()
            .map(LegacyTeam::from)
            .map(|row| serde_json::to_value(row).expect("LegacyTeam serializes"))
            .collect())
    }

    pub async fn list_projects_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        team_id: Uuid,
    ) -> AppResult<Vec<serde_json::Value>> {
        let repo = LegacyNavigationRepository::new(pool.clone());
        let org_id = repo.resolve_team_org_for_test(user_id, team_id).await?;
        let scope = tenant_scope_for_ids(org_id, user_id);
        let rows = repo.list_projects(&scope, TeamId::from(team_id)).await?;
        Ok(rows
            .into_iter()
            .map(LegacyProject::from)
            .map(|row| serde_json::to_value(row).expect("LegacyProject serializes"))
            .collect())
    }

    /// Drive the active legacy-navigation create surface through its service so
    /// integration tests can exercise the transactional create path
    /// (`/teams/:teamId/projects`) without an HTTP client. Returns the project id.
    pub async fn create_project_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        team_id: Uuid,
        name: &str,
        repository_url: Option<&str>,
    ) -> AppResult<Uuid> {
        create_project_with_slug_for_test(pool, user_id, team_id, name, None, repository_url).await
    }

    /// Like [`create_project_canonical_for_test`] but lets a test pass an
    /// explicit caller `slug`, so it can prove the legacy create path DISCARDS a
    /// hostile caller slug (e.g. `../../etc`): the on-disk `workspace_dir_name` is
    /// derived from the NAME alone, never from the caller's slug. Returns the new
    /// project id; the test reads back `workspace_dir_name` to assert the slug had
    /// no influence.
    pub async fn create_project_with_slug_for_test(
        pool: &PgPool,
        user_id: Uuid,
        team_id: Uuid,
        name: &str,
        slug: Option<&str>,
        repository_url: Option<&str>,
    ) -> AppResult<Uuid> {
        let repo = LegacyNavigationRepository::new(pool.clone());
        let org_id = repo.resolve_team_org_for_test(user_id, team_id).await?;
        let scope = tenant_scope_for_ids(org_id, user_id);
        let service = crate::services::legacy_navigation::LegacyNavigationService::from_pool(pool.clone());
        let project = service
            .create_project(
                &scope,
                team_id,
                name.to_string(),
                slug.map(|s| s.to_string()),
                None,
                None,
                repository_url.map(|u| u.to_string()),
            )
            .await?;
        Ok(project.id)
    }
}
