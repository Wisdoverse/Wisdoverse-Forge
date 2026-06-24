//! Email-domain org membership must fail closed until ownership is verified.

use agentforge_api::repositories::user::UserRepository;
use agentforge_core::UserId;
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org(pool: &PgPool, slug: &str, email_domain: Option<&str>) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, email_domain) VALUES ($1, $2, $3, $4)")
        .bind(org_id)
        .bind(slug)
        .bind(slug)
        .bind(email_domain)
        .execute(pool)
        .await
        .expect("seed org");
    org_id
}

async fn seed_user_with_personal_org(pool: &PgPool, email: &str) -> (Uuid, Uuid) {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(email)
        .execute(pool)
        .await
        .expect("seed user");
    let personal_org = seed_org(pool, &format!("personal-{user_id}"), None).await;
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(personal_org)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");
    (user_id, personal_org)
}

async fn user_memberships(pool: &PgPool, user_id: Uuid) -> Vec<(Uuid, String, Option<String>, String)> {
    sqlx::query_as(
        r#"SELECT o.id, om.role, o.email_domain, o.slug
           FROM organization_members om
           JOIN organizations o ON o.id = om.organization_id
           WHERE om.user_id = $1
           ORDER BY om.created_at, o.id"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .expect("list memberships")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn registration_does_not_join_existing_domain_org_without_verification(pool: PgPool) {
    let canonical = seed_org(&pool, "acme", Some("acme.com")).await;
    let repo = UserRepository::new(pool.clone());

    let user =
        repo.create("intruder@acme.com", "not-a-real-hash-for-repo-test", Some("Intruder")).await.expect("create user");

    let memberships = user_memberships(&pool, user.id.as_uuid()).await;
    assert_eq!(memberships.len(), 1, "self-signup should receive one personal org only");
    assert_ne!(memberships[0].0, canonical, "self-signup must not join canonical domain org");
    assert_eq!(memberships[0].1, "owner");
    assert_eq!(memberships[0].2, None, "self-signup personal org must not claim email_domain");

    let canonical_membership: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organization_members WHERE organization_id = $1 AND user_id = $2")
            .bind(canonical)
            .bind(user.id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("count canonical membership");
    assert_eq!(canonical_membership, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn login_domain_backfill_is_noop_without_verification(pool: PgPool) {
    let canonical = seed_org(&pool, "acme", Some("acme.com")).await;
    let (user_id, personal_org) = seed_user_with_personal_org(&pool, "legacy@acme.com").await;
    let repo = UserRepository::new(pool.clone());

    repo.ensure_domain_membership(UserId::from(user_id), "legacy@acme.com").await.expect("noop succeeds");

    let memberships = user_memberships(&pool, user_id).await;
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].0, personal_org);
    assert_ne!(memberships[0].0, canonical);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn registration_slug_collision_creates_new_org_instead_of_joining_existing(pool: PgPool) {
    let existing = seed_org(&pool, "sam", Some("other-company.test")).await;
    let repo = UserRepository::new(pool.clone());

    let user = repo.create("sam@gmail.com", "not-a-real-hash-for-repo-test", Some("Sam")).await.expect("create user");

    let memberships = user_memberships(&pool, user.id.as_uuid()).await;
    assert_eq!(memberships.len(), 1);
    assert_ne!(memberships[0].0, existing, "slug collision must not mutate/reuse an existing tenant org");
    assert_eq!(memberships[0].2, None);
    assert!(
        memberships[0].3.starts_with("sam-"),
        "personal org slug should retry with suffix after collision, got {}",
        memberships[0].3
    );

    let existing_member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organization_members WHERE organization_id = $1")
            .bind(existing)
            .fetch_one(&pool)
            .await
            .expect("count existing members");
    assert_eq!(existing_member_count, 0, "existing org must remain untouched");
}
