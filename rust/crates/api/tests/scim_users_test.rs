//! Integration tests for the SCIM 2.0 Users slice (paging, get, delete).
//!
//! Covers: oldest-first paging + totals, lookup by id, SCIM delete =
//! strip memberships + deactivate account (404 afterwards).

use agentforge_api::repositories::user::UserRepository;
use agentforge_api::services::user::UserService;
use agentforge_auth::JwtManager;
use sqlx::PgPool;
use uuid::Uuid;

const TEST_SECRET: &str = "sso-test-secret-key-that-is-32-bytes-long!!";

fn user_service(pool: &PgPool) -> UserService {
    UserService::new(UserRepository::new(pool.clone()), std::sync::Arc::new(JwtManager::new(TEST_SECRET, 3600)))
}

async fn seed_user(pool: &PgPool, email: &str, display: &str) -> Uuid {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(email)
        .bind(display)
        .execute(pool)
        .await
        .expect("seed user");
    user
}

async fn seed_personal_org_owner(pool: &PgPool, user: Uuid) {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org)
        .bind("Personal")
        .bind(format!("personal-{user}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(user)
        .bind("owner")
        .execute(pool)
        .await
        .expect("seed owner");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn scim_page_returns_oldest_first_with_totals(pool: PgPool) {
    let a = seed_user(&pool, "scim-a@example.com", "A").await;
    let b = seed_user(&pool, "scim-b@example.com", "B").await;
    let service = user_service(&pool);

    let total = service.scim_total().await.expect("count");
    assert!(total >= 2);
    let page = service.scim_page(10, 0).await.expect("page");
    let ids: Vec<Uuid> = page.iter().map(|(id, _, _, _)| *id).collect();
    let pos_a = ids.iter().position(|id| *id == a).expect("a in page");
    let pos_b = ids.iter().position(|id| *id == b).expect("b in page");
    assert!(pos_a < pos_b, "oldest first order");
    let (id, email, display, _created) = &page[pos_a];
    assert_eq!(*id, a);
    assert_eq!(email, "scim-a@example.com");
    assert_eq!(display.as_deref(), Some("A"));

    let slice = service.scim_user_by_id(agentforge_core::UserId::from(a)).await.expect("get by id").unwrap();
    assert_eq!(slice.email.as_str(), "scim-a@example.com");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn scim_delete_strips_memberships_and_deactivates_account(pool: PgPool) {
    let user = seed_user(&pool, "scim-del@example.com", "Del").await;
    seed_personal_org_owner(&pool, user).await;
    let team_org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(team_org)
        .bind("Team Org")
        .bind("scim-team-org")
        .execute(&pool)
        .await
        .expect("seed team org");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(team_org)
        .bind(user)
        .bind("member")
        .execute(&pool)
        .await
        .expect("seed membership");

    let service = user_service(&pool);
    let (found, removed) = service.scim_delete_user(agentforge_core::UserId::from(user)).await.expect("delete");
    assert!(found);
    assert_eq!(removed, 1, "team membership stripped");
    let floor: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT sessions_invalid_before FROM users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .expect("session floor");
    assert!(floor.is_some(), "SCIM deletion invalidates renewable sessions");

    assert!(
        service.scim_user_by_id(agentforge_core::UserId::from(user)).await.expect("lookup").is_none(),
        "account deactivated"
    );
    let page = service.scim_page(100, 0).await.expect("page");
    assert!(page.iter().all(|(id, _, _, _)| *id != user), "deactivated user excluded from list");

    // Delete is idempotent at the service level: already deactivated -> not found.
    let (found2, _) = service.scim_delete_user(agentforge_core::UserId::from(user)).await.expect("second delete");
    assert!(!found2);
}
