//! Domain error contracts for instruction-image upload, materialization, and
//! provider image blocks.
//!
//! The services and routes that handle instruction images (`task_image_materializer`,
//! `workspace_image_writer`, `agent_prompt`, `orchestration`, the attachments route)
//! must not own user-visible `ErrorKind` policy (see `route_ddd_boundary_test`); they
//! call these helpers instead. Every function returns the validated `AppError`
//! contract, keeping the wording and error class in one auditable place.

use agentforge_core::{AppError, ErrorKind};
use uuid::Uuid;

/// Images attached to a CLI agent's quick (non-task) message — unsupported; they
/// must ride the task-dispatch path.
pub(crate) fn cli_quick_message_images_unsupported() -> AppError {
    ErrorKind::Validation(
        "images are not supported for a CLI agent's quick message; attach them to a task instead".to_string(),
    )
    .into()
}

/// The provider agent has no provider configured to accept image input.
pub(crate) fn agent_has_no_provider() -> AppError {
    ErrorKind::Validation("agent has no provider for image input".to_string()).into()
}

/// The (provider, model) pair does not support image input.
pub(crate) fn model_does_not_support_images(model: &str, provider: &str) -> AppError {
    ErrorKind::Validation(format!("model '{model}' on provider '{provider}' does not support image input")).into()
}

/// More than `max` images attached to a single instruction (provider path).
pub(crate) fn too_many_instruction_images(max: usize) -> AppError {
    ErrorKind::Validation(format!("at most {max} images may be attached to one instruction")).into()
}

/// More than `max` images attached to a single task (dispatch path).
pub(crate) fn too_many_task_images(max: usize) -> AppError {
    ErrorKind::Validation(format!("at most {max} images may be attached to a task")).into()
}

/// An attachment id that is not a UUID.
pub(crate) fn invalid_attachment_id() -> AppError {
    ErrorKind::Validation("image attachment id must be a UUID".to_string()).into()
}

/// The referenced attachment is not an image.
pub(crate) fn attachment_not_an_image(id: Uuid) -> AppError {
    ErrorKind::Validation(format!("attachment {id} is not an image")).into()
}

/// An image referenced outside the agent's workspace — surfaced as not-found so a
/// cross-workspace probe cannot confirm existence.
pub(crate) fn image_not_found(id: Uuid) -> AppError {
    ErrorKind::NotFound(format!("image {id}")).into()
}

/// Image tasks are only supported for a container CLI agent (a Host CLI agent's
/// workspace is unreachable from the server).
pub(crate) fn image_tasks_container_only() -> AppError {
    ErrorKind::Validation("image tasks are only supported for container CLI agents".to_string()).into()
}

/// An image task without a resolvable container CLI tool.
pub(crate) fn image_task_requires_container_cli() -> AppError {
    ErrorKind::Validation("image task requires a container CLI agent".to_string()).into()
}

/// An unparseable CLI tool name while gating image capability.
pub(crate) fn invalid_cli_tool(err: impl std::fmt::Display) -> AppError {
    ErrorKind::Validation(err.to_string()).into()
}

/// The agent's CLI tool does not support image input.
pub(crate) fn cli_tool_does_not_support_images(cli_tool: &str) -> AppError {
    ErrorKind::Validation(format!("CLI tool '{cli_tool}' does not support image input")).into()
}

/// An instruction carrying images was created without an assigned vision-capable
/// agent (it must be push-dispatched, not auto-dispatched).
pub(crate) fn images_require_assigned_vision_agent() -> AppError {
    ErrorKind::Validation("an instruction with images must be assigned to a vision-capable agent".to_string()).into()
}

/// The target agent's sidecar predates the image-input capability (rolling deploy).
pub(crate) fn sidecar_image_input_unsupported() -> AppError {
    ErrorKind::Validation(
        "agent's sidecar does not yet support instruction images; restart or roll the agent and retry".to_string(),
    )
    .into()
}

/// A `.task-images` path component is a symlink (escape attempt) — refused.
pub(crate) fn symlinked_path_component(name: &str) -> AppError {
    ErrorKind::Validation(format!("workspace image path component '{name}' is a symlink")).into()
}

/// A target image file is a symlink (escape attempt) — refused.
pub(crate) fn symlinked_image_file(filename: &str) -> AppError {
    ErrorKind::Validation(format!("workspace image file '{filename}' is a symlink")).into()
}

/// An unexpected filesystem error while materializing images into the workspace.
pub(crate) fn workspace_write_internal(context: &str, err: impl std::fmt::Display) -> AppError {
    ErrorKind::Internal(anyhow::anyhow!("{context}: {err}")).into()
}
