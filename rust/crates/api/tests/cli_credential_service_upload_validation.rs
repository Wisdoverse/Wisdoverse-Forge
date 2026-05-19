//! Validation-only tests for CliCredentialService::upload. No HTTP layer;
//! we drive the service directly so error messages are checked verbatim.

use std::path::PathBuf;

use agentforge_api::repositories::credential::cli::CliCredentialRepository;
use agentforge_api::repositories::user_llm_config::UserLlmConfigRepository;
use agentforge_api::services::cli_credential::CliCredentialService;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

const TEST_KEY: [u8; 32] = [0x42; 32];

fn service(pool: PgPool) -> CliCredentialService {
    CliCredentialService::new(
        CliCredentialRepository::new(pool.clone()),
        UserLlmConfigRepository::new(pool),
        Some(TEST_KEY),
        PathBuf::from("/tmp/agentforge-upload-test"),
        None,
        None,
        None,
    )
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upload_rejects_non_object_files_payload(pool: PgPool) {
    let svc = service(pool);
    let scope = common::scope_for(Uuid::new_v4());
    let err = svc.upload(&scope, "codex", &serde_json::json!(42)).await.unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(msg.contains("must be a JSON object"), "got: {msg}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upload_rejects_non_string_file_values(pool: PgPool) {
    let svc = service(pool);
    let scope = common::scope_for(Uuid::new_v4());

    for (bad, kind) in [
        (serde_json::json!({"auth.json": 42}), "number"),
        (serde_json::json!({"auth.json": true}), "bool"),
        (serde_json::json!({"auth.json": null}), "null"),
        (serde_json::json!({"auth.json": []}), "array"),
        (serde_json::json!({"auth.json": {}}), "object"),
    ] {
        let err = svc.upload(&scope, "codex", &bad).await.unwrap_err();
        let msg = format!("{:?}", err.kind);
        assert!(msg.contains("must be a string"), "{kind}: {msg}");
        assert!(msg.contains(kind), "error must name the offending type {kind}: {msg}");
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upload_rejects_empty_object(pool: PgPool) {
    let svc = service(pool);
    let scope = common::scope_for(Uuid::new_v4());
    let err = svc.upload(&scope, "codex", &serde_json::json!({})).await.unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(msg.contains("must not be empty"), "got: {msg}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upload_rejects_when_no_encryption_key_configured(pool: PgPool) {
    // LLM_ENCRYPTION_KEY absent → refuse to store plaintext.
    let svc = CliCredentialService::new(
        CliCredentialRepository::new(pool.clone()),
        UserLlmConfigRepository::new(pool),
        None,
        PathBuf::from("/tmp"),
        None,
        None,
        None,
    );
    let err = svc
        .upload(&common::scope_for(Uuid::new_v4()), "codex", &serde_json::json!({"auth.json": "{}"}))
        .await
        .unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(msg.contains("LLM_ENCRYPTION_KEY"), "got: {msg}");
}
