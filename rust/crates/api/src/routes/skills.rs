//! Skill endpoints (nested under `/api/v1`).
//!
//! - `GET    /skills`                     — list skills
//! - `POST   /skills`                     — create skill
//! - `GET    /skills/{id}`                — get skill
//! - `PATCH  /skills/{id}`                — update skill
//! - `DELETE /skills/{id}`                — delete skill
//! - `GET    /skills/{id}/versions`       — list skill versions
//! - `POST   /skills/{id}/restore-version` — restore a version

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::domain::skill::{SkillScopeKind, SkillState};
use crate::health::AppState;
use crate::services::skill::{
    CreateSkillInput, RestoreSkillVersionInput, SkillService, UpdateSkillInput, skill_data_response,
    skill_delete_response,
};

/// Request body for creating a skill.
#[derive(Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub description: Option<String>,
    pub trigger_pattern: Option<String>,
    pub negative_trigger: Option<String>,
    pub content: String,
    pub scope_kind: Option<SkillScopeKind>,
    pub scope_id: Option<Uuid>,
    pub state: Option<SkillState>,
    pub sensitivity: Option<String>,
    pub provenance: Option<serde_json::Value>,
    pub required_inputs: Option<serde_json::Value>,
    pub tools: Option<serde_json::Value>,
    pub examples: Option<serde_json::Value>,
    pub success_evidence: Option<serde_json::Value>,
    pub ttl_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Request body for updating a skill.
#[derive(Deserialize)]
pub struct UpdateSkillRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub trigger_pattern: Option<String>,
    pub content: Option<String>,
    pub enabled: Option<bool>,
}

/// Request body for restoring a historical skill version.
#[derive(Deserialize)]
pub struct RestoreSkillVersionRequest {
    pub version: i32,
    pub expected_current_version: Option<i32>,
    #[serde(default)]
    pub confirm_expansion: bool,
}

/// Build a SkillService from shared state.
fn make_service(state: &AppState) -> SkillService {
    state.skill_service()
}

/// `GET /skills` — list skills.
async fn list_skills(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let skills = service.list(&auth.scope).await?;
    Ok(Json(skill_data_response(skills)))
}

/// `POST /skills` — create a skill.
async fn create_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateSkillRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let skill = service
        .create(
            &auth.scope,
            CreateSkillInput {
                name: req.name,
                description: req.description,
                trigger_pattern: req.trigger_pattern,
                negative_trigger: req.negative_trigger,
                content: req.content,
                scope_kind: req.scope_kind.unwrap_or(SkillScopeKind::Org),
                scope_id: req.scope_id,
                state: req.state,
                sensitivity: req.sensitivity,
                provenance: req.provenance,
                required_inputs: req.required_inputs,
                tools: req.tools,
                examples: req.examples,
                success_evidence: req.success_evidence,
                ttl_expires_at: req.ttl_expires_at,
            },
        )
        .await?;
    tracing::info!(org_id = %auth.scope.org_id(), skill = %skill.name, "Skill created");
    Ok(Json(skill_data_response(skill)))
}

/// `GET /skills/{id}` — get a skill.
async fn get_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let skill = service.get(&auth.scope, id).await?;
    Ok(Json(skill_data_response(skill)))
}

/// `PATCH /skills/{id}` — update a skill.
async fn update_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSkillRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let skill = service
        .update(
            &auth.scope,
            id,
            UpdateSkillInput {
                name: req.name,
                description: req.description,
                trigger_pattern: req.trigger_pattern,
                content: req.content,
                enabled: req.enabled,
            },
        )
        .await?;
    Ok(Json(skill_data_response(skill)))
}

/// `DELETE /skills/{id}` — delete a skill.
async fn delete_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(skill_delete_response()))
}

/// `GET /skills/{id}/versions` — list version snapshots for a skill.
async fn list_skill_versions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let versions = service.list_versions(&auth.scope, id).await?;
    Ok(Json(skill_data_response(versions)))
}

/// `POST /skills/{id}/restore-version` — restore a skill version.
async fn restore_skill_version(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RestoreSkillVersionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let skill = service
        .restore_version(
            &auth.scope,
            id,
            RestoreSkillVersionInput {
                version: req.version,
                expected_current_version: req.expected_current_version,
                confirm_expansion: req.confirm_expansion,
            },
        )
        .await?;
    Ok(Json(skill_data_response(skill)))
}

/// Build skill routes sub-router.
pub fn skill_routes() -> Router<AppState> {
    Router::new()
        .route("/skills", get(list_skills).post(create_skill))
        .route("/skills/{id}/versions", get(list_skill_versions))
        .route("/skills/{id}/restore-version", post(restore_skill_version))
        .route("/skills/{id}", get(get_skill).patch(update_skill).delete(delete_skill))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_skill_request_deserialization() {
        let req: CreateSkillRequest =
            serde_json::from_str(r#"{"name": "review", "description": "Code review", "content": "Review this code"}"#)
                .unwrap();
        assert_eq!(req.name, "review");
        assert_eq!(req.content, "Review this code");
        assert!(req.trigger_pattern.is_none());
    }

    #[test]
    fn create_skill_request_with_trigger() {
        let req: CreateSkillRequest =
            serde_json::from_str(r#"{"name": "greet", "trigger_pattern": "^hello", "content": "Say hello back"}"#)
                .unwrap();
        assert_eq!(req.trigger_pattern.as_deref(), Some("^hello"));
    }

    #[test]
    fn update_skill_request_partial() {
        let req: UpdateSkillRequest = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert_eq!(req.enabled, Some(false));
        assert!(req.name.is_none());
        assert!(req.content.is_none());
    }

    #[test]
    fn restore_skill_version_request_deserialization() {
        let req: RestoreSkillVersionRequest =
            serde_json::from_str(r#"{"version": 2, "expected_current_version": 4}"#).unwrap();
        assert_eq!(req.version, 2);
        assert_eq!(req.expected_current_version, Some(4));
        assert!(!req.confirm_expansion);
    }
}
