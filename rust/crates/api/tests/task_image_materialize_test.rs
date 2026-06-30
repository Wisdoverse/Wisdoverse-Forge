//! Hermetic integration test for the instruction-image materialization seam.
//!
//! Per-unit tests cover the edges (re-encode, symlink-safe writer, the sidecar
//! command, the frontend gate), but nothing exercised the middle wired together:
//! upload (re-encode + workspace stamp) -> object store -> capability/workspace
//! gates -> on-disk materialization into the agent's `/workspace`. This drives
//! `AttachmentService::create_image_upload_for_agent` into
//! `TaskImageMaterializer::materialize_for_dispatch` against real Postgres
//! (`#[sqlx::test]`), real local object storage, and a real temp filesystem. It
//! stops at the server-owned half (returns the container paths + writes the
//! files); spawning a container or invoking a CLI is the staging-only half and is
//! intentionally out of scope.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use agentforge_api::repositories::agent::AgentRepository;
use agentforge_api::repositories::attachment::AttachmentRepository;
use agentforge_api::services::attachment::AttachmentService;
use agentforge_api::services::task_image_materializer::TaskImageMaterializer;
use agentforge_api::test_support::{seed_cli_agent, tenant_scope_for_ids};
use agentforge_core::{RuntimeKind, TenantScope, workspace::workspace_projects_root};
use agentforge_infra::ObjectStorageClient;
use sqlx::PgPool;
use uuid::Uuid;

/// A 70-byte valid 1x1 PNG. The upload path decodes + re-encodes it, so it must
/// be a real image; the bytes are opaque on purpose (the test asserts the PNG
/// signature round-trips, not these exact bytes).
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xe0, 0x12, 0x91, 0xfb, 0x0f, 0x00, 0x01, 0xa4, 0x01, 0x3c, 0x93, 0x8b, 0x0e,
    0xb7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

const PNG_SIGNATURE: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

