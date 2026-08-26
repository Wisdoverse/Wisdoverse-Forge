//! Integration tests for SSO org provisioning + deprovisioning (group map).
//!
//! Covers mapped membership/role sync, group-loss access denial, and instant
//! deprovisioning.

use std::sync::Arc;

use agentforge_api::services::sso::{SsoMemoryStateStore, SsoService, SsoStateStore};
use agentforge_api::services::user::UserService;
use agentforge_auth::JwtManager;
use agentforge_core::config::SsoConfig;
use agentforge_core::{AppConfig, AppResult};
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
}

/// Minimal OIDC IdP whose userinfo returns `groups` plus a fixed email.
async fn spawn_idp(groups: &[&str]) -> Idp {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind idp");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let groups_value: Vec<String> = groups.iter().map(|g| g.to_string()).collect();

    let discovery_base = base.clone();
    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || {
                let endpoints = discovery_base.clone();
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
            "/userinfo",
            get(move || {
                let groups_value = groups_value.clone();
                async move {
                    let mut value = serde_json::json!({
                        "email": "sso-user@example.com",
                        "email_verified": true,
                        "name": "SSO User"
                    });
                    value["groups"] = serde_json::json!(groups_value);
                    axum::response::Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(value.to_string()))
                        .unwrap()
                }
            }),
        )
        .route(
            "/token",
            post(move || async move {
                axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"access_token":"idp-token-123"}"#))
                    .unwrap()
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve idp");
    });
    Idp { base }
}

fn config_with_provisioning(idp_base: &str, org_map: &str, deprovision: bool) -> AppConfig {
    let mut config = agentforge_api::test_support::test_app_config("postgres://unused");
    config.auth_sso = SsoConfig {
        enabled: true,
        oidc_discovery_url: Some(format!("{idp_base}/.well-known/openid-configuration")),
        oidc_client_id: Some("forge-client".to_string()),
        oidc_client_secret: Some(SecretString::from("forge-secret".to_string())),
        oidc_scopes: "openid profile email".to_string(),
        display_name: None,
        spa_base_url: Some("http://localhost:4002".to_string()),
        role_claim: Some("groups".to_string()),
        admin_groups: Some("forge-admins".to_string()),
        org_group_map: Some(org_map.to_string()),
        team_group_map: None,
        deprovision,
        deprovision_token: None,
    };
    config
}

fn sso_service(_pool: &PgPool, config: AppConfig) -> SsoService {
    SsoService::new(Arc::new(config), SsoStateStore::Memory(Arc::new(SsoMemoryStateStore::new())))
}

fn user_service(pool: &PgPool) -> UserService {
    UserService::new(
        agentforge_api::repositories::user::UserRepository::new(pool.clone()),
        Arc::new(JwtManager::new(TEST_SECRET, 3600)),
    )
}

async fn seed_org_by_slug(pool: &PgPool, slug: &str) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Mapped Org', $2)")
        .bind(org)
        .bind(slug)
        .execute(pool)
        .await
        .expect("seed org");
    org
}

async fn membership_role(pool: &PgPool, org: Uuid, user: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .fetch_optional(pool)
        .await
        .expect("read role")
}

async fn seed_user(pool: &PgPool) -> Uuid {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind("sso-user@example.com")
        .execute(pool)
        .await
        .expect("seed user");
    user
}

async fn seed_platform_admin(pool: &PgPool) {
    sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, 'platform-admin@example.com', true)")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .expect("seed platform admin");
}

async fn seed_personal_org_owner(pool: &PgPool, user: Uuid) {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Personal', $2)")
        .bind(org)
        .bind(format!("personal-{user}"))
        .execute(pool)
        .await
        .expect("seed personal org");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(user)
        .execute(pool)
        .await
        .expect("seed owner");
}

async fn run_callback(pool: &PgPool, _idp: &Idp, config: AppConfig) {
    run_callback_result(pool, config).await.expect("callback");
}

async fn run_callback_result(pool: &PgPool, config: AppConfig) -> AppResult<String> {
    let service = sso_service(pool, config);
    let users = user_service(pool);
    let (url, state) =
        service.authorize_url("http://localhost:4002/api/v1/auth/sso/oidc/callback").await.expect("authorize");
    let _ = url;
    service
        .handle_callback(
            "auth-code-1",
            &state,
            Some(&state),
            "http://localhost:4002/api/v1/auth/sso/oidc/callback",
            &users,
        )
        .await
}

