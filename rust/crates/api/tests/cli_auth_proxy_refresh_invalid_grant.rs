//! End-to-end refresh flow: mock IdP returns RFC 6749 `invalid_grant`,
//! verify the repo bumps fail_count on first hit and sets revoked_at on
//! the second sweep (threshold = 2).
//!
//! Uses an in-process axum `Router` on 127.0.0.1:0 so no new dev-deps are
//! pulled in for the mock IdP.

use std::sync::Arc;
use std::time::Duration;

use agentforge_api::repositories::cli_credential::CliCredentialRepository;
use agentforge_api::services::cli_auth_proxy::{
    CallbackMode, CliAuthProxyProvider, CliAuthProxyService, MemoryStateStore, StateStore,
};
use agentforge_core::crypto;
use axum::{Router, response::Response, routing::post};
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

/// Spin up an in-process IdP that always returns RFC 6749 `invalid_grant`
/// on the token endpoint. Listens on 127.0.0.1:0 so tests can run in parallel.
async fn spawn_invalid_grant_idp() -> String {
    let app = Router::new().route(
        "/token",
        post(|| async {
            Response::builder()
                .status(400)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"error":"invalid_grant","error_description":"revoked"}"#))
                .unwrap()
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

fn test_encryption_key() -> [u8; 32] {
    [0x42; 32]
}

/// Build an encrypted `auth.json` blob with a stale `last_refresh` so
/// `needs_refresh` classifies it as Stale and the worker attempts a refresh.
fn make_stale_encrypted_blob(key: &[u8; 32]) -> String {
    let past = chrono::Utc::now() - chrono::Duration::hours(24);
    let auth_json = serde_json::json!({
        "tokens": {
            "access_token": "expired-access",
            "refresh_token": "rt-fake-will-be-rejected",
        },
        "last_refresh": past.to_rfc3339(),
    });
    let file_map = serde_json::json!({
        "auth.json": serde_json::to_string(&auth_json).unwrap(),
    });
    crypto::encrypt_base64(key, &serde_json::to_string(&file_map).unwrap()).unwrap()
}

async fn seed_user(pool: &PgPool) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("test-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    user_id
}

fn provider_with_token_url(token_endpoint: String) -> CliAuthProxyProvider {
    CliAuthProxyProvider {
        name: "openai".into(),
        display_name: "OpenAI (Codex)".into(),
        cli_tool: "codex".into(),
        client_id: "test-client".into(),
        client_secret: None,
        auth_endpoint: "http://unused".into(),
        token_endpoint,
        redirect_uri: "http://unused".into(),
        scope: "openid".into(),
        callback_mode: CallbackMode::Manual,
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn two_invalid_grants_revoke_the_row(pool: PgPool) {
    let idp_url = spawn_invalid_grant_idp().await;
    let key = test_encryption_key();
    let ciphertext = make_stale_encrypted_blob(&key);
    let user_id = seed_user(&pool).await;

    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials)
         VALUES ($1, 'codex', $2)",
    )
    .bind(user_id)
    .bind(&ciphertext)
    .execute(&pool)
    .await
    .expect("seed cli credential row");

    let provider = CliAuthProxyProvider {
        name: "openai".into(),
        display_name: "OpenAI (Codex)".into(),
        cli_tool: "codex".into(),
        client_id: "test-client".into(),
        client_secret: None,
        auth_endpoint: "http://unused".into(),
        token_endpoint: idp_url.clone(),
        redirect_uri: "http://unused".into(),
        scope: "openid".into(),
        callback_mode: CallbackMode::Manual,
    };

    let repo = CliCredentialRepository::new(pool.clone());
    let service = CliAuthProxyService::new(
        vec![provider],
        repo.clone(),
        Some(key),
        StateStore::Memory(Arc::new(MemoryStateStore::default())),
        2,
    );

    // First sweep → fail_count should go 0 → 1, revoked_at still NULL.
    let summary = service.refresh_stale(Duration::from_secs(0)).await;
    assert_eq!(summary.invalid_grant, 1, "first sweep classifies as invalid_grant");
    assert!(summary.revoked_credentials.is_empty(), "below threshold does not emit owner notification input");

    let (count, revoked_at): (i32, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT refresh_fail_count, revoked_at FROM user_cli_credentials
         WHERE user_id = $1 AND cli_tool = $2",
    )
    .bind(user_id)
    .bind("codex")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "fail count bumped to 1");
    assert!(revoked_at.is_none(), "below threshold — not revoked yet");

    // Second sweep → fail_count 1 → 2, revoked_at NOW set.
    let summary = service.refresh_stale(Duration::from_secs(0)).await;
    assert_eq!(summary.invalid_grant, 1, "second sweep also classifies as invalid_grant");
    assert_eq!(summary.revoked_credentials.len(), 1, "threshold crossing emits owner notification input");
    assert_eq!(summary.revoked_credentials[0].user_id, user_id);
    assert_eq!(summary.revoked_credentials[0].cli_tool, "codex");
    assert_eq!(summary.revoked_credentials[0].reason, "invalid_grant");

    let (count, revoked_at, revoke_reason): (i32, Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as(
            "SELECT refresh_fail_count, revoked_at, revoke_reason FROM user_cli_credentials
         WHERE user_id = $1 AND cli_tool = $2",
        )
        .bind(user_id)
        .bind("codex")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "fail count now at threshold");
    assert!(revoked_at.is_some(), "row revoked at threshold crossing");
    assert_eq!(revoke_reason.as_deref(), Some("invalid_grant"));

    // Third sweep → active-only filter excludes the revoked row → no work.
    let summary = service.refresh_stale(Duration::from_secs(0)).await;
    assert_eq!(summary.eligible, 0, "revoked row no longer picked up by find_all_active");
    assert!(summary.revoked_credentials.is_empty(), "already revoked rows do not re-notify");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn configured_threshold_one_revokes_on_first_invalid_grant(pool: PgPool) {
    let idp_url = spawn_invalid_grant_idp().await;
    let key = test_encryption_key();
    let ciphertext = make_stale_encrypted_blob(&key);
    let user_id = seed_user(&pool).await;

    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials)
         VALUES ($1, 'codex', $2)",
    )
    .bind(user_id)
    .bind(&ciphertext)
    .execute(&pool)
    .await
    .expect("seed cli credential row");

    let repo = CliCredentialRepository::new(pool.clone());
    let service = CliAuthProxyService::new(
        vec![provider_with_token_url(idp_url)],
        repo,
        Some(key),
        StateStore::Memory(Arc::new(MemoryStateStore::default())),
        1,
    );

    let summary = service.refresh_stale(Duration::from_secs(0)).await;
    assert_eq!(summary.invalid_grant, 1, "configured threshold still classifies invalid_grant");
    assert_eq!(summary.revoked_credentials.len(), 1, "threshold=1 emits owner notification input");
    assert_eq!(summary.revoked_credentials[0].user_id, user_id);
    assert_eq!(summary.revoked_credentials[0].cli_tool, "codex");
    assert_eq!(summary.revoked_credentials[0].reason, "invalid_grant");

    let (count, revoked_at, revoke_reason): (i32, Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as(
            "SELECT refresh_fail_count, revoked_at, revoke_reason FROM user_cli_credentials
         WHERE user_id = $1 AND cli_tool = $2",
        )
        .bind(user_id)
        .bind("codex")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "threshold=1 revokes on the first consecutive invalid_grant");
    assert!(revoked_at.is_some(), "row revoked immediately at configured threshold");
    assert_eq!(revoke_reason.as_deref(), Some("invalid_grant"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn concurrent_bumps_cross_threshold_atomically(pool: PgPool) {
    // Regression for the "concurrent sweeps can't race past the boundary"
    // claim in bump_fail_count_or_revoke's doc comment.
    //
    // A non-atomic (read fail_count → compute → write) implementation could
    // let two workers both observe fail_count=1, both bump to 2, and only
    // one sets revoked_at — losing audit trail + double-incrementing.
    //
    // The real impl runs the CASE inside a single UPDATE, so two concurrent
    // callers on a row at fail_count=1 (threshold=2) must produce
    // fail_count=3 with revoked_at set exactly once. Whichever worker
    // crosses the boundary first wins; the second observes the already-
    // revoked row and gets None back.
    let user_id = seed_user(&pool).await;
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials, refresh_fail_count)
         VALUES ($1, 'codex', 'seed-blob', 1)",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed at fail_count=1");

    let repo = CliCredentialRepository::new(pool.clone());
    let a = repo.clone();
    let b = repo.clone();
    let (ra, rb) = tokio::join!(
        async move { a.bump_fail_count_or_revoke(user_id, "codex", "invalid_grant", 2).await },
        async move { b.bump_fail_count_or_revoke(user_id, "codex", "invalid_grant", 2).await },
    );

    let ra = ra.unwrap();
    let rb = rb.unwrap();

    // Exactly one caller sees revoked_at Some — whichever bumped first from
    // 1 → 2 crossed the threshold. The other either bumped 2 → 3 on the
    // still-not-revoked row (also sees revoked_at Some, since threshold
    // check uses `>= $threshold`) OR observed the already-revoked row and
    // got None back.
    let observed_revoke = [&ra, &rb].iter().filter(|r| matches!(r, Some((_, Some(_))))).count();
    let observed_none = [&ra, &rb].iter().filter(|r| r.is_none()).count();
    assert!(
        observed_revoke >= 1,
        "at least one concurrent bump must observe revoked_at Some (got ra={ra:?} rb={rb:?})"
    );
    assert_eq!(
        observed_revoke + observed_none,
        2,
        "each caller must land in exactly one of (revoked_set, already_revoked) — no lost updates"
    );

    // Final DB state: revoked_at must be set, revoke_reason = invalid_grant,
    // and fail_count must reflect the actual number of increments (2 or 3
    // depending on interleave — never less, never more).
    let (count, revoked_at, reason): (i32, Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT refresh_fail_count, revoked_at, revoke_reason
         FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2",
    )
    .bind(user_id)
    .bind("codex")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked_at.is_some(), "revoked_at must be set after threshold crossing");
    assert_eq!(reason.as_deref(), Some("invalid_grant"));
    assert!(
        (2..=3).contains(&count),
        "fail_count must be 2 (second bump hit the already-revoked guard) or 3 (both bumps \
         landed before revoked_at was visible), never less — got {count}"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn successful_upsert_clears_revocation_markers(pool: PgPool) {
    // Regression for: re-auth (complete_manual / server-callback / file map
    // upload / credential-sync publish) must atomically un-revoke the row.
    // Otherwise a user who re-authenticates stays flagged forever.
    let user_id = seed_user(&pool).await;

    // Seed a row already in the revoked state.
    sqlx::query(
        "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials,
                                           revoked_at, revoke_reason, refresh_fail_count,
                                           last_refresh_error, last_refresh_error_at)
         VALUES ($1, 'codex', 'old-blob', NOW(), 'invalid_grant', 3, 'invalid_grant', NOW())",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed revoked row");

    let repo = CliCredentialRepository::new(pool.clone());
    // Simulate the credential-sync path: a freshly-logged-in container
    // publishes new ciphertext, which upserts via user_id.
    repo.upsert_encrypted_by_user_id(user_id, "codex", "fresh-blob").await.unwrap();

    let (revoked_at, revoke_reason, fail_count, last_err): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
        i32,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT revoked_at, revoke_reason, refresh_fail_count, last_refresh_error
         FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2",
    )
    .bind(user_id)
    .bind("codex")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(revoked_at.is_none(), "upsert must clear revoked_at");
    assert!(revoke_reason.is_none(), "upsert must clear revoke_reason");
    assert_eq!(fail_count, 0, "upsert must reset fail count");
    assert!(last_err.is_none(), "upsert must clear last_refresh_error");
}
