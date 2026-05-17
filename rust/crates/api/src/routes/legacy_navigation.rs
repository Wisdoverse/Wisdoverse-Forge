//! Navigation routes — the frontend's tree-pane contract.
//!
//! URL shapes predate the Rust cutover (the "legacy nav" in prior plans);
//! P4 kept the URL surface stable and deleted the dual-read machinery.
//! Every handler here now reads from `public.*` only.
//!
//! - `GET /api/v1/orgs`
//! - `GET /api/v1/orgs/{orgId}`
//! - `PATCH /api/v1/orgs/{orgId}`
//! - `GET    /api/v1/orgs/{orgId}/teams`
//! - `POST   /api/v1/orgs/{orgId}/teams`
//! - `PATCH  /api/v1/orgs/{orgId}/teams/{teamId}`
//! - `DELETE /api/v1/orgs/{orgId}/teams/{teamId}`
//! - `GET    /api/v1/teams/{teamId}/projects`
//! - `POST   /api/v1/teams/{teamId}/projects`
//! - `PATCH  /api/v1/teams/{teamId}/projects/{projectId}`
//! - `DELETE /api/v1/teams/{teamId}/projects/{projectId}`
//! - `GET /api/v1/groups?projectId=...` (served from `routes/groups.rs`)
//!
//! File name is historical; renaming requires touching every `mod` /
//! test reference, which is out of scope for P4 cleanup.

use axum::extract::{Path, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind, OrgId, ProjectId, TeamId};

use crate::domain::resource::NavigationResourcePolicy;
use crate::health::AppState;
use crate::repositories::group::GroupRepository;
use crate::repositories::organization::OrganizationRepository;
use crate::repositories::resource_permission::ResourcePermissionRepository;
use crate::services::group::GroupService;
use crate::services::organization::OrganizationService;
use crate::services::resource_permission::ResourcePermissionService;

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct LegacyOrg {
    id: Uuid,
    name: String,
    slug: String,
    plan: String,
    role: String,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct LegacyTeam {
    id: Uuid,
    org_id: Uuid,
    name: String,
    slug: String,
    visibility: String,
    description: String,
    can_manage: bool,
    can_delete: bool,
    can_create_project: bool,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct LegacyProject {
    id: Uuid,
    team_id: Uuid,
    workspace_id: Uuid,
    name: String,
    slug: String,
    color: String,
    description: String,
    can_manage: bool,
    can_delete: bool,
}

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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProjectUpdateRequest {
    name: Option<String>,
    slug: Option<String>,
    color: Option<String>,
    description: Option<String>,
}

fn make_org_service(state: &AppState) -> OrganizationService {
    OrganizationService::new(OrganizationRepository::new(state.pool.clone()))
}

fn make_permission_service(state: &AppState) -> ResourcePermissionService {
    ResourcePermissionService::new(ResourcePermissionRepository::new(state.pool.clone()))
}

async fn list_orgs(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    // Three-tier sort mirroring `UserRepository::find_default_org`:
    //   1. Canonical org whose `email_domain` matches the user's email domain.
    //   2. Any other canonical org (`email_domain IS NOT NULL`).
    //   3. Personal Space (`email_domain IS NULL`).
    // Ties broken by earliest `created_at`.
    let orgs = sqlx::query_as::<_, LegacyOrg>(
        r#"SELECT
               o.id,
               o.name,
               o.slug,
               COALESCE(o.plan, 'free') AS plan,
               om.role
           FROM organizations o
           JOIN organization_members om
             ON om.organization_id = o.id
           JOIN users u
             ON u.id = om.user_id
          WHERE om.user_id = $1
            AND o.deleted_at IS NULL
          ORDER BY
            (o.email_domain IS DISTINCT FROM lower(split_part(u.email, '@', 2))),
            (o.email_domain IS NULL),
            om.created_at ASC,
            o.created_at ASC"#,
    )
    .bind(auth.scope.user_id().as_uuid())
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({ "ok": true, "orgs": orgs })))
}

