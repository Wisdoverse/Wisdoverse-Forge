//! Voice endpoints (nested under `/api/v1`).
//!
//! - `GET    /voice/status`              — voice service status
//! - `GET    /voice/providers`           — list voice providers
//! - `POST   /voice/providers`           — add provider
//! - `PUT    /voice/providers/{id}`      — update provider
//! - `DELETE /voice/providers/{id}`      — remove provider
//! - `POST   /voice/providers/{id}/default` — set as default
//! - `POST   /voice/transcribe`          — transcribe audio (stub)

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::repositories::voice::VoiceRepository;
use crate::services::voice::VoiceService;

/// Request body for creating a voice provider.
#[derive(Deserialize)]
pub struct CreateVoiceProviderRequest {
    pub name: String,
    pub provider_type: String,
    #[serde(default = "default_config")]
    pub config: serde_json::Value,
}

fn default_config() -> serde_json::Value {
    serde_json::json!({})
}

/// Request body for updating a voice provider.
#[derive(Deserialize)]
pub struct UpdateVoiceProviderRequest {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub config: Option<serde_json::Value>,
}

/// Build a VoiceService from shared state.
fn make_service(state: &AppState) -> VoiceService {
    VoiceService::new(VoiceRepository::new(state.pool.clone()))
}

/// `GET /voice/status` — voice service status.
async fn voice_status(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let status = service.status(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": status })))
}

/// `GET /voice/providers` — list providers.
async fn list_providers(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let providers = service.list_providers(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": providers })))
}

/// `POST /voice/providers` — add provider.
async fn add_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateVoiceProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let provider = service.add_provider(&auth.scope, &req.name, &req.provider_type, &req.config).await?;
    tracing::info!(org_id = %auth.scope.org_id(), provider = %provider.name, "Voice provider added");
    Ok(Json(serde_json::json!({ "ok": true, "data": provider })))
}

/// `PUT /voice/providers/{id}` — update provider.
async fn update_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateVoiceProviderRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let provider = service
        .update_provider(&auth.scope, id, req.name.as_deref(), req.provider_type.as_deref(), req.config.as_ref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": provider })))
}

/// `DELETE /voice/providers/{id}` — remove provider.
async fn delete_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove_provider(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /voice/providers/{id}/default` — set as default.
async fn set_default_provider(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let provider = service.set_default(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": provider })))
}

/// `POST /voice/transcribe` — transcribe audio (stub).
async fn transcribe(_state: State<AppState>, _auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "ok": true,
        "data": {
            "text": "",
            "message": "Voice transcription not yet implemented"
        }
    })))
}

/// Build voice routes sub-router.
pub fn voice_routes() -> Router<AppState> {
    Router::new()
        .route("/voice/status", get(voice_status))
        .route("/voice/providers", get(list_providers).post(add_provider))
        .route("/voice/providers/{id}", put(update_provider).delete(delete_provider))
        .route("/voice/providers/{id}/default", post(set_default_provider))
        .route("/voice/transcribe", post(transcribe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_voice_provider_request_deserialization() {
        let req: CreateVoiceProviderRequest =
            serde_json::from_str(r#"{"name": "OpenAI TTS", "provider_type": "openai"}"#).unwrap();
        assert_eq!(req.name, "OpenAI TTS");
        assert_eq!(req.provider_type, "openai");
    }

    #[test]
    fn create_voice_provider_with_config() {
        let req: CreateVoiceProviderRequest =
            serde_json::from_str(r#"{"name": "Deepgram", "provider_type": "deepgram", "config": {"model": "nova-2"}}"#)
                .unwrap();
        assert_eq!(req.config["model"], "nova-2");
    }

    #[test]
    fn update_voice_provider_partial() {
        let req: UpdateVoiceProviderRequest = serde_json::from_str(r#"{"name": "Updated Name"}"#).unwrap();
        assert_eq!(req.name.as_deref(), Some("Updated Name"));
        assert!(req.provider_type.is_none());
        assert!(req.config.is_none());
    }
}
