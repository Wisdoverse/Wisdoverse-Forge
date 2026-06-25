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

/// Max images materialized for one task (bounds the in-tx MinIO+FS work and the
/// assignment envelope size); mirrors the provider/quick-message cap.
const MAX_TASK_IMAGES: usize = 8;

/// Cap on the readable stem of a materialized filename so the full on-disk
/// component (`<uuid>-<stem>.png`) stays under the 255-byte `NAME_MAX`:
/// 255 - 36 (uuid) - 1 (`-`) - 4 (`.png`) = 214.
const MAX_IMAGE_FILENAME_STEM: usize = 214;

/// Build the on-disk workspace filename for one image: `<uuid>-<stem>.png`.
/// The `uuid` prefix makes two same-named uploads collide-free and gives the
/// agent-unpredictable component; the stored bytes are always re-encoded PNG so
/// the extension is forced to `.png`; and the readable stem is bounded so the
/// component never exceeds `NAME_MAX` (which would fail `openat` at dispatch).
/// `filename_segment` is ASCII-only, so byte-slicing the stem is safe.
fn materialized_image_filename(uuid: Uuid, original: &str) -> String {
    let seg = filename_segment(original);
    let stem = seg.strip_suffix(".png").unwrap_or(&seg);
    let stem = &stem[..stem.len().min(MAX_IMAGE_FILENAME_STEM)];
    format!("{uuid}-{stem}.png")
}

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
        if image_ids.len() > MAX_TASK_IMAGES {
            return Err(
                ErrorKind::Validation(format!("at most {MAX_TASK_IMAGES} images may be attached to a task")).into()
            );
        }

        // Runtime gate: workspace-file delivery only works for a CONTAINER agent,
        // whose `/workspace` is a server-side bind mount we can write into. A
        // Host-CLI agent runs on the operator's own machine (its `/workspace` is
        // unreachable here), and is identified by a non-NULL `runtime_id`
        // (`host-*`); container agents have `runtime_id IS NULL`. Fail closed so a
        // vision-capable Host-CLI agent can't be handed dangling paths.
        if agent.runtime_id.is_some() {
            return Err(
                ErrorKind::Validation("image tasks are only supported for container CLI agents".to_string()).into()
            );
        }
        // Capability gate: only a vision-capable container CLI tool can consume images.
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
            images.push((materialized_image_filename(uuid, &attachment.filename), bytes));
        }

        let paths = resolve_agent_workspace_paths(
            &self.workspace_root,
            WorkspaceMountScope { org_id: scope.org_id().as_uuid(), workspace_id },
            None,
        )?;
        materialize_task_images(&paths.host_projects_root, task_id, &images)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialized_filename_is_uuid_prefixed_png() {
        let id = Uuid::nil();
        assert_eq!(materialized_image_filename(id, "screenshot.png"), format!("{id}-screenshot.png"));
        // Non-png stored name still gets a .png on-disk name (bytes are re-encoded PNG).
        assert_eq!(materialized_image_filename(id, "photo.jpeg"), format!("{id}-photo.jpeg.png"));
        // Path separators / odd chars are sanitized to underscores.
        assert_eq!(materialized_image_filename(id, "a/b c.png"), format!("{id}-a_b_c.png"));
    }

    #[test]
    fn materialized_filename_never_exceeds_name_max() {
        // A pathological near-255-char upload name must not produce an on-disk
        // component over NAME_MAX (255) — that would fail openat at dispatch.
        let long = format!("{}.png", "x".repeat(300));
        let name = materialized_image_filename(Uuid::new_v4(), &long);
        assert!(name.len() <= 255, "component must fit NAME_MAX, got {}", name.len());
        assert!(name.ends_with(".png"), "must keep the .png extension: {name}");
    }
}