async fn get_org(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let org = fetch_org_for_user(&state.pool, auth.scope.user_id().as_uuid(), org_id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "org": org })))
}

async fn update_org(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Json(req): Json<LegacyOrgUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let Some(name) = req.name.as_deref() else {
        return Err(ErrorKind::Validation("name is required".into()).into());
    };

    let service = make_org_service(&state);
    service.update(&auth.scope, OrgId::from(org_id), name).await?;

    let org = fetch_org_for_user(&state.pool, auth.scope.user_id().as_uuid(), org_id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "org": org })))
}

async fn list_teams(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let teams = list_teams_canonical(&state.pool, auth.scope.user_id().as_uuid(), org_id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "teams": teams })))
}

async fn create_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    Json(req): Json<LegacyTeamCreateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let draft = NavigationResourcePolicy::team_create_draft(req.name, req.slug, req.visibility, req.description)?;
    make_permission_service(&state).require_org_manager(&auth.scope).await?;

    let team = sqlx::query_as::<_, LegacyTeam>(
        r#"INSERT INTO public.teams (organization_id, name, slug, visibility, description)
           SELECT o.id, $4, $5, COALESCE($6::text, 'private'), COALESCE($7::text, '')
             FROM public.organizations o
             JOIN organization_members om
               ON om.organization_id = o.id
            WHERE o.id = $1
              AND o.id = $2
              AND om.user_id = $3
              AND o.deleted_at IS NULL
            RETURNING
              id,
              organization_id AS org_id,
              name,
              slug,
              COALESCE(visibility, 'private') AS visibility,
              COALESCE(description, '')       AS description,
              TRUE AS can_manage,
              TRUE AS can_delete,
              TRUE AS can_create_project"#,
    )
    .bind(org_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .bind(draft.name)
    .bind(draft.slug)
    .bind(draft.visibility)
    .bind(draft.description)
    .fetch_optional(&state.pool)
    .await?;
    let Some(team) = team else {
        return Err(ErrorKind::NotFound(format!("organization {org_id}")).into());
    };

    Ok(Json(serde_json::json!({ "ok": true, "team": team })))
}

async fn update_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<LegacyTeamUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let draft = NavigationResourcePolicy::team_update_draft(req.name, req.slug, req.visibility, req.description)?;
    make_permission_service(&state).require_team_manager(&auth.scope, TeamId::from(team_id)).await?;

    let team = sqlx::query_as::<_, LegacyTeam>(
        r#"UPDATE public.teams t
              SET name = COALESCE($5, t.name),
                  slug = COALESCE($6, t.slug),
                  visibility = COALESCE($7, t.visibility),
                  description = COALESCE($8, t.description),
                  updated_at = NOW()
             FROM organization_members om
            WHERE t.id = $1
              AND t.organization_id = $2
              AND t.organization_id = $3
              AND om.organization_id = t.organization_id
              AND om.user_id = $4
              AND t.deleted_at IS NULL
            RETURNING
              t.id,
              t.organization_id AS org_id,
              t.name,
              t.slug,
              COALESCE(t.visibility, 'private') AS visibility,
              COALESCE(t.description, '')       AS description,
              TRUE AS can_manage,
              TRUE AS can_delete,
              TRUE AS can_create_project"#,
    )
    .bind(team_id)
    .bind(org_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .bind(draft.name)
    .bind(draft.slug)
    .bind(draft.visibility)
    .bind(draft.description)
    .fetch_optional(&state.pool)
    .await?;
    let Some(team) = team else {
        return Err(ErrorKind::NotFound(format!("team {team_id}")).into());
    };

    Ok(Json(serde_json::json!({ "ok": true, "team": team })))
}

