//! Resource permission regression coverage for team/project management.

use sqlx::PgPool;
use uuid::Uuid;

use agentforge_api::repositories::resource::member::ResourceMemberRepository;
use agentforge_api::repositories::resource::permission::ResourcePermissionRepository;
use agentforge_api::routes::legacy_navigation::test_only::{
    list_projects_canonical_for_test, list_teams_canonical_for_test,
};
use agentforge_api::services::resource_member::ResourceMemberService;
use agentforge_api::services::resource_permission::ResourcePermissionService;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{ErrorKind, ProjectId, TeamId, TenantScope};

async fn seed_org(pool: &PgPool) -> Uuid {
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
    org_id
}

async fn seed_user(pool: &PgPool, org_id: Uuid, role: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed org membership");
    user_id
}

async fn seed_team(pool: &PgPool, org_id: Uuid) -> Uuid {
    let team_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.teams (id, organization_id, name, slug) VALUES ($1, $2, 'Engineering', 'engineering')",
    )
    .bind(team_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed team");
    team_id
}

async fn seed_project(pool: &PgPool, org_id: Uuid, team_id: Uuid) -> Uuid {
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $2, $3, 'Wisdoverse Forge', 'agentforge')",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(team_id)
    .execute(pool)
    .await
    .expect("seed project");
    project_id
}

