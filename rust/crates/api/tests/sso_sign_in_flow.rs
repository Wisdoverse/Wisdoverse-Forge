//! Integration tests for the SSO (OIDC) flow: authorize → callback → exchange.
//!
//! A tiny axum IdP doubles as discovery + token + userinfo endpoints so the
//! full callback path runs against real HTTP, and the DB checks prove first-
//! login provisioning and single-use code enforcement.

use std::sync::Arc;

use agentforge_api::services::sso::{SsoMemoryStateStore, SsoService, SsoStateStore};
use agentforge_api::services::user::UserService;
use agentforge_auth::JwtManager;
use agentforge_core::AppConfig;
use agentforge_core::config::SsoConfig;
use axum::{
    Router,
    routing::{get, post},
};
use secrecy::SecretString;
use serde_json::json;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

mod common;

const TEST_SECRET: &str = "sso-test-secret-key-that-is-32-bytes-long!!";

struct Idp {
    base: String,
    token_hits: Arc<std::sync::atomic::AtomicUsize>,
    userinfo_hits: Arc<std::sync::atomic::AtomicUsize>,
}

/// Discovery document served at /.well-known/openid-configuration.
async fn spawn_idp(groups: &[&str]) -> Idp {
    spawn_idp_with_verified_email(groups, true).await
}

async fn spawn_idp_with_verified_email(groups: &[&str], email_verified: bool) -> Idp {
    let token_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let userinfo_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let token_counter = token_hits.clone();
    let userinfo_counter = userinfo_hits.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind idp");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let cluster = base.clone();
    let groups_value: Vec<String> = groups.iter().map(|group| group.to_string()).collect();
    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || {
                let endpoints = cluster.clone();
                async move {
                    let value = serde_json::to_string(&json!({
                        "authorization_endpoint": format!("{endpoints}/authorize"),
                        "token_endpoint": format!("{endpoints}/token"),
                        "userinfo_endpoint": format!("{endpoints}/userinfo"),
                    }))
                    .expect("discovery serializes");
                    axum::response::Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(value))
                        .unwrap()
                }
            }),
        )
        .route(
            "/token",
            post(move |body: axum::body::Bytes| {
                let counter = token_counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let _ = body;
                    axum::response::Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(r#"{"access_token":"idp-token-123"}"#))
                        .unwrap()
                }
            }),
        )
        .route(
            "/userinfo",
            get(move |headers: axum::http::HeaderMap| {
                let counter = userinfo_counter.clone();
                let groups_value = groups_value.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                    let body = if auth == "Bearer idp-token-123" {
                        let mut value = json!({
                            "email":"sso-user@example.com",
                            "email_verified": email_verified,
                            "name":"SSO User"
                        });
                        value["groups"] = serde_json::json!(groups_value);
                        value.to_string()
                    } else {
                        r#"{"error":"invalid_token"}"#.to_string()
                    };
                    axum::response::Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap()
                }
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve idp");
    });
    Idp { base, token_hits, userinfo_hits }
}

fn sso_config(idp_base: &str) -> SsoConfig {
    SsoConfig {
        enabled: true,
        oidc_discovery_url: Some(format!("{idp_base}/.well-known/openid-configuration")),
        oidc_client_id: Some("forge-client".to_string()),
        oidc_client_secret: Some(SecretString::from("forge-secret".to_string())),
        oidc_scopes: "openid profile email".to_string(),
        display_name: None,
        spa_base_url: Some("http://localhost:4002".to_string()),
        role_claim: None,
        admin_groups: None,
        org_group_map: None,
        team_group_map: None,
        deprovision: false,
        deprovision_token: None,
    }
}

fn user_service(pool: &PgPool) -> UserService {
    UserService::new(
        agentforge_api::repositories::user::UserRepository::new(pool.clone()),
        Arc::new(JwtManager::new(TEST_SECRET, 3600)),
    )
}

fn sso_service(_pool: &PgPool, config: AppConfig) -> SsoService {
    SsoService::new(Arc::new(config), SsoStateStore::Memory(Arc::new(SsoMemoryStateStore::new())))
}

async fn seed_platform_admin(pool: &PgPool) {
    sqlx::query("INSERT INTO users (email, is_admin) VALUES ('admin@example.com', TRUE)")
        .execute(pool)
        .await
        .expect("seed platform admin");
}

fn base_config(database_url: &str, idp_base: &str) -> AppConfig {
    let mut config = agentforge_api::test_support::test_app_config(database_url);
    config.auth_sso = sso_config(idp_base);
    config
}

