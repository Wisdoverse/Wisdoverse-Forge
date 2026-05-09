//! Attachment service — validation and management.

use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, AttachmentId, ErrorKind, TenantScope};
use agentforge_db::entities::Attachment;
use agentforge_infra::ObjectStorageClient;
use uuid::Uuid;

use crate::repositories::attachment::{AttachmentRepository, NewAttachment};

/// Maximum filename length.
const MAX_FILENAME_LEN: usize = 255;
/// Maximum accepted content-type length.
const MAX_CONTENT_TYPE_LEN: usize = 255;

/// Business logic layer for attachment operations.
pub struct AttachmentService {
    repo: AttachmentRepository,
    storage: Arc<ObjectStorageClient>,
    max_file_size: i64,
    max_files_per_session: i64,
}

impl AttachmentService {
    pub fn new(
        repo: AttachmentRepository,
        storage: Arc<ObjectStorageClient>,
        max_file_size: i64,
        max_files_per_session: i64,
    ) -> Self {
        Self { repo, storage, max_file_size, max_files_per_session }
    }

    /// List attachments, optionally filtered by agent.
    pub async fn list(&self, scope: &TenantScope, agent_id: Option<AgentId>) -> AppResult<Vec<Attachment>> {
        self.repo.list(scope, agent_id).await
    }

    /// Get attachment metadata by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Attachment> {
        self.repo.get(scope, id).await
    }

    /// Store attachment bytes, then create the metadata row.
    ///
    /// Object upload happens before the database insert so the API never
    /// exposes metadata for a file body that was never persisted. If metadata
    /// insertion fails, the uploaded object is cleaned up best-effort.
    pub async fn create(
        &self,
        scope: &TenantScope,
        agent_id: Option<AgentId>,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> AppResult<Attachment> {
        let filename = validate_filename(filename)?;
        let content_type = validate_content_type(content_type)?;
        let size_bytes =
            i64::try_from(bytes.len()).map_err(|_| ErrorKind::Validation("attachment is too large".to_string()))?;
        if size_bytes > self.max_file_size {
            return Err(ErrorKind::Validation(format!(
                "attachment exceeds configured size limit of {} bytes",
                self.max_file_size
            ))
            .into());
        }
        if let Some(agent_id) = agent_id {
            let existing = self.repo.count_for_agent(scope, agent_id).await?;
            if existing >= self.max_files_per_session {
                return Err(ErrorKind::Conflict(format!(
                    "attachment limit reached for agent {agent_id}: max {}",
                    self.max_files_per_session
                ))
                .into());
            }
        }

        let id = AttachmentId::new();
        let storage_path = object_key(scope, id, &filename);
        let storage_backend = self.storage.backend();
        self.storage.put_bytes(&storage_path, &content_type, bytes).await?;

        match self
            .repo
            .create(
                scope,
                NewAttachment {
                    id,
                    agent_id,
                    filename: &filename,
                    content_type: &content_type,
                    size_bytes,
                    storage_path: &storage_path,
                    storage_backend,
                },
            )
            .await
        {
            Ok(attachment) => Ok(attachment),
            Err(err) => {
                if let Err(cleanup_err) = self.storage.delete(&storage_path).await {
                    tracing::warn!(
                        attachment_id = %id,
                        storage_path = %storage_path,
                        error = ?cleanup_err.kind,
                        "failed to clean up attachment object after metadata insert failure"
                    );
                }
                Err(err)
            }
        }
    }

    /// Load attachment metadata and object bytes.
    pub async fn download(&self, scope: &TenantScope, id: Uuid) -> AppResult<(Attachment, Vec<u8>)> {
        let attachment = self.repo.get(scope, id).await?;
        let bytes = self.storage.get_bytes(&attachment.storage_path).await?;
        Ok((attachment, bytes))
    }

    /// Delete the object before deleting metadata so a failed object deletion
    /// does not leave a database row claiming the file was removed.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let attachment = self.repo.get(scope, id).await?;
        self.storage.delete(&attachment.storage_path).await?;
        self.repo.delete(scope, id).await
    }
}

fn validate_filename(filename: &str) -> AppResult<String> {
    let filename = filename.trim();
    if filename.is_empty() || filename.len() > MAX_FILENAME_LEN {
        return Err(ErrorKind::Validation(format!("filename must be 1-{MAX_FILENAME_LEN} characters")).into());
    }
    if matches!(filename, "." | "..") {
        return Err(ErrorKind::Validation("filename must not be a relative path segment".into()).into());
    }
    if filename.chars().any(|ch| ch.is_control() || matches!(ch, '/' | '\\')) {
        return Err(
            ErrorKind::Validation("filename must not contain path separators or control characters".into()).into()
        );
    }
    Ok(filename.to_string())
}

fn validate_content_type(content_type: &str) -> AppResult<String> {
    let content_type = content_type.trim();
    if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
        return Err(ErrorKind::Validation(format!("content_type must be 1-{MAX_CONTENT_TYPE_LEN} characters")).into());
    }
    if content_type.chars().any(char::is_control) {
        return Err(ErrorKind::Validation("content_type must not contain control characters".into()).into());
    }
    Ok(content_type.to_string())
}

fn object_key(scope: &TenantScope, id: AttachmentId, filename: &str) -> String {
    format!("organizations/{}/attachments/{}/{}", scope.org_id().as_uuid(), id.as_uuid(), filename_segment(filename))
}

fn filename_segment(filename: &str) -> String {
    let segment = filename
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '_',
        })
        .collect::<String>();
    if segment.is_empty() { "file".to_string() } else { segment }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_length_limit() {
        assert_eq!(MAX_FILENAME_LEN, 255);
    }

    #[test]
    fn filename_validation_rejects_path_segments() {
        assert!(validate_filename("report.txt").is_ok());
        assert!(validate_filename(".").is_err());
        assert!(validate_filename("..").is_err());
        assert!(validate_filename("../report.txt").is_err());
        assert!(validate_filename("nested/report.txt").is_err());
        assert!(validate_filename("bad\nname.txt").is_err());
    }

    #[test]
    fn object_key_is_tenant_scoped_and_safe() {
        let scope = crate::test_support::tenant_scope();
        let key = object_key(&scope, AttachmentId::new(), "weekly report.txt");
        assert!(key.starts_with("organizations/"));
        assert!(key.contains("/attachments/"));
        assert!(key.ends_with("/weekly_report.txt"));
    }

    #[test]
    fn content_type_validation_rejects_header_injection() {
        assert!(validate_content_type("text/plain").is_ok());
        assert!(validate_content_type("text/plain\r\nx: y").is_err());
    }
}
