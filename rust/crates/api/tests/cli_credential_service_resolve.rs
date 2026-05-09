//! End-to-end resolve() coverage. Exercises every branch of the 3-tier
//! decision tree with a live encryption key so the decrypt paths are real,
//! not stubbed.

mod common;

use std::path::PathBuf;

use agentforge_api::repositories::cli_credential::CliCredentialRepository;
use agentforge_api::repositories::user_llm_config::UserLlmConfigRepository;
use agentforge_api::services::cli_credential::CliCredentialService;
use agentforge_core::crypto;
use secrecy::SecretString;
use sqlx::PgPool;
use uuid::Uuid;

const TEST_KEY: [u8; 32] = [0x42; 32];

fn tmp_mount_root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("agentforge-resolve-test-{tag}-{}", Uuid::new_v4()))
}

fn service_with(
    pool: PgPool,
    mount_root: PathBuf,
    encryption_key: Option<[u8; 32]>,
    anthropic: Option<&str>,
    google: Option<&str>,
    openai: Option<&str>,
) -> CliCredentialService {
    CliCredentialService::new(
        CliCredentialRepository::new(pool.clone()),
        UserLlmConfigRepository::new(pool),
        encryption_key,
        mount_root,
        anthropic.map(|s| SecretString::from(s.to_string())),
        google.map(|s| SecretString::from(s.to_string())),
        openai.map(|s| SecretString::from(s.to_string())),
    )
}

