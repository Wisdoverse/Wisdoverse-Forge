//! Materialize instruction images for a container-CLI task assignment.
//!
//! Bridges the stored image attachments (MinIO) to the agent's `/workspace`
//! mount: gate on the agent's capability + tenant/workspace boundary, fetch the
//! re-encoded PNG bytes, and write them symlink-safely (see
//! `workspace_image_writer`). Returns the container-relative paths to thread
//! into `TaskAssignment.image_paths`. Fails closed on any violation.

use std::sync::Arc;

use agentforge_core::{AppResult, CliToolKind, ErrorKind, RuntimeCapability, RuntimeKind, TenantScope};
use agentforge_infra::ObjectStorageClient;
use uuid::Uuid;

use crate::domain::agent_workspace::{WorkspaceMountScope, resolve_agent_workspace_paths};
use crate::repositories::attachment::AttachmentRepository;
use crate::services::attachment::filename_segment;
use crate::services::workspace_image_writer::materialize_task_images;

#[derive(Clone)]
pub struct TaskImageMaterializer {
    attachments: Arc<AttachmentRepository>,
    object_storage: Arc<ObjectStorageClient>,
    workspace_root: String,
}

impl TaskImageMaterializer {
    pub fn new(
        attachments: Arc<AttachmentRepository>,
        object_storage: Arc<ObjectStorageClient>,
        workspace_root: String,
    ) -> Self {
        Self { attachments, object_storage, workspace_root }
    }

    /// Resolve, authorize, fetch and materialize the images for `task_id` into
    /// the agent's workspace. Returns container paths
    /// (`/workspace/.task-images/<task_id>/<file>`), or fails closed.
    pub async fn materialize_for_dispatch(
        &self,
        scope: &TenantScope,
        agent: &agentforge_db::entities::Agent,
        task_id: Uuid,
        image_ids: &[String],
    ) -> AppResult<Vec<String>> {
        if image_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Capability gate: only a vision-capable container CLI agent can consume
        // workspace-file images. The agent entity does not carry runtime_kind, so
        // container delivery is assumed; Host-CLI image delivery is out of scope.
        let cli_tool = agent
            .cli_tool
            .as_deref()
            .ok_or_else(|| ErrorKind::Validation("image task requires a container CLI agent".to_string()))?;
        let kind = CliToolKind::parse_legacy(cli_tool).map_err(|err| ErrorKind::Validation(err.to_string()))?;
        if !RuntimeCapability::for_cli_tool(kind, RuntimeKind::Container).supports_image_input {
            return Err(ErrorKind::Validation(format!("CLI tool '{cli_tool}' does not support image input")).into());
        }

        let workspace_id = agent.workspace_id.as_uuid();
        let mut images = Vec::with_capacity(image_ids.len());
        for id in image_ids {
            let uuid = Uuid::parse_str(id.trim())
                .map_err(|_| ErrorKind::Validation("image attachment id must be a UUID".to_string()))?;
            let attachment = self.attachments.get(scope, uuid).await?; // org-scoped
            if attachment.kind != "image" {
                return Err(ErrorKind::Validation(format!("attachment {uuid} is not an image")).into());
            }
            // CLAUDE.md execution boundary: image must belong to the agent's workspace.
            if attachment.workspace_id != Some(workspace_id) {
                return Err(ErrorKind::NotFound(format!("image {uuid}")).into());
            }
            let bytes = self.object_storage.get_bytes(&attachment.storage_path).await?;
            // UUID-prefixed + sanitized so two same-named images can't collide and
            // the on-disk name has no path separators.
            let filename = format!("{uuid}-{}", filename_segment(&attachment.filename));
            images.push((filename, bytes));
        }

        let paths = resolve_agent_workspace_paths(
            &self.workspace_root,
            WorkspaceMountScope { org_id: scope.org_id().as_uuid(), workspace_id },
            None,
        )?;
        materialize_task_images(&paths.host_projects_root, task_id, &images)
    }
}
