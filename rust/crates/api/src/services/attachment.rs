//! Attachment service — validation and management.

use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, AttachmentId, TenantScope};
use agentforge_db::entities::Attachment;
use agentforge_infra::ObjectStorageClient;
use uuid::Uuid;

pub(crate) use crate::domain::attachment::{
    AttachmentAgentScope, AttachmentUploadDraft, DEFAULT_ATTACHMENT_CONTENT_TYPE, attachment_data_response,
    attachment_delete_response, attachment_download_content_disposition,
};
use crate::domain::attachment::{
    AttachmentContentType, AttachmentCountPolicy, AttachmentDownload, AttachmentFilename, AttachmentPayloadSize,
};
use crate::repositories::attachment::{AttachmentRepository, NewAttachment};

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
        self.create_upload(
            scope,
            AttachmentUploadDraft {
                filename: filename.to_string(),
                content_type: content_type.to_string(),
                agent_id,
                bytes,
            },
        )
        .await
    }

    pub(crate) async fn create_upload(
        &self,
        scope: &TenantScope,
        upload: AttachmentUploadDraft,
    ) -> AppResult<Attachment> {
        let AttachmentUploadDraft { filename, content_type, agent_id, bytes } = upload;
        let filename = AttachmentFilename::parse(&filename)?;
        let content_type = AttachmentContentType::parse(&content_type)?;
        let size_bytes = AttachmentPayloadSize::from_len(bytes.len(), self.max_file_size)?.bytes();
        if let Some(agent_id) = agent_id {
            let existing = self.repo.count_for_agent(scope, agent_id).await?;
            AttachmentCountPolicy::ensure_agent_file_slot(agent_id, existing, self.max_files_per_session)?;
        }

        let id = AttachmentId::new();
        let storage_path = object_key(scope, id, filename.value());
        let storage_backend = self.storage.backend();
        self.storage.put_bytes(&storage_path, content_type.value(), bytes).await?;

        match self
            .repo
            .create(
                scope,
                NewAttachment {
                    id,
                    agent_id,
                    filename: filename.value(),
                    content_type: content_type.value(),
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

    pub(crate) async fn download_payload(&self, scope: &TenantScope, id: Uuid) -> AppResult<AttachmentDownload> {
        let (attachment, bytes) = self.download(scope, id).await?;
        Ok(AttachmentDownload::new(attachment.filename, attachment.content_type, bytes))
    }

    /// Delete the object before deleting metadata so a failed object deletion
    /// does not leave a database row claiming the file was removed.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let attachment = self.repo.get(scope, id).await?;
        self.storage.delete(&attachment.storage_path).await?;
        self.repo.delete(scope, id).await
    }
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
    fn object_key_is_tenant_scoped_and_safe() {
        let scope = crate::test_support::tenant_scope();
        let key = object_key(&scope, AttachmentId::new(), "weekly report.txt");
        assert!(key.starts_with("organizations/"));
        assert!(key.contains("/attachments/"));
        assert!(key.ends_with("/weekly_report.txt"));
    }
}
