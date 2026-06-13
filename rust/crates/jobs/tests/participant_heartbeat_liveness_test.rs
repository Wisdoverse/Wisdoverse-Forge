//! Integration coverage for the ADR 0008 heartbeat hot-path contract.
//!
//! A steady-state beat must only refresh `last_heartbeat_at` and must NOT
//! recompute `busy`/`available` (that is maintained event-driven). The one
//! exception is resurrection: a returning `offline` agent that still owns a
//! `working` task must come back `busy`, otherwise it could be double-assigned.
//! The `agents.status` mirror must only be written when the status actually
//! changes.

use agentforge_jobs::apply_participant_heartbeat;
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org_and_user(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
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
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    (org_id, user_id)
}

async fn seed_agent(pool: &PgPool, org_id: Uuid, user_id: Uuid, status: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) \
         VALUES ($1, $2, $2, $3, 'hb-agent', $4::agent_status)",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed agent");
    agent_id
}

async fn seed_participant(pool: &PgPool, org_id: Uuid, agent_id: Uuid, status: &str) {
    sqlx::query(
        r#"INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'hb-agent', ARRAY['codex'], $3, NOW() - INTERVAL '5 minutes')"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed participant");
}

async fn seed_working_task(pool: &PgPool, org_id: Uuid, user_id: Uuid, agent_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
           VALUES ($1, $2, 'in-flight', 'working', $3, $4, 'normal', 1,
                   NOW() + INTERVAL '15 minutes', $5, NOW())"#,
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(Uuid::now_v7())
    .execute(pool)
    .await
    .expect("seed working task");
}

async fn participant_status(pool: &PgPool, agent_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM participants WHERE agent_id = $1")
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .expect("read participant status")
}

async fn last_heartbeat(pool: &PgPool, agent_id: Uuid) -> chrono::DateTime<chrono::Utc> {
    sqlx::query_scalar("SELECT last_heartbeat_at FROM participants WHERE agent_id = $1")
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .expect("read last_heartbeat_at")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn first_seen_heartbeat_inserts_available(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    let agent_id = seed_agent(&pool, org_id, user_id, "idle").await;

    let (participant, _) = apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()])
        .await
        .expect("apply heartbeat")
        .expect("agent row exists");

    assert_eq!(participant.status, "available", "a brand-new participant holds no task");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn first_seen_heartbeat_with_working_task_inserts_busy(pool: PgPool) {
    // Defensive: if a participant row is absent while a working task still
    // references the agent (e.g. a row hard-deleted out from under a live task),
    // the first beat must INSERT 'busy', never wrongly default 'available' and
    // let the auto-dispatcher double-assign the agent.
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    let agent_id = seed_agent(&pool, org_id, user_id, "working").await;
    seed_working_task(&pool, org_id, user_id, agent_id).await;

    let (participant, _) = apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()]).await.unwrap().unwrap();

    assert_eq!(participant.status, "busy", "first beat must derive busy from the live working task");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn steady_state_beat_refreshes_heartbeat_without_recomputing_busy(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    let agent_id = seed_agent(&pool, org_id, user_id, "working").await;
    // Busy participant whose working task has since vanished. The old per-beat
    // recompute would have flipped this to `available`; the new contract leaves
    // busy→available to the event-driven release path, so a steady-state beat
    // must NOT recompute it.
    seed_participant(&pool, org_id, agent_id, "busy").await;
    let before = last_heartbeat(&pool, agent_id).await;

    let (participant, _) = apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()]).await.unwrap().unwrap();

    assert_eq!(participant.status, "busy", "steady-state beat must not recompute busy/available");
    assert!(last_heartbeat(&pool, agent_id).await > before, "last_heartbeat_at must advance");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn resurrection_with_working_task_returns_busy(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    let agent_id = seed_agent(&pool, org_id, user_id, "offline").await;
    seed_participant(&pool, org_id, agent_id, "offline").await;
    // Lease is 900s, far longer than the 90s offline window, so the agent still
    // owns the task when it heartbeats back.
    seed_working_task(&pool, org_id, user_id, agent_id).await;

    let (participant, changed) =
        apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()]).await.unwrap().unwrap();

    assert_eq!(participant.status, "busy", "offline agent that still owns a working task resurrects busy");
    assert!(changed, "agents.status moved offline->working");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn resurrection_without_working_task_returns_available(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    let agent_id = seed_agent(&pool, org_id, user_id, "offline").await;
    seed_participant(&pool, org_id, agent_id, "offline").await;

    let (participant, changed) =
        apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()]).await.unwrap().unwrap();

    assert_eq!(participant.status, "available", "offline agent with no task resurrects available");
    assert!(changed, "agents.status moved offline->idle");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn agents_status_mirror_is_a_noop_when_unchanged(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;
    // Agent already idle, participant already available: a beat changes nothing.
    let agent_id = seed_agent(&pool, org_id, user_id, "idle").await;
    seed_participant(&pool, org_id, agent_id, "available").await;
    let updated_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&pool)
            .await
            .expect("read agent updated_at");

    let (_, changed) = apply_participant_heartbeat(&pool, agent_id, vec!["codex".into()]).await.unwrap().unwrap();

    assert!(!changed, "unchanged status must be a zero-row agents write");
    let updated_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&pool)
            .await
            .expect("read agent updated_at");
    assert_eq!(updated_before, updated_after, "agents.updated_at must not move on an unchanged beat");
    assert_eq!(participant_status(&pool, agent_id).await, "available");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reconcile_releases_busy_participant_with_no_working_task(pool: PgPool) {
    // A participant left busy after its task already left 'working' (a failed
    // best-effort release) is no longer self-healed per beat; the periodic
    // reconcile backstop must flip it back to available. A busy participant that
    // DOES still own a working task must be left alone.
    let (org_id, user_id) = seed_org_and_user(&pool).await;

    let stranded = seed_agent(&pool, org_id, user_id, "working").await;
    seed_participant(&pool, org_id, stranded, "busy").await; // no working task seeded

    let working = seed_agent(&pool, org_id, user_id, "working").await;
    seed_participant(&pool, org_id, working, "busy").await;
    seed_working_task(&pool, org_id, user_id, working).await;

    let released = agentforge_jobs::reconcile_orphaned_busy_participant_rows(&pool).await.expect("reconcile");

    assert_eq!(released.len(), 1, "only the stranded participant is released");
    assert_eq!(released[0].agent_id.as_uuid(), stranded);
    assert_eq!(participant_status(&pool, stranded).await, "available");
    assert_eq!(participant_status(&pool, working).await, "busy", "an actively-working agent is untouched");
    let stranded_agent: String = sqlx::query_scalar("SELECT status::text FROM agents WHERE id = $1")
        .bind(stranded)
        .fetch_one(&pool)
        .await
        .expect("read agent status");
    assert_eq!(stranded_agent, "idle", "agents.status mirror follows the participant back to idle");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn heartbeat_for_unknown_agent_yields_none(pool: PgPool) {
    let outcome =
        apply_participant_heartbeat(&pool, Uuid::new_v4(), vec!["codex".into()]).await.expect("apply heartbeat");
    assert!(outcome.is_none(), "no agent row -> None, not an error");
}