async fn seed_org_user(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind("Image Materialize Test Org")
        .bind(format!("img-materialize-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    // seed_cli_agent uses workspace_id == org_id; agents.workspace_id has an FK to
    // workspaces, so the workspace must exist first (id == org_id, the test convention).
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("img-materialize-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    (org_id, user_id)
}

fn temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

/// Build an AttachmentService + TaskImageMaterializer that SHARE one object store
/// (so an upload is readable back by the materializer) over a temp workspace root.
fn build(
    pool: &PgPool,
    storage: Arc<ObjectStorageClient>,
    workspace_root: &str,
) -> (AttachmentService, TaskImageMaterializer) {
    let attachments = AttachmentService::new(
        AttachmentRepository::new(pool.clone()),
        AgentRepository::new(pool.clone()),
        storage.clone(),
        10 * 1024 * 1024,
        20,
    );
    let materializer = TaskImageMaterializer::new(
        Arc::new(AttachmentRepository::new(pool.clone())),
        storage,
        workspace_root.to_string(),
    );
    (attachments, materializer)
}

async fn load_agent(
    pool: &PgPool,
    scope: &TenantScope,
    agent_id: agentforge_core::AgentId,
) -> agentforge_db::entities::Agent {
    AgentRepository::new(pool.clone()).find_by_id(scope, agent_id).await.expect("load agent entity")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn materialize_round_trips_image_into_container_agent_workspace(pool: PgPool) {
    let (org, user) = seed_org_user(&pool).await;
    let scope = tenant_scope_for_ids(org, user);
    let agent_id = seed_cli_agent(&pool, org, user, "claude").await; // container + vision CLI; workspace_id == org

    let storage: Arc<ObjectStorageClient> = Arc::new(ObjectStorageClient::Local { root: temp_dir("af-img-store") });
    let ws_root = temp_dir("af-img-ws");
    let ws_root_str = ws_root.to_str().expect("utf8 ws root");
    // The materializer writes under the agent's projects root; it must pre-exist.
    let projects_root = workspace_projects_root(ws_root_str, org, org); // workspace_id == org
    std::fs::create_dir_all(&projects_root).expect("pre-create projects root");

    let (attachments, materializer) = build(&pool, storage, ws_root_str);

    // Upload: re-encoded to PNG, kind=image, stamped with the agent's workspace.
    let att = attachments
        .create_image_upload_for_agent(&scope, agent_id, "screenshot.png", TINY_PNG.to_vec())
        .await
        .expect("upload image for agent");
    assert_eq!(att.kind, "image");
    assert_eq!(att.workspace_id, Some(org), "image must be stamped with the agent's workspace");

    // Materialize for dispatch.
    let agent = load_agent(&pool, &scope, agent_id).await;
    let task_id = Uuid::new_v4();
    let paths = materializer
        .materialize_for_dispatch(&scope, &agent, task_id, &[att.id.as_uuid().to_string()])
        .await
        .expect("materialize for dispatch");

    // Container-relative path returned for the assignment.
    assert_eq!(paths.len(), 1, "exactly one image path");
    let container_prefix = format!("/workspace/.task-images/{task_id}/");
    assert!(paths[0].starts_with(&container_prefix), "unexpected container path: {}", paths[0]);
    assert!(paths[0].ends_with(".png"), "materialized name must be .png: {}", paths[0]);

    // The bytes actually landed on disk, world-readable (the container reads them), as a valid PNG.
    let filename = paths[0].rsplit('/').next().expect("filename component");
    let on_disk = projects_root.join(".task-images").join(task_id.to_string()).join(filename);
    let meta = std::fs::metadata(&on_disk).expect("materialized file exists on disk");
    let mode = meta.permissions().mode();
    assert!(mode & 0o004 != 0, "materialized image must be other-readable for the container, mode={mode:o}");
    let bytes = std::fs::read(&on_disk).expect("read materialized image");
    assert!(bytes.starts_with(PNG_SIGNATURE), "materialized bytes must be a PNG (re-encoded)");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rejects_image_from_a_foreign_workspace_without_writing(pool: PgPool) {
    let (org, user) = seed_org_user(&pool).await;
    let scope = tenant_scope_for_ids(org, user);
    let agent_id = seed_cli_agent(&pool, org, user, "claude").await; // workspace_id == org

    let storage: Arc<ObjectStorageClient> = Arc::new(ObjectStorageClient::Local { root: temp_dir("af-img-store") });
    let ws_root = temp_dir("af-img-ws");
    let ws_root_str = ws_root.to_str().expect("utf8 ws root");
    let (attachments, materializer) = build(&pool, storage, ws_root_str);

    // Same org (so the org-scoped fetch succeeds), but a DIFFERENT workspace than
    // the agent — the CLAUDE.md execution boundary must reject this.
    let foreign_workspace = Uuid::new_v4();
    let att = attachments
        .create_image_upload(&scope, foreign_workspace, None, "leak.png", TINY_PNG.to_vec())
        .await
        .expect("upload foreign-workspace image");
    assert_eq!(att.workspace_id, Some(foreign_workspace));

    let agent = load_agent(&pool, &scope, agent_id).await;
    let task_id = Uuid::new_v4();
    let err = materializer
        .materialize_for_dispatch(&scope, &agent, task_id, &[att.id.as_uuid().to_string()])
        .await
        .expect_err("foreign-workspace image must be rejected");
    let msg = format!("{}", err.kind);
    assert!(
        msg.contains(&att.id.as_uuid().to_string()) || msg.to_lowercase().contains("image"),
        "expected image-not-found across the workspace boundary, got: {err:?}"
    );

    // Nothing must have been written for this task.
    let projects_root = workspace_projects_root(ws_root_str, org, org);
    let task_dir = projects_root.join(".task-images").join(task_id.to_string());
    assert!(!task_dir.exists(), "no image must be materialized when the boundary check fails");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rejects_non_container_runtime_before_touching_images(pool: PgPool) {
    let (org, user) = seed_org_user(&pool).await;
    let scope = tenant_scope_for_ids(org, user);
    let agent_id = seed_cli_agent(&pool, org, user, "claude").await;

    let storage: Arc<ObjectStorageClient> = Arc::new(ObjectStorageClient::Local { root: temp_dir("af-img-store") });
    let ws_root = temp_dir("af-img-ws");
    let (_attachments, materializer) = build(&pool, storage, ws_root.to_str().expect("utf8 ws root"));

    // A Host-CLI agent's /workspace is unreachable from the server: image tasks
    // must fail closed before any image is resolved or any file is touched.
    let mut agent = load_agent(&pool, &scope, agent_id).await;
    agent.runtime_kind = RuntimeKind::Cli;

    let task_id = Uuid::new_v4();
    let err = materializer
        .materialize_for_dispatch(&scope, &agent, task_id, &[Uuid::new_v4().to_string()])
        .await
        .expect_err("non-container runtime must be rejected");
    assert!(
        format!("{}", err.kind).contains("container CLI agents"),
        "expected container-only rejection, got: {err:?}"
    );
}