#[sqlx::test(migrations = "../db/migrations")]
async fn admin_group_cannot_bootstrap_platform_admin(pool: PgPool) {
    let idp = spawn_idp(&["team-apps", "forge-admins"]).await;
    seed_org_by_slug(&pool, "team-org").await;
    let config = config_with_provisioning(&idp.base, "team-org=team-apps", false);

    run_callback_result(&pool, config).await.expect_err("IdP groups cannot authorize global bootstrap");
    let user = agentforge_api::repositories::user::UserRepository::new(pool.clone())
        .find_by_email("sso-user@example.com")
        .await
        .expect("find user");
    assert!(user.is_none(), "failed bootstrap must not persist an account");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_provisions_member_into_mapped_org(pool: PgPool) {
    seed_platform_admin(&pool).await;
    let idp = spawn_idp(&["team-apps"]).await;
    let org = seed_org_by_slug(&pool, "team-org").await;
    let config = config_with_provisioning(&idp.base, "team-org=team-apps", false);
    run_callback(&pool, &idp, config).await;

    let user = agentforge_api::repositories::user::UserRepository::new(pool.clone())
        .find_by_email("sso-user@example.com")
        .await
        .expect("find user");
    let user = user.expect("provisioned user");
    assert_eq!(
        membership_role(&pool, org, user.id.as_uuid()).await.as_deref(),
        Some("member"),
        "mapped group provisions the org membership as member"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_provisions_admin_into_mapped_org(pool: PgPool) {
    seed_platform_admin(&pool).await;
    let idp = spawn_idp(&["team-apps", "forge-admins"]).await;
    let org = seed_org_by_slug(&pool, "team-org").await;
    let config = config_with_provisioning(&idp.base, "team-org=team-apps", false);
    run_callback(&pool, &idp, config).await;

    let repo = agentforge_api::repositories::user::UserRepository::new(pool.clone());
    let user = repo.find_by_email("sso-user@example.com").await.expect("find user").expect("provisioned user");
    assert_eq!(
        membership_role(&pool, org, user.id.as_uuid()).await.as_deref(),
        Some("admin"),
        "admin group + mapped group provisions as admin"
    );

    let member_idp = spawn_idp(&["team-apps"]).await;
    let member_config = config_with_provisioning(&member_idp.base, "team-org=team-apps", false);
    run_callback(&pool, &member_idp, member_config).await;
    assert_eq!(
        membership_role(&pool, org, user.id.as_uuid()).await.as_deref(),
        Some("member"),
        "removing the admin group demotes the mapped role"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_denies_group_loss_without_removing_retained_membership(pool: PgPool) {
    let idp = spawn_idp(&["other-group"]).await;
    let org = seed_org_by_slug(&pool, "team-org").await;
    let user = seed_user(&pool).await;
    seed_personal_org_owner(&pool, user).await;
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .expect("seed team membership");

    let config = config_with_provisioning(&idp.base, "team-org=team-apps", true);
    let error = run_callback_result(&pool, config).await.expect_err("missing mapped group must deny sign-in");
    assert!(error.to_string().contains("not assigned"), "unexpected error: {error}");

    assert_eq!(
        membership_role(&pool, org, user).await.as_deref(),
        Some("member"),
        "retaining a membership must not turn access denial into destructive cleanup"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_denies_group_loss_even_when_it_is_the_last_membership(pool: PgPool) {
    let idp = spawn_idp(&["other-group"]).await;
    let org = seed_org_by_slug(&pool, "team-org").await;
    let user = seed_user(&pool).await;
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .expect("seed team membership");

    let config = config_with_provisioning(&idp.base, "team-org=team-apps", true);
    run_callback_result(&pool, config).await.expect_err("retained last membership must not grant sign-in");

    assert_eq!(
        membership_role(&pool, org, user).await.as_deref(),
        Some("member"),
        "the last membership remains stored while sign-in is denied"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn callback_grants_and_deprovisions_mapped_team_membership(pool: PgPool) {
    let idp = spawn_idp(&["team-apps", "forge-admins"]).await;
    let user = seed_user(&pool).await;
    seed_personal_org_owner(&pool, user).await;
    let personal_org: Uuid = sqlx::query_scalar("SELECT id FROM organizations WHERE slug = $1")
        .bind(format!("personal-{user}"))
        .fetch_one(&pool)
        .await
        .expect("personal org");
    let team = Uuid::new_v4();
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Builders', 'builders')")
        .bind(team)
        .bind(personal_org)
        .execute(&pool)
        .await
        .expect("seed team");

    let mut config = config_with_provisioning(&idp.base, "team-org=team-apps", false);
    config.auth_sso.team_group_map = Some("Builders=team-apps".to_string());
    run_callback(&pool, &idp, config).await;

    let role: Option<String> = sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(user)
        .fetch_optional(&pool)
        .await
        .expect("team role");
    assert_eq!(role.as_deref(), Some("admin"), "mapped group + admin group grants team admin");

    let member_idp = spawn_idp(&["team-apps"]).await;
    let mut member_config = config_with_provisioning(&member_idp.base, "team-org=team-apps", false);
    member_config.auth_sso.team_group_map = Some("Builders=team-apps".to_string());
    run_callback(&pool, &member_idp, member_config).await;
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(user)
        .fetch_optional(&pool)
        .await
        .expect("team role after demotion");
    assert_eq!(role.as_deref(), Some("member"), "removing the admin group demotes the team role");

    sqlx::query("UPDATE team_members SET role = 'owner' WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(user)
        .execute(&pool)
        .await
        .expect("promote team owner");
    let owner_idp = spawn_idp(&["team-apps"]).await;
    let mut owner_config = config_with_provisioning(&owner_idp.base, "team-org=team-apps", true);
    owner_config.auth_sso.team_group_map = Some("Builders=builders-group".to_string());
    run_callback(&pool, &owner_idp, owner_config).await;
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(user)
        .fetch_optional(&pool)
        .await
        .expect("team owner after deprovision");
    assert_eq!(role.as_deref(), Some("owner"), "SSO deprovisioning must not remove a team owner");

    // Sign in again without the mapped org group: access is denied before any
    // retained membership can be treated as authorization.
    let idp2 = spawn_idp(&["other-group"]).await;
    let mut config2 = config_with_provisioning(&idp2.base, "team-org=team-apps", true);
    config2.auth_sso.team_group_map = Some("Builders=team-apps".to_string());
    run_callback_result(&pool, config2).await.expect_err("missing mapped group must deny sign-in");

    let role: Option<String> = sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(user)
        .fetch_optional(&pool)
        .await
        .expect("team role after deprovision");
    assert_eq!(role.as_deref(), Some("owner"), "denied sign-in leaves existing membership unchanged");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn instant_deprovision_removes_non_owner_memberships(pool: PgPool) {
    let user = seed_user(&pool).await;
    let personal_org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Personal', $2)")
        .bind(personal_org)
        .bind(format!("personal-{user}"))
        .execute(&pool)
        .await
        .expect("seed personal org");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(personal_org)
        .bind(user)
        .execute(&pool)
        .await
        .expect("seed owner");
    for (slug, role) in [("dep-a", "member"), ("dep-b", "admin")] {
        let org = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Dep', $2)")
            .bind(org)
            .bind(slug)
            .execute(&pool)
            .await
            .expect("seed dep org");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org)
            .bind(user)
            .bind(role)
            .execute(&pool)
            .await
            .expect("seed dep membership");
    }

    let users = user_service(&pool);
    let (found, removed) = users.deprovision_user("sso-user@example.com").await.expect("deprovision");
    assert!(found);
    assert_eq!(removed, 2, "member + admin memberships are revoked");
    assert_eq!(
        membership_role(&pool, personal_org, user).await.as_deref(),
        Some("owner"),
        "owners are never auto-removed"
    );
    let floor: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT sessions_invalid_before FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .expect("session floor");
    assert!(floor.is_some(), "deprovisioning blocks refresh even when an owner row is retained");

    let (found, _) = users.deprovision_user("nobody@example.com").await.expect("unknown email");
    assert!(!found);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn scim_provision_creates_account_and_memberships(pool: PgPool) {
    seed_platform_admin(&pool).await;
    let org = Uuid::new_v4();
    let org_admin = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Scim', 'scim-org'), ($2, 'Scim Admins', 'scim-admins')")
        .bind(org)
        .bind(org_admin)
        .execute(&pool)
        .await
        .expect("seed orgs");
    let users = user_service(&pool);

    let user = users
        .provision_user("scim@example.com", Some("Scim User"), &["scim-org".to_string()], &[])
        .await
        .expect("provision");
    assert_eq!(membership_role(&pool, org, user.id.as_uuid()).await.as_deref(), Some("member"));

    let user2 = users
        .provision_user(
            "scim@example.com",
            None,
            &["scim-admins".to_string(), "nope".to_string()],
            &["admin".to_string()],
        )
        .await
        .expect("provision again");
    assert_eq!(user.id, user2.id, "same account returned");
    assert_eq!(membership_role(&pool, org_admin, user.id.as_uuid()).await.as_deref(), Some("admin"));
    assert_eq!(
        membership_role(&pool, org, user.id.as_uuid()).await.as_deref(),
        Some("member"),
        "existing member is not downgraded"
    );
}
