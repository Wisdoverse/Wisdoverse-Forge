//! Issue #17 P4 regression suite — end-to-end proof that the post-P4
//! canonical-only nav contract still serves the four scenarios the issue
//! tracker enumerates:
//!
//! 1. Old-user-style login preserves the nav tree
//! 2. Multi-organization membership resolves per-tenant
//! 3. Org tree (orgs → teams → projects → groups) loads top-to-bottom
//! 4. Switch-context / refresh preserves the active org
//!
//! Tests drive the production SQL via the `test_only` wrappers in
//! `routes/legacy_navigation.rs` + `routes/groups.rs`, so drift between
//! the canonical helpers and the real handlers fails here before
//! anyone hits the live contract. No Axum / reqwest stack is needed —
//! `#[sqlx::test]` provisions a fresh DB per case, migrations run up to
//! the post-P4 state (027 dropping `legacy.*`) before the test body.

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::repositories::identity::group::GroupRepository;
use agentforge_api::routes::groups::test_only::list_groups_canonical_for_test;
use agentforge_api::routes::legacy_navigation::test_only::{
    list_projects_canonical_for_test, list_teams_canonical_for_test,
};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::ProjectId;

/// Minimum seed: a user with an org membership + a default workspace.
/// Returns `(org_id, user_id)`. Every nav-read helper in this file
/// depends on these rows existing in canonical shape.
async fn seed_user_with_org(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    // `public.projects.workspace_id` has a NOT NULL FK on workspaces.id.
    // The P2 backfill (migration 009) seeded workspaces for every org
    // present at migration time; orgs created post-migration (like the
    // ones we seed per-test) need an explicit workspace row. Convention
    // here: reuse the org_id as the workspace_id for deterministic seeds.
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");
    (org_id, user_id)
}

/// Adds a membership row for an existing user in another org.
async fn add_org_membership(pool: &PgPool, user_id: Uuid, role: &str) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed membership");
    org_id
}

/// Seeds a canonical team under the given org. Slug is required post-MR-B
/// (migration 026 made `teams.slug NOT NULL`), so we pin it here.
async fn seed_team(pool: &PgPool, org_id: Uuid, name: &str, slug: &str) -> Uuid {
    let team_id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.teams (id, organization_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(team_id)
        .bind(org_id)
        .bind(name)
        .bind(slug)
        .execute(pool)
        .await
        .expect("seed team");
    team_id
}

/// Seeds a canonical project under the given team. Both `team_id` and
/// `slug` are NOT NULL post-MR-B, so both are required here.
async fn seed_project(pool: &PgPool, org_id: Uuid, team_id: Uuid, name: &str, slug: &str) -> Uuid {
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $2, $3, $4, $5)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(team_id)
    .bind(name)
    .bind(slug)
    .execute(pool)
    .await
    .expect("seed project");
    project_id
}

/// Seeds a canonical group under the given project. `groups.project_id`
/// stays nullable per ADR 0001; pass `None` to seed a pre-project group.
async fn seed_group(pool: &PgPool, org_id: Uuid, project_id: Option<Uuid>, user_id: Uuid, name: &str) -> Uuid {
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.groups (id, organization_id, project_id, name, created_by)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(group_id)
    .bind(org_id)
    .bind(project_id)
    .bind(name)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed group");
    group_id
}

// ---------------------------------------------------------------------------
// Issue #17 checklist item 1: 老用户登录 E2E (old-user-login nav tree)
// ---------------------------------------------------------------------------
// Simulates a pre-migration user logging in post-P4. Prior to this suite,
// the only assurance was "P2 parity tests passed once"; the live path
// post-P4 had never been exercised end-to-end. This test drives the three
// nav-list queries that the frontend fires immediately after login and
// asserts the canonical contract still holds.
#[sqlx::test(migrations = "../db/migrations")]
async fn old_user_login_loads_full_nav_tree(pool: PgPool) {
    let (org_id, user_id) = seed_user_with_org(&pool).await;
    let team_id = seed_team(&pool, org_id, "Engineering", "engineering").await;
    let project_id = seed_project(&pool, org_id, team_id, "Wisdoverse Forge", "agentforge").await;
    let group_id = seed_group(&pool, org_id, Some(project_id), user_id, "Backend").await;

    // Step 1: list teams for the user's org — mirrors the frontend's
    // first post-login call.
    let teams = list_teams_canonical_for_test(&pool, user_id, org_id).await.expect("list teams");
    assert_eq!(teams.len(), 1, "expected 1 team after login");
    assert_eq!(teams[0]["id"], serde_json::Value::String(team_id.to_string()));
    assert_eq!(teams[0]["name"], "Engineering");
    assert_eq!(teams[0]["slug"], "engineering");

    // Step 2: list projects for the team — second call.
    let projects = list_projects_canonical_for_test(&pool, user_id, team_id).await.expect("list projects");
    assert_eq!(projects.len(), 1, "expected 1 project under the team");
    assert_eq!(projects[0]["id"], serde_json::Value::String(project_id.to_string()));
    assert_eq!(projects[0]["name"], "Wisdoverse Forge");
    assert_eq!(projects[0]["slug"], "agentforge");

    // Step 3: list groups for the project — third call.
    let groups = list_groups_canonical_for_test(&pool, user_id, project_id).await.expect("list groups");
    assert_eq!(groups.len(), 1, "expected 1 group under the project");
    assert_eq!(groups[0]["id"], serde_json::Value::String(group_id.to_string()));
    assert_eq!(groups[0]["name"], "Backend");
}

