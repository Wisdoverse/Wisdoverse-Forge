//! Integration tests for CliAuthProxyService::complete_manual, covering
//! provider-mismatch, user-mismatch, and state-replay CSRF defenses.

use std::sync::Arc;
use std::time::Duration;

use agentforge_api::repositories::credential::cli::CliCredentialRepository;
use agentforge_api::services::cli_auth_proxy::{
    CallbackMode, CliAuthProxyProvider, CliAuthProxyService, MemoryStateStore, StateStore,
};
use agentforge_core::crypto;
use axum::{Router, response::Response, routing::post};
use sqlx::PgPool;
use tokio::net::TcpListener;

mod common;

const TEST_KEY: [u8; 32] = [0x42; 32];

fn provider_with_token_url(token_url: String) -> CliAuthProxyProvider {
    CliAuthProxyProvider {
        name: "openai".into(),
        display_name: "OpenAI (Codex)".into(),
        cli_tool: "codex".into(),
        client_id: "test-client".into(),
        client_secret: None,
        auth_endpoint: "http://unused/authorize".into(),
        token_endpoint: token_url,
        redirect_uri: "http://localhost:1455/auth/callback".into(),
        scope: "openid".into(),
        callback_mode: CallbackMode::Manual,
    }
}

fn extract_state_from_url(url: &str) -> String {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("state query param present")
}

