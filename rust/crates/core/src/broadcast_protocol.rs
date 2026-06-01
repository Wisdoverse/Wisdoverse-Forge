//! Subject taxonomy for the platform -> browser WebSocket broadcast channel.
//!
//! The WS gateway subscribes each connection to org- and scope-scoped
//! `broadcast.{org_id}...` subjects. A small number of subjects are
//! AUDIENCE-scoped rather than tenant-scoped: the admin CLI agent-image toast is
//! delivered on a single global subject that ONLY owner/admin connections
//! subscribe to.
//!
//! This is safe because every agent (sidecar) JWT denies `broadcast.>` for both
//! publish and subscribe (see the auth-callout perms), so a rooted sidecar can
//! neither read the admin toast nor spoof one. Keeping the subject + frame
//! discriminator here gives the producer (jobs updater worker) and the consumer
//! (api WS gateway) a single source of truth.

/// Global admin broadcast subject for CLI agent-image auto-updater toasts.
///
/// Producer: the deployment-side updater worker (`agentforge-jobs`). Consumer:
/// the WS gateway, which subscribes a connection to this subject only when the
/// JWT role is `owner` or `admin`.
pub const ADMIN_CLI_IMAGE_SUBJECT: &str = "broadcast.admin.cli_image";

/// The `type` discriminator the browser dispatch switches on for the toast
/// frame published on [`ADMIN_CLI_IMAGE_SUBJECT`]. Mirror of the TS literal in
/// `shared/types/protocol.ts`.
pub const CLI_IMAGE_UPDATED_EVENT: &str = "cli_image.updated";