// ---------------------------------------------------------------------------
// Issue #17 checklist item 2: 多组织切换 E2E (multi-org switch)
// ---------------------------------------------------------------------------
// User belongs to two orgs. Each org lists independently. The nav read
// path MUST NOT leak one org's teams into the other's response — tenant
// isolation is the load-bearing invariant of the whole cutover.
#[sqlx::test(migrations = "../db/migrations")]
async fn multi_org_user_sees_per_tenant_teams_only(pool: PgPool) {
    let (org_a, user_id) = seed_user_with_org(&pool).await;
    let org_b = add_org_membership(&pool, user_id, "member").await;

    let team_a = seed_team(&pool, org_a, "A-Team", "a-team").await;
    let team_b = seed_team(&pool, org_b, "B-Team", "b-team").await;

    let teams_a = list_teams_canonical_for_test(&pool, user_id, org_a).await.expect("list A");
    let teams_b = list_teams_canonical_for_test(&pool, user_id, org_b).await.expect("list B");

    assert_eq!(teams_a.len(), 1, "org A must return only its own team");
    assert_eq!(teams_b.len(), 1, "org B must return only its own team");
    assert_eq!(teams_a[0]["id"], serde_json::Value::String(team_a.to_string()));
    assert_eq!(teams_b[0]["id"], serde_json::Value::String(team_b.to_string()));

    // Negative: the B-team must NOT appear in the A-org list (and vice
    // versa). Explicitly scan for each other's team id so the failure
    // mode is "leaked row from other tenant" rather than generic
    // "length mismatch".
    assert!(
        !teams_a.iter().any(|t| t["id"] == serde_json::json!(team_b.to_string())),
        "cross-tenant leak: org A listing saw org B's team {team_b}"
    );
    assert!(
        !teams_b.iter().any(|t| t["id"] == serde_json::json!(team_a.to_string())),
        "cross-tenant leak: org B listing saw org A's team {team_a}"
    );
}