#[sqlx::test(migrations = "../db/migrations")]
async fn tier_1_user_api_key_wins_over_oauth_and_system(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("tier1");
    let svc = service_with(pool.clone(), root.clone(), Some(TEST_KEY), Some("system-key"), None, None);

    // Seed user_llm_configs with encrypted API key.
    let encrypted = crypto::encrypt_base64(&TEST_KEY, "user-api-key-value").unwrap();
    sqlx::query(
        "INSERT INTO user_llm_configs (user_id, provider, encrypted_api_key, is_default)
         VALUES ($1, 'anthropic', $2, TRUE)",
    )
    .bind(user)
    .bind(&encrypted)
    .execute(&pool)
    .await
    .unwrap();

    // Also seed an OAuth blob — must be ignored because tier 1 wins.
    let oauth_ct = crypto::encrypt_base64(&TEST_KEY, r#"{"auth.json":"{}"}"#).unwrap();
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials) VALUES ($1, 'claude', $2)",
    )
    .bind(user)
    .bind(&oauth_ct)
    .execute(&pool)
    .await
    .unwrap();

    let inj = svc.resolve(&common::scope_for(user), "claude", "container-key-1").await.unwrap();
    let source = inj.env.iter().find(|(k, _)| k == "AGENTFORGE_CREDENTIAL_SOURCE").map(|(_, v)| v.as_str());
    assert_eq!(source, Some("user"));
    let api_key = inj.env.iter().find(|(k, _)| k == "ANTHROPIC_API_KEY").map(|(_, v)| v.as_str());
    assert_eq!(api_key, Some("user-api-key-value"));
    assert!(inj.oauth_mount_host_dir.is_none(), "tier 1 never mounts");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn tier_1_decrypt_failure_errors_does_not_fall_through(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("tier1-err");
    let svc = service_with(pool.clone(), root, Some(TEST_KEY), Some("system-key"), None, None);

    // Seed a row encrypted with a DIFFERENT key (simulates rotation without re-encrypt).
    let wrong_key: [u8; 32] = [0x99; 32];
    let encrypted = crypto::encrypt_base64(&wrong_key, "legacy-key-value").unwrap();
    sqlx::query(
        "INSERT INTO user_llm_configs (user_id, provider, encrypted_api_key, is_default) VALUES ($1, 'anthropic', $2, TRUE)",
    )
    .bind(user)
    .bind(&encrypted)
    .execute(&pool)
    .await
    .unwrap();

    let err = svc.resolve(&common::scope_for(user), "claude", "container-key-1b").await.unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(
        msg.contains("failed to decrypt") || msg.contains("LLM_ENCRYPTION_KEY rotation"),
        "decrypt failure must surface as Internal error, not fall through. got: {msg}"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn tier_2_oauth_writes_mount_dir(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("tier2-ok");
    let svc = service_with(pool.clone(), root.clone(), Some(TEST_KEY), Some("system-fallback"), None, None);

    let file_map = serde_json::json!({ "auth.json": r#"{"tokens":{"access_token":"at","refresh_token":"rt"}}"# });
    let ct = crypto::encrypt_base64(&TEST_KEY, &file_map.to_string()).unwrap();
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials) VALUES ($1, 'claude', $2)",
    )
    .bind(user)
    .bind(&ct)
    .execute(&pool)
    .await
    .unwrap();

    let inj = svc.resolve(&common::scope_for(user), "claude", "container-key-2").await.unwrap();
    assert_eq!(
        inj.env.iter().find(|(k, _)| k == "AGENTFORGE_CREDENTIAL_SOURCE").map(|(_, v)| v.as_str()),
        Some("oauth-db-mount"),
    );
    let mount = inj.oauth_mount_host_dir.expect("mount dir set");
    assert!(mount.join("credentials").exists(), "credentials file written at {mount:?}");
    // Tier 3 must not also activate — no ANTHROPIC_API_KEY from system key.
    assert!(inj.env.iter().all(|(k, _)| k != "ANTHROPIC_API_KEY"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn tier_2_revoked_row_falls_through_to_system_key(pool: PgPool) {
    // Regression lock: this is the behavior added by #45 — `find_encrypted_active`
    // filters revoked rows so a revoked OAuth row no longer blocks tier 3.
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("tier2-revoked");
    let svc = service_with(pool.clone(), root, Some(TEST_KEY), Some("system-fallback"), None, None);

    let ct = crypto::encrypt_base64(&TEST_KEY, r#"{"auth.json":"{}"}"#).unwrap();
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials, revoked_at, revoke_reason)
         VALUES ($1, 'claude', $2, NOW(), 'invalid_grant')",
    )
    .bind(user)
    .bind(&ct)
    .execute(&pool)
    .await
    .unwrap();

    let inj = svc.resolve(&common::scope_for(user), "claude", "container-key-3").await.unwrap();
    assert_eq!(
        inj.env.iter().find(|(k, _)| k == "AGENTFORGE_CREDENTIAL_SOURCE").map(|(_, v)| v.as_str()),
        Some("system"),
        "revoked OAuth must not block tier 3"
    );
    assert_eq!(
        inj.env.iter().find(|(k, _)| k == "ANTHROPIC_API_KEY").map(|(_, v)| v.as_str()),
        Some("system-fallback"),
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn tier_3_empty_system_key_treated_as_absent(pool: PgPool) {
    // `CONTAINER_*_API_KEY=` in .env parses as Some("") — must NOT inject
    // a blank env var (would mask real auth-missing signal with a bogus
    // `AGENTFORGE_CREDENTIAL_SOURCE=system`).
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("tier3-empty");
    let svc = service_with(pool.clone(), root, Some(TEST_KEY), Some(""), Some(""), Some(""));

    let inj = svc.resolve(&common::scope_for(user), "claude", "container-key-4").await.unwrap();
    assert!(inj.env.is_empty(), "empty system key must produce empty injection, got {:?}", inj.env);
    assert!(inj.oauth_mount_host_dir.is_none());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_cli_tool_yields_empty_injection(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("unknown");
    let svc = service_with(pool, root, Some(TEST_KEY), Some("system-key"), None, None);

    let inj = svc.resolve(&common::scope_for(user), "vim", "container-key-5").await.unwrap();
    assert!(inj.env.is_empty());
    assert!(inj.oauth_mount_host_dir.is_none());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn resolve_rejects_path_traversal_container_key(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let root = tmp_mount_root("traversal");
    let svc = service_with(pool.clone(), root.clone(), Some(TEST_KEY), None, None, None);

    // Seed a valid OAuth row so resolve reaches write_oauth_mount.
    let file_map = serde_json::json!({ "auth.json": "{}" });
    let ct = crypto::encrypt_base64(&TEST_KEY, &file_map.to_string()).unwrap();
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials) VALUES ($1, 'claude', $2)",
    )
    .bind(user)
    .bind(&ct)
    .execute(&pool)
    .await
    .unwrap();

    // When write_oauth_mount rejects the container_key it currently falls
    // back to the env-var delivery path (not an error). The guard test must
    // assert that: (a) no directory named after the bad key exists on disk,
    // (b) AGENTFORGE_OAUTH_CREDENTIALS env fallback fires instead.
    for bad in ["..", "/", "../etc", ".", ""] {
        let inj = svc.resolve(&common::scope_for(user), "claude", bad).await.unwrap();
        assert!(inj.oauth_mount_host_dir.is_none(), "bad key {bad:?} must not produce mount dir");
        assert!(
            inj.env.iter().any(|(k, _)| k == "AGENTFORGE_OAUTH_CREDENTIALS"),
            "bad key {bad:?} must trigger env-fallback delivery"
        );
    }
    // Defense-in-depth: the temp mount root must not contain any subdirectory
    // matching a bad key. We read the root's contents rather than joining on
    // `bad` because `root.join("/")` resolves to the filesystem root on Unix
    // (`exists()` is trivially true), which would make the assertion pass for
    // the wrong reason. Reading `root` tells us what actually got written.
    //
    // resolve() runs create_dir_all(&self.oauth_mount_root) unconditionally
    // before the path-traversal guard fires, so an empty root directory is
    // expected here. The invariant we enforce is that the root has NO child
    // entries — bad keys never reached the per-container `mount_dir.join(...)`
    // step.
    let entries: Vec<std::ffi::OsString> = match tokio::fs::read_dir(&root).await {
        Ok(mut rd) => {
            let mut out = Vec::new();
            while let Some(entry) = rd.next_entry().await.expect("read_dir iteration") {
                out.push(entry.file_name());
            }
            out
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => panic!("unexpected read_dir error: {err}"),
    };
    assert!(entries.is_empty(), "path-traversal guard broken — root {root:?} contains unexpected entries: {entries:?}");
}