async fn delete_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_permission_service(&state).require_team_manager(&auth.scope, TeamId::from(team_id)).await?;

    let result = sqlx::query(
        r#"UPDATE public.teams t
              SET deleted_at = NOW(),
                  updated_at = NOW()
             FROM organization_members om
            WHERE t.id = $1
              AND t.organization_id = $2
              AND t.organization_id = $3
              AND om.organization_id = t.organization_id
              AND om.user_id = $4
              AND t.deleted_at IS NULL"#,
    )
    .bind(team_id)
    .bind(org_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ErrorKind::NotFound(format!("team {team_id}")).into());
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let projects = list_projects_canonical(&state.pool, auth.scope.user_id().as_uuid(), team_id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "projects": projects })))
}

async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    Json(req): Json<LegacyProjectCreateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let draft = NavigationResourcePolicy::project_create_draft(req.name, req.slug, req.color, req.description)?;

    let org_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT t.organization_id
             FROM public.teams t
             JOIN organization_members om
               ON om.organization_id = t.organization_id
            WHERE t.id = $1
              AND t.organization_id = $2
              AND om.user_id = $3
              AND t.deleted_at IS NULL"#,
    )
    .bind(team_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .fetch_optional(&state.pool)
    .await?;
    let Some(org_id) = org_id else {
        return Err(ErrorKind::NotFound(format!("team {team_id}")).into());
    };
    make_permission_service(&state).require_project_creator(&auth.scope, TeamId::from(team_id)).await?;

    let workspace_id = default_workspace_for_org(&state.pool, org_id).await?;
    let project = sqlx::query_as::<_, LegacyProject>(
        r#"INSERT INTO public.projects (
               organization_id,
               workspace_id,
               team_id,
               name,
               slug,
               color,
               description
           )
           VALUES ($1, $2, $3, $4, $5, COALESCE($6::text, '#007AFF'), COALESCE($7::text, ''))
           RETURNING
             id,
             workspace_id,
             team_id,
             name,
             slug,
             COALESCE(color, '#007AFF') AS color,
             COALESCE(description, '')  AS description,
             TRUE AS can_manage,
             TRUE AS can_delete"#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .bind(draft.name)
    .bind(draft.slug)
    .bind(draft.color)
    .bind(draft.description)
    .fetch_one(&state.pool)
    .await?;

    let group_service = GroupService::new(GroupRepository::new(state.pool.clone()));
    group_service.find_or_create_default_for_project(&auth.scope, ProjectId::from(project.id)).await?;

    Ok(Json(serde_json::json!({ "ok": true, "project": project })))
}

async fn update_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<LegacyProjectUpdateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let draft = NavigationResourcePolicy::project_update_draft(req.name, req.slug, req.color, req.description)?;
    make_permission_service(&state).require_project_manager(&auth.scope, ProjectId::from(project_id)).await?;

    let project = sqlx::query_as::<_, LegacyProject>(
        r#"UPDATE public.projects p
              SET name = COALESCE($5, p.name),
                  slug = COALESCE($6, p.slug),
                  color = COALESCE($7, p.color),
                  description = COALESCE($8, p.description),
                  updated_at = NOW()
             FROM public.teams t
             JOIN organization_members om
               ON om.organization_id = t.organization_id
            WHERE p.id = $1
              AND p.team_id = $2
              AND p.team_id = t.id
              AND t.organization_id = $3
              AND om.user_id = $4
              AND p.deleted_at IS NULL
              AND t.deleted_at IS NULL
            RETURNING
              p.id,
              p.workspace_id,
              p.team_id,
              p.name,
              p.slug,
              COALESCE(p.color, '#007AFF') AS color,
              COALESCE(p.description, '')  AS description,
              TRUE AS can_manage,
              TRUE AS can_delete"#,
    )
    .bind(project_id)
    .bind(team_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .bind(draft.name)
    .bind(draft.slug)
    .bind(draft.color)
    .bind(draft.description)
    .fetch_optional(&state.pool)
    .await?;
    let Some(project) = project else {
        return Err(ErrorKind::NotFound(format!("project {project_id}")).into());
    };

    Ok(Json(serde_json::json!({ "ok": true, "project": project })))
}