#[sqlx::test(migrations = "../db/migrations")]
async fn authorize_builds_provider_url(_pool: PgPool) {
    let idp = spawn_idp(&[]).await;
    let config = base_config("postgres://unused", &idp.base);
    let service = sso_service(&_pool, config);
    let (url, _state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize url");
    assert!(url.contains("/authorize?"), "url: {url}");
    assert!(url.contains("client_id=forge-client"), "url: {url}");
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A4002"), "url: {url}");
    assert!(url.contains("scope=openid%20profile%20email"), "url: {url}");
    assert!(url.contains("state="), "url: {url}");
    idp.token_hits.load(std::sync::atomic::Ordering::SeqCst);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_exchanges_and_provisions_then_exchanges_signs_in(pool: PgPool) {
    let idp = spawn_idp(&[]).await;
    seed_platform_admin(&pool).await;
    let memory = Arc::new(SsoMemoryStateStore::new());
    let service =
        SsoService::new(Arc::new(base_config("postgres://unused", &idp.base)), SsoStateStore::Memory(memory.clone()));
    let exchange_service =
        SsoService::new(Arc::new(base_config("postgres://unused", &idp.base)), SsoStateStore::Memory(memory));
    let user_service = user_service(&pool);

    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;

    let redirect = service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await
        .expect("callback");
    assert!(redirect.starts_with("http://localhost:4002/login?auth_code="), "redirect: {redirect}");

    // First login provisions the SSO user (no password) and its default org.
    let user = user_service
        .ensure_sso_user("sso-user@example.com", Some("SSO User"), false)
        .await
        .expect("reprovision idempotent");
    assert_eq!(user.email, "sso-user@example.com");
    assert!(user.password_hash.is_none(), "SSO users have no password");

    // Redeem the one-time code once.
    let code = redirect.split("auth_code=").nth(1).expect("auth code").to_string();
    assert!(JwtManager::new(TEST_SECRET, 3600).verify_token(&code).is_err(), "exchange code must not be a bearer JWT");
    let result = exchange_service.exchange(&code, &user_service).await.expect("exchange across service instances");
    assert_eq!(result.user.email, "sso-user@example.com");
    let claims = service_verify(&result.access_token);
    assert_eq!(claims.sub, user.id.as_uuid());

    // The same code cannot be redeemed twice.
    let second = service.exchange(&code, &user_service).await;
    assert!(second.is_err(), "code must be single-use");

    // A code minted before deprovisioning cannot revive the retained owner
    // membership into a new access/refresh session.
    let (_, stale_state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize again");
    let stale_redirect = service
        .handle_callback(
            "auth-code-2",
            &stale_state,
            Some(&stale_state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await
        .expect("second callback");
    let stale_code = stale_redirect.split("auth_code=").nth(1).expect("stale auth code").to_string();
    let (found, _) = user_service.deprovision_user("sso-user@example.com").await.expect("deprovision");
    assert!(found);
    assert!(
        exchange_service.exchange(&stale_code, &user_service).await.is_err(),
        "a code issued before the session floor must be rejected"
    );

    assert_eq!(idp.token_hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(idp.userinfo_hits.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_rejects_unverified_email(pool: PgPool) {
    let idp = spawn_idp_with_verified_email(&[], false).await;
    let service = sso_service(&pool, base_config("postgres://unused", &idp.base));
    let users = user_service(&pool);
    let (_, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let result = service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &users,
        )
        .await;
    assert!(result.is_err(), "unverified provider email must not create a session");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_rejects_mismatched_state(pool: PgPool) {
    let idp = spawn_idp(&[]).await;
    let config = base_config("postgres://unused", &idp.base);
    let service = sso_service(&pool, config);
    let user_service = user_service(&pool);

    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;

    let rejected = service
        .handle_callback(
            "auth-code-1",
            &state,
            None,
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await;
    assert!(rejected.is_err(), "cookie/state mismatch must reject");
    assert_eq!(idp.token_hits.load(std::sync::atomic::Ordering::SeqCst), 0, "no provider call before state validation");
}

fn service_verify(token: &str) -> agentforge_auth::claims::Claims {
    JwtManager::new(TEST_SECRET, 3600).verify_token(token).expect("token verifies")
}

fn sso_config_with_role_mapping(idp_base: &str) -> SsoConfig {
    let mut config = sso_config(idp_base);
    config.role_claim = Some("groups".to_string());
    config.admin_groups = Some("forge-admins,admins".to_string());
    config
}

fn base_config_with_role_mapping(database_url: &str, idp_base: &str) -> AppConfig {
    let mut config = agentforge_api::test_support::test_app_config(database_url);
    config.auth_sso = sso_config_with_role_mapping(idp_base);
    config
}

async fn seed_member_with_role(pool: &PgPool, role: &str) -> (uuid::Uuid, uuid::Uuid) {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, email_domain) VALUES ($1, 'Sso Org', $2, 'example.com')")
        .bind(org)
        .bind(format!("sso-org-{org}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind("sso-user@example.com")
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(user)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed member");
    (org, user)
}

async fn membership_role(pool: &PgPool, org: uuid::Uuid, user: uuid::Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .fetch_one(pool)
        .await
        .expect("read role")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_upgrades_member_in_admin_group(pool: PgPool) {
    let idp = spawn_idp(&["forge-admins"]).await;
    let config = base_config_with_role_mapping("postgres://unused", &idp.base);
    let service = sso_service(&pool, config);
    let user_service = user_service(&pool);
    let (org, user) = seed_member_with_role(&pool, "member").await;

    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;
    service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await
        .expect("callback");

    assert_eq!(membership_role(&pool, org, user).await, "admin", "member in an admin group is upgraded");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_preserves_member_without_admin_group(pool: PgPool) {
    let idp = spawn_idp(&["engineering"]).await;
    let config = base_config_with_role_mapping("postgres://unused", &idp.base);
    let service = sso_service(&pool, config);
    let user_service = user_service(&pool);
    let (org, user) = seed_member_with_role(&pool, "member").await;

    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;
    service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await
        .expect("callback");

    assert_eq!(membership_role(&pool, org, user).await, "member", "no admin group -> no change");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_never_touches_owner(pool: PgPool) {
    let idp = spawn_idp(&["forge-admins"]).await;
    let config = base_config_with_role_mapping("postgres://unused", &idp.base);
    let service = sso_service(&pool, config);
    let user_service = user_service(&pool);
    let (org, user) = seed_member_with_role(&pool, "owner").await;

    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;
    service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &user_service,
        )
        .await
        .expect("callback");

    assert_eq!(membership_role(&pool, org, user).await, "owner", "SSO must never touch an owner");
}