// ---------------------------------------------------------------------------
// Issue #17 checklist item 3: 组织树/团队/项目/分组加载 E2E
// ---------------------------------------------------------------------------
// Fans out — an org with two teams, each with a different number of
// projects, each with their own groups. Verifies deep-tree traversal
// (teams → projects → groups) doesn't hit any legacy.* path post-P4.
// Migration 027 dropped `legacy.*`, so any lingering FROM legacy.* in
// a handler SQL would fail with `relation "legacy.X" does not exist`
// — this test exercises that loud-fail guarantee.
#[sqlx::test(migrations = "../db/migrations")]
async fn deep_nav_tree_loads_without_touching_legacy_schema(pool: PgPool) {
    let (org_id, user_id) = seed_user_with_org(&pool).await;

    let team_eng = seed_team(&pool, org_id, "Engineering", "engineering").await;
    let team_design = seed_team(&pool, org_id, "Design", "design").await;

    let proj_af = seed_project(&pool, org_id, team_eng, "Wisdoverse Forge", "agentforge").await;
    let proj_infra = seed_project(&pool, org_id, team_eng, "Infra", "infra").await;
    let proj_marketing = seed_project(&pool, org_id, team_design, "Marketing", "marketing").await;

    // Two groups under Wisdoverse Forge; one under Marketing; none under Infra.
    seed_group(&pool, org_id, Some(proj_af), user_id, "Backend").await;
    seed_group(&pool, org_id, Some(proj_af), user_id, "Frontend").await;
    seed_group(&pool, org_id, Some(proj_marketing), user_id, "Brand").await;

    // Orphan group (project_id IS NULL) — ADR 0001 permits this. MUST NOT
    // appear in any project's group list.
    let orphan_group = seed_group(&pool, org_id, None, user_id, "Orphan").await;

    // Teams list: 2 teams.
    let teams = list_teams_canonical_for_test(&pool, user_id, org_id).await.expect("list teams");
    assert_eq!(teams.len(), 2, "deep tree: 2 teams");

    // Projects per team.
    let eng_projects = list_projects_canonical_for_test(&pool, user_id, team_eng).await.expect("eng projects");
    assert_eq!(eng_projects.len(), 2, "Engineering has 2 projects");
    let design_projects = list_projects_canonical_for_test(&pool, user_id, team_design).await.expect("design projects");
    assert_eq!(design_projects.len(), 1, "Design has 1 project");

    // Groups per project.
    let af_groups = list_groups_canonical_for_test(&pool, user_id, proj_af).await.expect("AF groups");
    assert_eq!(af_groups.len(), 2, "Wisdoverse Forge has 2 groups");
    let infra_groups = list_groups_canonical_for_test(&pool, user_id, proj_infra).await.expect("Infra groups");
    assert_eq!(infra_groups.len(), 0, "Infra has 0 groups");
    let marketing_groups =
        list_groups_canonical_for_test(&pool, user_id, proj_marketing).await.expect("Marketing groups");
    assert_eq!(marketing_groups.len(), 1, "Marketing has 1 group");

    // Orphan group does NOT appear in any project-scoped list.
    let all_project_group_ids: Vec<serde_json::Value> =
        af_groups.iter().chain(infra_groups.iter()).chain(marketing_groups.iter()).map(|g| g["id"].clone()).collect();
    assert!(
        !all_project_group_ids.iter().any(|id| id == &serde_json::json!(orphan_group.to_string())),
        "ADR 0001 orphan group {orphan_group} must stay hidden from project-scoped listings"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn empty_project_gets_one_default_task_group(pool: PgPool) {
    let (org_id, user_id) = seed_user_with_org(&pool).await;
    let team_id = seed_team(&pool, org_id, "Engineering", "engineering").await;
    let project_id = seed_project(&pool, org_id, team_id, "Wisdoverse Forge", "agentforge").await;
    let scope = tenant_scope_for_ids(org_id, user_id);
    let repo = GroupRepository::new(pool.clone());

    let group = repo
        .find_or_create_default_for_project(&scope, ProjectId::from(project_id))
        .await
        .expect("create default group");
    assert_eq!(group.name, "Tasks");
    assert_eq!(group.project_id.map(|id| id.as_uuid()), Some(project_id));

    let same_group = repo
        .find_or_create_default_for_project(&scope, ProjectId::from(project_id))
        .await
        .expect("reuse default group");
    assert_eq!(same_group.id, group.id);

    let groups = list_groups_canonical_for_test(&pool, user_id, project_id).await.expect("list groups");
    assert_eq!(groups.len(), 1, "default group should not be duplicated");
    assert_eq!(groups[0]["id"], serde_json::Value::String(group.id.to_string()));
}

// ---------------------------------------------------------------------------
// Issue #17 checklist item 4: 刷新后保留正确 org context
// ---------------------------------------------------------------------------
// "Refresh" at the API layer is the client re-issuing the same
// `list_teams` call with the same (user_id, org_id) tuple. The contract
// must be deterministic — same inputs, same outputs, no per-request
// state drift. This test exercises exactly that: two back-to-back list
// calls in the same test return byte-equal JSON.
#[sqlx::test(migrations = "../db/migrations")]
async fn refresh_same_inputs_yields_same_nav_tree(pool: PgPool) {
    let (org_id, user_id) = seed_user_with_org(&pool).await;
    let team_id = seed_team(&pool, org_id, "Engineering", "engineering").await;
    seed_project(&pool, org_id, team_id, "Wisdoverse Forge", "agentforge").await;

    let first = list_teams_canonical_for_test(&pool, user_id, org_id).await.expect("first call");
    let second = list_teams_canonical_for_test(&pool, user_id, org_id).await.expect("second call");
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap(),
        "list_teams is non-deterministic — refresh would surface different data"
    );

    let first_projects = list_projects_canonical_for_test(&pool, user_id, team_id).await.expect("first projects");
    let second_projects = list_projects_canonical_for_test(&pool, user_id, team_id).await.expect("second projects");
    assert_eq!(
        serde_json::to_value(&first_projects).unwrap(),
        serde_json::to_value(&second_projects).unwrap(),
        "list_projects is non-deterministic"
    );
}

// ---------------------------------------------------------------------------
// Issue #17 checklist item 5 (by proxy): 导航读取不再命中 `legacy.*`
// ---------------------------------------------------------------------------
// Migration 027 drops `legacy.*`. This test proves the constraint: a
// nav-read query post-P4 does not implicitly reference the dropped
// schema. Asserts via negative lookup: `pg_tables` must have zero
// entries for `schemaname = 'legacy'` after all migrations run.
#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_schema_is_physically_absent_post_p4(pool: PgPool) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_tables WHERE schemaname = 'legacy'")
        .fetch_one(&pool)
        .await
        .expect("query pg_tables");
    assert_eq!(count, 0, "legacy.* tables must not exist post-migration-027");

    // And the reconcile function (migration 024) must also be gone.
    let fn_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_proc WHERE proname = 'legacy_nav_reconcile' AND pronamespace = 'public'::regnamespace",
    )
    .fetch_one(&pool)
    .await
    .expect("query pg_proc");
    assert_eq!(fn_count, 0, "public.legacy_nav_reconcile() must not exist post-migration-027");
}