async fn spawn_rejecting_idp() -> String {
    let app = Router::new().route(
        "/token",
        post(|| async {
            Response::builder()
                .status(400)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"error":"invalid_grant"}"#))
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

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_manual_rejects_provider_mismatch(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let store = Arc::new(MemoryStateStore::default());
    let provider = provider_with_token_url("http://unused/token".into());
    let service = CliAuthProxyService::new(
        vec![provider.clone()],
        CliCredentialRepository::new(pool.clone()),
        Some(TEST_KEY),
        StateStore::Memory(store.clone()),
        2,
    );

    // Authorize to populate the state store. The state is emitted in the URL —
    // parse it back out so we can craft the complete_manual payload.
    let url = service.authorize(&common::scope_for(user), "openai").await.unwrap();
    let state = extract_state_from_url(&url);

    // Register a second provider bound to the same cli_tool label so the
    // require_provider lookup succeeds but the state->provider check fails.
    let mut second = provider.clone();
    second.name = "other".into();
    let service2 = CliAuthProxyService::new(
        vec![provider, second],
        CliCredentialRepository::new(pool.clone()),
        Some(TEST_KEY),
        StateStore::Memory(store),
        2,
    );
    // Reuse the state we just issued for `openai` but claim it's for `other`.
    let err = service2
        .complete_manual(&common::scope_for(user), "other", &format!("code=abc&state={state}"))
        .await
        .unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(msg.contains("provider mismatch"), "expected provider-mismatch Validation, got: {msg}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_manual_rejects_user_mismatch(pool: PgPool) {
    let alice = common::seed_user(&pool).await;
    let bob = common::seed_user(&pool).await;
    let store = Arc::new(MemoryStateStore::default());
    let provider = provider_with_token_url("http://unused/token".into());
    let service = CliAuthProxyService::new(
        vec![provider],
        CliCredentialRepository::new(pool.clone()),
        Some(TEST_KEY),
        StateStore::Memory(store),
        2,
    );

    let url = service.authorize(&common::scope_for(alice), "openai").await.unwrap();
    let state = extract_state_from_url(&url);

    let err = service
        .complete_manual(&common::scope_for(bob), "openai", &format!("code=abc&state={state}"))
        .await
        .unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(
        msg.contains("different user") || msg.contains("belongs to a different"),
        "expected user-mismatch Validation, got: {msg}"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_manual_rejects_state_replay(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let store = Arc::new(MemoryStateStore::default());

    // Spin up an IdP that ALWAYS returns 400 so the first call consumes
    // state + fails at exchange_code. The test is about state replay, not
    // token exchange: on second call, state must already be gone.
    let token_url = spawn_rejecting_idp().await;
    let provider = provider_with_token_url(token_url);
    let service = CliAuthProxyService::new(
        vec![provider],
        CliCredentialRepository::new(pool.clone()),
        Some(TEST_KEY),
        StateStore::Memory(store),
        2,
    );

    let url = service.authorize(&common::scope_for(user), "openai").await.unwrap();
    let state = extract_state_from_url(&url);

    let first = service.complete_manual(&common::scope_for(user), "openai", &format!("code=abc&state={state}")).await;
    // First call hits exchange_code which fails — state was consumed before that.
    assert!(first.is_err(), "first call should error from token exchange");

    let second = service.complete_manual(&common::scope_for(user), "openai", &format!("code=abc&state={state}")).await;
    let err = second.unwrap_err();
    let msg = format!("{:?}", err.kind);
    assert!(msg.contains("invalid or expired OAuth state"), "replay must be rejected by take_state — got: {msg}");
}

async fn spawn_refreshing_idp_without_refresh_token() -> String {
    let app = Router::new().route(
        "/token",
        post(|| async {
            Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"access_token":"new-at","token_type":"Bearer","expires_in":3600}"#))
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

#[sqlx::test(migrations = "../db/migrations")]
async fn refresh_single_preserves_existing_refresh_token_when_idp_omits_it(pool: PgPool) {
    // IdP returns a 200 with a fresh access_token but no refresh_token.
    // Expected: stored auth.json still carries the original refresh_token
    // so the next refresh sweep can continue.
    let user = common::seed_user(&pool).await;
    let original_refresh = "rt-original-must-persist";

    let token_url = spawn_refreshing_idp_without_refresh_token().await;
    let provider = provider_with_token_url(token_url);

    let repo = CliCredentialRepository::new(pool.clone());
    let service = CliAuthProxyService::new(
        vec![provider.clone()],
        repo.clone(),
        Some(TEST_KEY),
        StateStore::Memory(Arc::new(MemoryStateStore::default())),
        2,
    );

    // Seed a stale row so refresh_stale picks it up.
    let past = chrono::Utc::now() - chrono::Duration::hours(24);
    let file_map = serde_json::json!({
        "auth.json": serde_json::to_string(&serde_json::json!({
            "tokens": {"access_token": "old-at", "refresh_token": original_refresh},
            "last_refresh": past.to_rfc3339(),
        })).unwrap()
    });
    let ct = crypto::encrypt_base64(&TEST_KEY, &file_map.to_string()).unwrap();
    sqlx::query("INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials) VALUES ($1, 'codex', $2)")
        .bind(user)
        .bind(&ct)
        .execute(&pool)
        .await
        .unwrap();

    let summary = service.refresh_stale(Duration::from_secs(0)).await;
    assert_eq!(summary.refreshed, 1, "refresh must succeed: {summary:?}");

    // Decrypt the stored ciphertext and assert refresh_token is still the original.
    let stored_ct = repo.find_encrypted(&common::scope_for(user), "codex").await.unwrap().expect("row still present");
    let plaintext = crypto::decrypt_base64(&TEST_KEY, &stored_ct).unwrap();
    let file_map: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
    let auth: serde_json::Value = serde_json::from_str(file_map["auth.json"].as_str().unwrap()).unwrap();
    assert_eq!(
        auth["tokens"]["refresh_token"].as_str(),
        Some(original_refresh),
        "refresh_token must be preserved when IdP omits it"
    );
    assert_eq!(
        auth["tokens"]["access_token"].as_str(),
        Some("new-at"),
        "access_token must be rotated to the fresh value"
    );
}

/// Mint an unsigned JWT-shaped token whose payload carries the claim
/// `https://api.openai.com/auth` → `chatgpt_account_id`. The real parser
/// doesn't verify signatures — it just base64-decodes the middle segment.
fn mint_chatgpt_access_token(account_id: &str) -> String {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
    let header = B64.encode(r#"{"alg":"none"}"#);
    let payload = serde_json::json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": account_id},
    });
    let payload_b64 = B64.encode(payload.to_string());
    format!("{header}.{payload_b64}.signature-unused")
}

async fn spawn_exchange_code_idp(access_token: &str) -> String {
    // Build the JSON body once so we can move it into the handler closure.
    let body = format!(
        r#"{{"access_token":"{access_token}","refresh_token":"rt-new","token_type":"Bearer","expires_in":3600}}"#
    );
    let body = Arc::new(body);
    let app = Router::new().route(
        "/token",
        post({
            let body = Arc::clone(&body);
            move || {
                let body = Arc::clone(&body);
                async move {
                    Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from((*body).clone()))
                        .unwrap()
                }
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/token")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn exchange_code_extracts_chatgpt_account_id_via_complete_manual(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let account_id = "chatgpt-acct-xyz-42";
    let access_token = mint_chatgpt_access_token(account_id);

    let token_url = spawn_exchange_code_idp(&access_token).await;
    let provider = provider_with_token_url(token_url);
    let store = Arc::new(MemoryStateStore::default());
    let service = CliAuthProxyService::new(
        vec![provider],
        CliCredentialRepository::new(pool.clone()),
        Some(TEST_KEY),
        StateStore::Memory(store),
        2,
    );

    let url = service.authorize(&common::scope_for(user), "openai").await.unwrap();
    let state = extract_state_from_url(&url);
    service
        .complete_manual(&common::scope_for(user), "openai", &format!("code=abc&state={state}"))
        .await
        .expect("exchange + store must succeed");

    // Reach into stored ciphertext and assert account_id baked into auth.json.
    let repo = CliCredentialRepository::new(pool.clone());
    let stored = repo.find_encrypted(&common::scope_for(user), "codex").await.unwrap().expect("stored");
    let plaintext = crypto::decrypt_base64(&TEST_KEY, &stored).unwrap();
    let file_map: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
    let auth: serde_json::Value = serde_json::from_str(file_map["auth.json"].as_str().unwrap()).unwrap();
    assert_eq!(
        auth["tokens"]["account_id"].as_str(),
        Some(account_id),
        "chatgpt_account_id must be extracted from the access_token JWT and stored"
    );
}