fn scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn org_member_is_read_only_until_granted_team_or_project_role(pool: PgPool) {
    let org_id = seed_org(&pool).await;
    let owner_id = seed_user(&pool, org_id, "owner").await;
    let member_id = seed_user(&pool, org_id, "member").await;
    let team_id = seed_team(&pool, org_id).await;
    let project_id = seed_project(&pool, org_id, team_id).await;

    let permission = ResourcePermissionService::new(ResourcePermissionRepository::new(pool.clone()));
    permission
        .require_team_manager(&scope(org_id, owner_id), TeamId::from(team_id))
        .await
        .expect("org owner can manage team");

    let other_org_id = seed_org(&pool).await;
    let other_team_id = seed_team(&pool, other_org_id).await;
    let other_project_id = seed_project(&pool, other_org_id, other_team_id).await;
    let cross_team_err = permission
        .require_project_creator(&scope(org_id, owner_id), TeamId::from(other_team_id))
        .await
        .expect_err("org owner cannot create projects in another org's team");
    assert!(matches!(cross_team_err.kind, ErrorKind::Forbidden(_)));
    let cross_project_err = permission
        .require_project_manager(&scope(org_id, owner_id), ProjectId::from(other_project_id))
        .await
        .expect_err("org owner cannot manage another org's project");
    assert!(matches!(cross_project_err.kind, ErrorKind::Forbidden(_)));

    let err = permission
        .require_team_manager(&scope(org_id, member_id), TeamId::from(team_id))
        .await
        .expect_err("plain org member cannot manage team");
    assert!(matches!(err.kind, ErrorKind::Forbidden(_)));

    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(team_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .expect("grant team admin");
    permission
        .require_team_manager(&scope(org_id, member_id), TeamId::from(team_id))
        .await
        .expect("team admin can manage team");

    let project_member_id = seed_user(&pool, org_id, "member").await;
    let project_err = permission
        .require_project_manager(&scope(org_id, project_member_id), ProjectId::from(project_id))
        .await
        .expect_err("plain org member cannot manage project");
    assert!(matches!(project_err.kind, ErrorKind::Forbidden(_)));

    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(project_id)
        .bind(project_member_id)
        .execute(&pool)
        .await
        .expect("grant project maintainer");
    permission
        .require_project_manager(&scope(org_id, project_member_id), ProjectId::from(project_id))
        .await
        .expect("project maintainer can manage project");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn nav_read_model_exposes_management_capabilities(pool: PgPool) {
    let org_id = seed_org(&pool).await;
    let owner_id = seed_user(&pool, org_id, "owner").await;
    let member_id = seed_user(&pool, org_id, "member").await;
    let team_id = seed_team(&pool, org_id).await;
    let project_id = seed_project(&pool, org_id, team_id).await;

    let owner_teams = list_teams_canonical_for_test(&pool, owner_id, org_id).await.expect("owner teams");
    assert_eq!(owner_teams[0]["canManage"], true);
    assert_eq!(owner_teams[0]["canDelete"], true);
    assert_eq!(owner_teams[0]["canCreateProject"], true);

    let member_teams = list_teams_canonical_for_test(&pool, member_id, org_id).await.expect("member teams");
    assert_eq!(member_teams[0]["canManage"], false);
    assert_eq!(member_teams[0]["canDelete"], false);
    assert_eq!(member_teams[0]["canCreateProject"], false);

    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(team_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .expect("grant team maintainer");
    let team_maintainer_teams =
        list_teams_canonical_for_test(&pool, member_id, org_id).await.expect("team maintainer teams");
    assert_eq!(team_maintainer_teams[0]["canManage"], false);
    assert_eq!(team_maintainer_teams[0]["canDelete"], false);
    assert_eq!(team_maintainer_teams[0]["canCreateProject"], true);

    let member_projects = list_projects_canonical_for_test(&pool, member_id, team_id).await.expect("member projects");
    assert_eq!(member_projects[0]["id"], serde_json::json!(project_id.to_string()));
    assert_eq!(member_projects[0]["canManage"], true);
    assert_eq!(member_projects[0]["canDelete"], true);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn team_and_project_member_management_requires_resource_manager(pool: PgPool) {
    let org_id = seed_org(&pool).await;
    let owner_id = seed_user(&pool, org_id, "owner").await;
    let member_id = seed_user(&pool, org_id, "member").await;
    let other_member_id = seed_user(&pool, org_id, "member").await;
    let team_id = seed_team(&pool, org_id).await;
    let project_id = seed_project(&pool, org_id, team_id).await;

    let service = ResourceMemberService::new(
        ResourceMemberRepository::new(pool.clone()),
        ResourcePermissionRepository::new(pool.clone()),
    );

    let forbidden = service
        .add_team_member(&scope(org_id, member_id), org_id, TeamId::from(team_id), other_member_id, Some("maintainer"))
        .await
        .expect_err("plain org member cannot grant team membership");
    assert!(matches!(forbidden.kind, ErrorKind::Forbidden(_)));

    let team_member = service
        .add_team_member(&scope(org_id, owner_id), org_id, TeamId::from(team_id), member_id, Some("editor"))
        .await
        .expect("owner can add team member");
    assert_eq!(team_member.user_id, member_id);
    assert_eq!(team_member.role, "maintainer");

    let team_members = service
        .list_team_members(&scope(org_id, owner_id), org_id, TeamId::from(team_id))
        .await
        .expect("owner can list team members");
    assert_eq!(team_members.len(), 1);

    let project_member = service
        .add_project_member(&scope(org_id, owner_id), ProjectId::from(project_id), other_member_id, Some("viewer"))
        .await
        .expect("owner can add project member");
    assert_eq!(project_member.user_id, other_member_id);
    assert_eq!(project_member.role, "member");

    let project_member = service
        .update_project_member(&scope(org_id, owner_id), ProjectId::from(project_id), other_member_id, "maintainer")
        .await
        .expect("owner can update project member");
    assert_eq!(project_member.role, "maintainer");

    service
        .remove_project_member(&scope(org_id, owner_id), ProjectId::from(project_id), other_member_id)
        .await
        .expect("owner can remove project member");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn team_and_project_member_role_constraints_match_permission_model(pool: PgPool) {
    let org_id = seed_org(&pool).await;
    let team_id = seed_team(&pool, org_id).await;
    let project_id = seed_project(&pool, org_id, team_id).await;

    for role in ["owner", "admin", "maintainer", "member"] {
        let team_user_id = seed_user(&pool, org_id, "member").await;
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(team_id)
            .bind(team_user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("canonical team member role is accepted");

        let project_user_id = seed_user(&pool, org_id, "member").await;
        sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind(project_user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("canonical project member role is accepted");
    }

    let definitions = sqlx::query_scalar::<_, String>(
        r#"SELECT string_agg(c.conname || '=' || pg_get_constraintdef(c.oid), E'\n' ORDER BY c.conname)
             FROM pg_constraint c
             JOIN pg_class t ON t.oid = c.conrelid
            WHERE t.relname IN ('team_members', 'project_members')
              AND c.conname IN ('team_members_role_check', 'project_members_role_check')"#,
    )
    .fetch_one(&pool)
    .await
    .expect("role constraints exist");

    assert!(definitions.contains("owner"));
    assert!(definitions.contains("admin"));
    assert!(definitions.contains("maintainer"));
    assert!(definitions.contains("member"));
    assert!(!definitions.contains("editor"));
    assert!(!definitions.contains("viewer"));
}
