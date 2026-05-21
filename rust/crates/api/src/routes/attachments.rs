//! Attachment endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/attachments`              — list attachments (query: agent_id?)
//! - `POST   /api/v1/attachments`              — multipart upload
//! - `GET    /api/v1/attachments/{id}`         — get metadata
//! - `GET    /api/v1/attachments/{id}/download` — download bytes
//! - `DELETE /api/v1/attachments/{id}`         — delete metadata and object

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult, ErrorKind};

use crate::health::AppState;
use crate::services::attachment::{
    AttachmentAgentScope, AttachmentService, AttachmentUploadDraft, DEFAULT_ATTACHMENT_CONTENT_TYPE,
    attachment_data_response, attachment_delete_response, attachment_download_content_disposition,
};

/// Query parameters for listing attachments.
#[derive(Deserialize)]
pub struct ListAttachmentsQuery {
    pub agent_id: Option<Uuid>,
}

/// Build an AttachmentService from shared state.
fn make_service(state: &AppState) -> AttachmentService {
    AttachmentService::from_pool_and_app_config(state.pool.clone(), state.object_storage.clone(), &state.config)
}

/// `GET /api/v1/attachments` — list attachments.
async fn list_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListAttachmentsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let agent_id = query.agent_id.map(AgentId::from);
    let attachments = service.list(&auth.scope, agent_id).await?;
    Ok(Json(attachment_data_response(attachments)))
}

/// `POST /api/v1/attachments` — upload an attachment.
async fn create_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let upload = parse_upload(multipart).await?;
    let service = make_service(&state);
    let att = service.create_upload(&auth.scope, upload).await?;
    Ok((StatusCode::CREATED, Json(attachment_data_response(att))))
}

/// `GET /api/v1/attachments/{id}` — get attachment metadata.
async fn get_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let att = service.get(&auth.scope, id).await?;
    Ok(Json(attachment_data_response(att)))
}

/// `GET /api/v1/attachments/{id}/download` — download an attachment.
async fn download_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let service = make_service(&state);
    let download = service.download_payload(&auth.scope, id).await?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, content_type_header(download.content_type()));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition_header(download.filename()));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&download.len().to_string()).unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    Ok((StatusCode::OK, headers, download.into_bytes()).into_response())
}

/// `DELETE /api/v1/attachments/{id}` — delete attachment.
async fn delete_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(attachment_delete_response()))
}

async fn parse_upload(mut multipart: Multipart) -> AppResult<AttachmentUploadDraft> {
    let mut file_name: Option<String> = None;
    let mut file_content_type: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut filename_override: Option<String> = None;
    let mut content_type_override: Option<String> = None;
    let mut agent_id: Option<AgentId> = None;

    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        let name = field.name().unwrap_or("").to_string();
        if name.is_empty() {
            return Err(ErrorKind::Validation("multipart field name is required".to_string()).into());
        }

        match name.as_str() {
            "file" => {
                if bytes.is_some() {
                    return Err(ErrorKind::Validation("exactly one file field is allowed".to_string()).into());
                }
                file_name = field.file_name().map(ToString::to_string);
                file_content_type = field.content_type().map(ToString::to_string);
                bytes = Some(field.bytes().await.map_err(multipart_error)?.to_vec());
            }
            "filename" => {
                filename_override = Some(field.text().await.map_err(multipart_error)?);
            }
            "content_type" => {
                content_type_override = Some(field.text().await.map_err(multipart_error)?);
            }
            "agent_id" => {
                let value = field.text().await.map_err(multipart_error)?;
                agent_id = Some(AttachmentAgentScope::parse(&value)?);
            }
            other => {
                return Err(ErrorKind::Validation(format!("unsupported multipart field '{other}'")).into());
            }
        }
    }

    AttachmentUploadDraft::from_parts(
        file_name,
        file_content_type,
        filename_override,
        content_type_override,
        agent_id,
        bytes,
    )
}

fn multipart_error(err: axum::extract::multipart::MultipartError) -> agentforge_core::AppError {
    ErrorKind::Validation(format!("invalid multipart body: {err}")).into()
}

fn content_type_header(content_type: &str) -> HeaderValue {
    HeaderValue::from_str(content_type).unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_ATTACHMENT_CONTENT_TYPE))
}

fn content_disposition_header(filename: &str) -> HeaderValue {
    let value = attachment_download_content_disposition(filename);
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// Build attachment routes sub-router.
pub fn attachment_routes(max_upload_body_bytes: usize) -> Router<AppState> {
    Router::new()
        .route("/attachments", get(list_attachments).post(create_attachment))
        // Static route BEFORE parameterized (per CLAUDE.md)
        .route("/attachments/{id}/download", get(download_attachment))
        .route("/attachments/{id}", get(get_attachment).delete(delete_attachment))
        .layer(DefaultBodyLimit::max(max_upload_body_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_deserialization() {
        let query: ListAttachmentsQuery =
            serde_json::from_str(r#"{"agent_id": "550e8400-e29b-41d4-a716-446655440000"}"#).unwrap();
        assert!(query.agent_id.is_some());
    }

    #[test]
    fn list_query_empty() {
        let query: ListAttachmentsQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert!(query.agent_id.is_none());
    }

    #[test]
    fn parse_agent_id_rejects_non_uuid() {
        assert!(AttachmentAgentScope::parse("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(AttachmentAgentScope::parse("not-a-uuid").is_err());
    }
}