async fn delete_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_permission_service(&state).require_project_manager(&auth.scope, ProjectId::from(project_id)).await?;

    let result = sqlx::query(
        r#"UPDATE public.projects p
              SET deleted_at = NOW(),
                  updated_at = NOW()
             FROM public.teams t
             JOIN organization_members om
               ON om.organization_id = t.organization_id
            WHERE p.id = $1
              AND p.team_id = $2
              AND p.team_id = t.id
              AND t.organization_id = $3
              AND om.user_id = $4
              AND p.deleted_at IS NULL
              AND t.deleted_at IS NULL"#,
    )
    .bind(project_id)
    .bind(team_id)
    .bind(auth.scope.org_id().as_uuid())
    .bind(auth.scope.user_id().as_uuid())
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ErrorKind::NotFound(format!("project {project_id}")).into());
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Canonical (`public.teams`) read for `GET /orgs/{orgId}/teams`.
///
/// Migration 026 made `slug` NOT NULL; migration 027 dropped the legacy
/// fallback. `visibility` and `description` stay nullable (ADR 0001),
/// so their `COALESCE` wrappers remain.
async fn list_teams_canonical(pool: &PgPool, user_id: Uuid, org_id: Uuid) -> AppResult<Vec<LegacyTeam>> {
    sqlx::query_as::<_, LegacyTeam>(
        r#"SELECT
               t.id,
               t.organization_id        AS org_id,
               t.name,
               t.slug,
               COALESCE(t.visibility, 'private') AS visibility,
               COALESCE(t.description, '')       AS description,
               (
                   om.role IN ('owner', 'admin')
                   OR EXISTS (
                       SELECT 1
                         FROM public.team_members tm
                        WHERE tm.team_id = t.id
                          AND tm.user_id = $2
                          AND tm.role IN ('owner', 'admin')
                   )
               ) AS can_manage,
               (
                   om.role IN ('owner', 'admin')
                   OR EXISTS (
                       SELECT 1
                         FROM public.team_members tm
                        WHERE tm.team_id = t.id
                          AND tm.user_id = $2
                          AND tm.role IN ('owner', 'admin')
                   )
               ) AS can_delete,
               (
                   om.role IN ('owner', 'admin')
                   OR EXISTS (
                       SELECT 1
                         FROM public.team_members tm
                        WHERE tm.team_id = t.id
                          AND tm.user_id = $2
                          AND tm.role IN ('owner', 'admin', 'maintainer')
                   )
               ) AS can_create_project
           FROM public.teams t
           JOIN organization_members om
             ON om.organization_id = t.organization_id
          WHERE t.organization_id = $1
            AND om.user_id = $2
            AND t.deleted_at IS NULL
          ORDER BY t.created_at ASC"#,
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

/// Canonical (`public.projects`) read for `GET /teams/{teamId}/projects`.
///
/// `team_id` + `slug` are NOT NULL after migration 026; `color` and
/// `description` stay nullable so their `COALESCE` wrappers remain.
async fn list_projects_canonical(pool: &PgPool, user_id: Uuid, team_id: Uuid) -> AppResult<Vec<LegacyProject>> {
    sqlx::query_as::<_, LegacyProject>(
        r#"SELECT
               p.id,
               p.workspace_id,
               p.team_id,
               p.name,
               p.slug,
               COALESCE(p.color, '#007AFF')  AS color,
               COALESCE(p.description, '')   AS description,
               (
                   om.role IN ('owner', 'admin')
                   OR EXISTS (
                       SELECT 1
                         FROM public.team_members tm
                        WHERE tm.team_id = t.id
                          AND tm.user_id = $2
                          AND tm.role IN ('owner', 'admin', 'maintainer')
                   )
                   OR EXISTS (
                       SELECT 1
                         FROM public.project_members pm
                        WHERE pm.project_id = p.id
                          AND pm.user_id = $2
                          AND pm.role IN ('owner', 'admin', 'maintainer')
                   )
               ) AS can_manage,
               (
                   om.role IN ('owner', 'admin')
                   OR EXISTS (
                       SELECT 1
                         FROM public.team_members tm
                        WHERE tm.team_id = t.id
                          AND tm.user_id = $2
                          AND tm.role IN ('owner', 'admin', 'maintainer')
                   )
                   OR EXISTS (
                       SELECT 1
                         FROM public.project_members pm
                        WHERE pm.project_id = p.id
                          AND pm.user_id = $2
                          AND pm.role IN ('owner', 'admin', 'maintainer')
                   )
               ) AS can_delete
           FROM public.projects p
           JOIN public.teams t
             ON t.id = p.team_id
           JOIN organization_members om
             ON om.organization_id = t.organization_id
          WHERE p.team_id = $1
            AND om.user_id = $2
            AND p.deleted_at IS NULL
            AND t.deleted_at IS NULL
          ORDER BY p.created_at ASC"#,
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

async fn fetch_org_for_user(pool: &PgPool, user_id: Uuid, org_id: Uuid) -> AppResult<LegacyOrg> {
    sqlx::query_as::<_, LegacyOrg>(
        r#"SELECT
               o.id,
               o.name,
               o.slug,
               COALESCE(o.plan, 'free') AS plan,
               om.role
           FROM organizations o
           JOIN organization_members om
             ON om.organization_id = o.id
          WHERE o.id = $1
            AND om.user_id = $2
            AND o.deleted_at IS NULL
          LIMIT 1"#,
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ErrorKind::NotFound(format!("organization {org_id}")).into())
}

async fn default_workspace_for_org(pool: &PgPool, org_id: Uuid) -> AppResult<Uuid> {
    if let Some(workspace_id) = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id
             FROM public.workspaces
            WHERE organization_id = $1
              AND deleted_at IS NULL
            ORDER BY created_at ASC
            LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(workspace_id);
    }

    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO public.workspaces (organization_id, name)
           VALUES ($1, 'Default Workspace')
           RETURNING id"#,
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
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
    fn legacy_org_serializes_old_frontend_shape() {
        let value = serde_json::to_value(LegacyOrg {
            id: Uuid::nil(),
            name: "Test Org".to_string(),
            slug: "test-org".to_string(),
            plan: "pro".to_string(),
            role: "owner".to_string(),
        })
        .unwrap();

        assert_eq!(value["name"], "Test Org");
        assert_eq!(value["plan"], "pro");
        assert_eq!(value["role"], "owner");
    }

    #[test]
    fn legacy_team_uses_camel_case_org_id() {
        let value = serde_json::to_value(LegacyTeam {
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
        .unwrap();

        assert!(value.get("orgId").is_some());
        assert_eq!(value["slug"], "engineering");
        assert_eq!(value["canManage"], true);
        assert_eq!(value["canDelete"], true);
        assert_eq!(value["canCreateProject"], true);
    }

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
    fn legacy_project_uses_camel_case_team_id() {
        let value = serde_json::to_value(LegacyProject {
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
        .unwrap();

        assert!(value.get("teamId").is_some());
        assert!(value.get("workspaceId").is_some());
        assert_eq!(value["color"], "#007AFF");
        assert_eq!(value["canManage"], true);
        assert_eq!(value["canDelete"], true);
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
/// private fields so callers outside this module can't synthesize a
/// response by accident. The `testing` module uses these helpers to build
/// shape fixtures for contract tests.
#[doc(hidden)]
pub mod test_only {
    use super::{AppResult, LegacyOrg, LegacyProject, LegacyTeam, PgPool};
    use uuid::Uuid;

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
        let rows = super::list_teams_canonical(pool, user_id, org_id).await?;
        Ok(rows.iter().map(|r| serde_json::to_value(r).expect("LegacyTeam serializes")).collect())
    }

    pub async fn list_projects_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        team_id: Uuid,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = super::list_projects_canonical(pool, user_id, team_id).await?;
        Ok(rows.iter().map(|r| serde_json::to_value(r).expect("LegacyProject serializes")).collect())
    }
}
