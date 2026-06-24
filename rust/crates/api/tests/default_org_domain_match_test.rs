//! Default-org selection must prefer the org whose `email_domain` matches
//! the user's own email domain over any other canonical org.
//!
//! Regression coverage for !529 follow-up (Codex review P2). Before the
//! three-tier sort, a user with memberships in multiple canonical orgs
//! (e.g. legacy cross-company invite + their own domain org) would
//! receive whichever org they joined first. The JWT `org` claim and the
//! frontend's `orgs[0]` fallback both read from these two queries, so the
//! wrong-tenant failure mode is immediate: empty sidebar, "Pick a project
//! to get started" on the tasks page.

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::repositories::user::UserRepository;
use agentforge_core::UserId;

// `list_orgs`'s SQL reuses the identical ORDER BY, so `find_default_org`
// coverage is sufficient. A dedicated `list_orgs` integration test is
// blocked by a pre-existing gap: `list_orgs` SELECTs `o.plan` which only
// exists in legacy production schemas, not on a fresh `#[sqlx::test]`
// DB (migration 009 documents this).

async fn seed_user(pool: &PgPool, email: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(email)
        .execute(pool)
        .await
        .expect("seed user");
    user_id
}

/// Seed an org. `email_domain = None` models a personal Space; `Some(d)`
/// models a canonical team org tagged by migration 011.
async fn seed_org(pool: &PgPool, name: &str, email_domain: Option<&str>) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, email_domain) VALUES ($1, $2, $3, $4)")
        .bind(org_id)
        .bind(name)
        .bind(format!("{name}-{org_id}"))
        .bind(email_domain)
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    org_id
}

async fn add_membership(pool: &PgPool, user_id: Uuid, org_id: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO organization_members (organization_id, user_id, role, created_at)
         VALUES ($1, $2, $3, NOW())",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed membership");
}

/// Scenario: user in [personal, own-domain-canonical, foreign-domain-canonical],
/// joined foreign canonical FIRST (earliest created_at among canonical rows).
/// Pre-fix sort tiebroke on created_at and picked the foreign canonical.
/// The three-tier sort must pick the user's own-domain canonical.
#[sqlx::test(migrations = "../db/migrations")]
async fn own_domain_wins_over_foreign_canonical(pool: PgPool) {
    let user = seed_user(&pool, "dev@acme.com").await;
    let personal = seed_org(&pool, "Personal Space", None).await;
    let foreign_canonical = seed_org(&pool, "Beta Team", Some("beta.io")).await;
    let own_canonical = seed_org(&pool, "Acme Team", Some("acme.com")).await;

    // Order matters: foreign joined first, then own. Pre-fix: picks foreign.
    add_membership(&pool, user, personal, "owner").await;
    add_membership(&pool, user, foreign_canonical, "member").await;
    add_membership(&pool, user, own_canonical, "owner").await;

    let repo = UserRepository::new(pool.clone());
    let (default_org, role) =
        repo.find_default_org(UserId::from(user)).await.expect("find_default_org").expect("some default org");
    assert_eq!(default_org, own_canonical, "own-domain canonical must win");
    assert_eq!(role, "owner");
}

/// Scenario: user in [personal, own-domain-canonical]. Trivial case — own
/// canonical preferred over personal. Mirrors !529's original target.
#[sqlx::test(migrations = "../db/migrations")]
async fn own_domain_wins_over_personal(pool: PgPool) {
    let user = seed_user(&pool, "dev@example.com").await;
    let personal = seed_org(&pool, "Personal", None).await;
    let canonical = seed_org(&pool, "Example Team", Some("example.com")).await;
    add_membership(&pool, user, personal, "owner").await;
    add_membership(&pool, user, canonical, "owner").await;

    let repo = UserRepository::new(pool.clone());
    let (default_org, _) = repo.find_default_org(UserId::from(user)).await.expect("find_default_org").expect("some");
    assert_eq!(default_org, canonical);
}

/// Scenario: user on a public domain (gmail) invited to a team. Team has
/// `email_domain='acme.com'`, user's domain is `gmail.com` — no
/// own-domain match exists. The second tier kicks in: any canonical beats
/// personal. The invited team wins over personal Space.
#[sqlx::test(migrations = "../db/migrations")]
async fn public_domain_user_prefers_canonical_over_personal(pool: PgPool) {
    let user = seed_user(&pool, "dev@gmail.com").await;
    let personal = seed_org(&pool, "Personal", None).await;
    let team = seed_org(&pool, "Acme Team", Some("acme.com")).await;
    add_membership(&pool, user, personal, "owner").await;
    add_membership(&pool, user, team, "member").await;

    let repo = UserRepository::new(pool.clone());
    let (default_org, _) = repo.find_default_org(UserId::from(user)).await.expect("find_default_org").expect("some");
    assert_eq!(default_org, team, "any canonical beats personal for public-domain users");
}

/// Scenario: user only has a personal Space (no canonical). Must return it.
#[sqlx::test(migrations = "../db/migrations")]
async fn personal_only_returned_when_no_canonical(pool: PgPool) {
    let user = seed_user(&pool, "dev@gmail.com").await;
    let personal = seed_org(&pool, "Personal", None).await;
    add_membership(&pool, user, personal, "owner").await;

    let repo = UserRepository::new(pool.clone());
    let (default_org, _) = repo.find_default_org(UserId::from(user)).await.expect("find_default_org").expect("some");
    assert_eq!(default_org, personal);
}

/// Scenario: user in two own-domain-matching canonical orgs (pathological
/// edge case — unique partial index on `organizations.email_domain`
/// normally forbids this, but legacy data predating migration 011 could
/// slip through if the unique index is missing on older snapshots). Tie
/// broken by earliest `created_at`. Ensures the sort is stable.
#[sqlx::test(migrations = "../db/migrations")]
async fn ties_broken_by_created_at(pool: PgPool) {
    let user = seed_user(&pool, "dev@acme.com").await;
    let personal = seed_org(&pool, "Personal", None).await;
    let acme_old = seed_org(&pool, "Acme Old", Some("acme.com")).await;

    add_membership(&pool, user, personal, "owner").await;
    add_membership(&pool, user, acme_old, "owner").await;

    let repo = UserRepository::new(pool.clone());
    let (default_org, _) = repo.find_default_org(UserId::from(user)).await.expect("find_default_org").expect("some");
    assert_eq!(default_org, acme_old);
}
