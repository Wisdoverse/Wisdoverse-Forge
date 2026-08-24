//! Runtime tests for team invites: creation, token lookup, membership grant,
//! acceptance, and the email-match rule.

use crate::services::resource_member::{InviteOutcome, ResourceMemberService};
use crate::test_support::tenant_scope_for_ids;
use agentforge_core::TeamId;
use uuid::Uuid;

async fn seed_org_team_user(pool: &sqlx::PgPool) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let org = Uuid::new_v4();
    let team = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Invite Org', $2)")
        .bind(org)
        .bind(format!("invite-org-{org}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind("owner@example.com")
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Team', $3)")
        .bind(team)
        .bind(org)
        .bind(format!("team-{team}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(user)
        .execute(pool)
        .await
        .expect("seed membership");
    (org, team, user)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn invite_missing_user_creates_pending_and_redeem_grants_memberships(pool: sqlx::PgPool) {
    let (org, team, owner) = seed_org_team_user(&pool).await;
    let scope = tenant_scope_for_ids(org, owner);
    let service = ResourceMemberService::new(
        crate::repositories::resource::member::ResourceMemberRepository::new(pool.clone()),
        crate::repositories::resource::permission::ResourcePermissionRepository::new(pool.clone()),
        crate::repositories::resource::invite::TeamInviteRepository::new(pool.clone()),
    );

    // No account for the invited email: a pending invite with a link is created.
    let outcome = service
        .invite_team_member_by_email(
            &scope,
            org,
            TeamId::from(team),
            "new@example.com",
            Some("member"),
            Some("https://forge.example.com"),
        )
        .await
        .expect("invite");
    let invite_url = match outcome {
        InviteOutcome::Invited { invite_url } => invite_url,
        InviteOutcome::Added(_) => panic!("expected pending invite"),
    };
    assert!(invite_url.starts_with("https://forge.example.com/login?invite="));
    let token = invite_url.rsplit('=').next().expect("token");

    // The invited person registers and redeems with matching email.
    let invitee = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(invitee)
        .bind("new@example.com")
        .execute(&pool)
        .await
        .expect("seed invitee");
    let invitee_row = sqlx::query_as::<_, agentforge_db::entities::User>("SELECT * FROM users WHERE id = $1")
        .bind(invitee)
        .fetch_one(&pool)
        .await
        .expect("fetch invitee");
    let result = service.redeem_team_invite(token, &invitee_row).await.expect("redeem");
    assert!(result["orgId"].as_str().is_some(), "orgId serializes as a string");
    assert!(result["teamId"].is_string());

    let org_role: String =
        sqlx::query_scalar("SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2")
            .bind(org)
            .bind(invitee)
            .fetch_one(&pool)
            .await
            .expect("read org role");
    assert_eq!(org_role, "member", "redeem grants org membership");
    let team_role: String = sqlx::query_scalar("SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team)
        .bind(invitee)
        .fetch_one(&pool)
        .await
        .expect("read team role");
    assert_eq!(team_role, "member", "redeem grants team membership");

    // One-time: a second redeem with the same token fails.
    let second = service.redeem_team_invite(token, &invitee_row).await;
    assert!(second.is_err(), "invite is single-use");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn redeem_rejects_a_different_email(pool: sqlx::PgPool) {
    let (org, team, owner) = seed_org_team_user(&pool).await;
    let scope = tenant_scope_for_ids(org, owner);
    let service = ResourceMemberService::new(
        crate::repositories::resource::member::ResourceMemberRepository::new(pool.clone()),
        crate::repositories::resource::permission::ResourcePermissionRepository::new(pool.clone()),
        crate::repositories::resource::invite::TeamInviteRepository::new(pool.clone()),
    );
    let outcome = service
        .invite_team_member_by_email(
            &scope,
            org,
            TeamId::from(team),
            "someone@example.com",
            None,
            Some("https://forge.example.com"),
        )
        .await
        .expect("invite");
    let invite_url = match outcome {
        InviteOutcome::Invited { invite_url } => invite_url,
        _ => panic!("expected pending invite"),
    };
    let token = invite_url.rsplit('=').next().expect("token");

    let intruder = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(intruder)
        .bind("intruder@example.com")
        .execute(&pool)
        .await
        .expect("seed intruder");
    let intruder_row = sqlx::query_as::<_, agentforge_db::entities::User>("SELECT * FROM users WHERE id = $1")
        .bind(intruder)
        .fetch_one(&pool)
        .await
        .expect("fetch intruder");

    let result = service.redeem_team_invite(token, &intruder_row).await;
    assert!(result.is_err(), "invite only matches the invited email");
    let pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM team_invites WHERE email = 'someone@example.com' AND accepted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count pending");
    assert_eq!(pending, 1, "invite stays pending after a rejected redeem");
}
