//! #1324: by-id repository writes carry the organization in their own SQL.
//!
//! Every case seeds one row per organization, calls the write for organization
//! A's row with organization B's scope, and asserts that:
//! - the result is exactly what an id that does not exist produces, and
//! - neither organization's row changed (compared as `to_jsonb(row)::text`).
//!
//! It then calls the same write with organization A's scope and asserts that it
//! succeeds, so the predicate does not break legitimate callers.

use agentforge_api::repositories::agent::AgentRepository;
use agentforge_api::repositories::context_candidate::{
    ContextCandidateRepository, ContextFeedbackRepository, CreateContextFeedbackRecord,
};
use agentforge_api::repositories::identity::GroupRepository;
use agentforge_api::repositories::memory::{MemoryRepository, UpdateMemoryRecord};
use agentforge_api::repositories::orchestration::TaskRunRepository;
use agentforge_api::repositories::skill::{SkillRepository, SkillSnapshot, UpdateSkillRecord};
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::{
    AgentId, AppResult, GroupId, MemoryItemId, ProjectId, ScopedRead, ScopedWrite, TenantScope, UserId, WorkspaceId,
};
use agentforge_db::entities::{OrchestrationTask, Skill};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Tenant {
    org: Uuid,
    user: Uuid,
}

impl Tenant {
    /// The organization's default workspace shares the organization id.
    fn ws(self) -> Uuid {
        self.org
    }

    fn scope(self) -> TenantScope {
        tenant_scope_for_ids_with_axes(self.org, self.user, Some(self.ws()), None, None)
    }

    fn proof(self) -> ScopedRead {
        ScopedRead::from_validated_memberships(
            self.org.into(),
            UserId::from(self.user),
            [WorkspaceId::from(self.ws())],
            [],
            [],
        )
    }

    /// Same organization as `self`, acting as another tenant's user (used to
    /// hit a unique key that does not include the organization).
    fn proof_as(self, user: Uuid) -> ScopedRead {
        ScopedRead::from_validated_memberships(
            self.org.into(),
            UserId::from(user),
            [WorkspaceId::from(self.ws())],
            [],
            [],
        )
    }
}

async fn exec(pool: &PgPool, sql: &str, binds: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    query.execute(pool).await.unwrap_or_else(|err| panic!("seed failed: {sql}: {err}"));
}

async fn seed_tenant(pool: &PgPool) -> Tenant {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(format!("Org {org}"))
        .bind(format!("org-{org}"))
        .execute(pool)
        .await
        .expect("seed org");
    exec(pool, "INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')", &[org]).await;
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind(format!("u-{user}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    exec(
        pool,
        "INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')",
        &[org, user],
    )
    .await;
    Tenant { org, user }
}

async fn tenants(pool: &PgPool) -> (Tenant, Tenant) {
    (seed_tenant(pool).await, seed_tenant(pool).await)
}

/// The full row as JSON text; `None` when the row is gone.
async fn row(pool: &PgPool, table: &str, key: &str, id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>(&format!("SELECT to_jsonb(t)::text FROM {table} t WHERE t.{key} = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("read row snapshot")
}

/// Error text with the target id masked, so "foreign id" and "unknown id"
/// errors compare equal exactly when they are the same error.
fn shape<T: std::fmt::Debug>(result: &AppResult<T>, id: Uuid) -> String {
    match result {
        Ok(value) => format!("Ok({value:?})"),
        Err(err) => format!("Err({})", err.to_string().replace(&id.to_string(), "<id>")),
    }
}

/// Asserts the cross-organization call behaves like an unknown id and leaves
/// both rows untouched.
async fn assert_foreign_is_unknown(
    pool: &PgPool,
    rows: (&str, &str, Uuid, Uuid),
    foreign: String,
    unknown: String,
    before: (Option<String>, Option<String>),
) {
    let (table, key, a_id, b_id) = rows;
    assert_eq!(foreign, unknown, "{table}: a foreign id must look exactly like an unknown id");
    assert_eq!(row(pool, table, key, a_id).await, before.0, "{table}: organization A's row changed");
    assert_eq!(row(pool, table, key, b_id).await, before.1, "{table}: organization B's row changed");
}

// ---------------------------------------------------------------- memory ----

async fn seed_memory(pool: &PgPool, t: Tenant) -> Uuid {
    let id = Uuid::new_v4();
    exec(
        pool,
        "INSERT INTO memory_items (id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id, title, content)
         VALUES ($1, $2, $2, $3, 'user', $3, 'title', 'content')",
        &[id, t.org, t.user],
    )
    .await;
    id
}

fn memory_update() -> UpdateMemoryRecord<'static> {
    UpdateMemoryRecord {
        title: Some("changed"),
        content: None,
        content_redacted: None,
        sensitivity: None,
        provenance: None,
        visibility: None,
        confidence: None,
        last_verified_at: None,
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn memory_writes_are_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let ttl = Some(Utc::now() + Duration::days(7));

    for op in ["update", "extend_ttl", "revoke", "reclassify"] {
        let a_id = seed_memory(&pool, a).await;
        let b_id = seed_memory(&pool, b).await;
        let unknown = Uuid::new_v4();
        let before = (row(&pool, "memory_items", "id", a_id).await, row(&pool, "memory_items", "id", b_id).await);

        let call = |proof: ScopedRead, id: Uuid| {
            let pool = pool.clone();
            async move {
                let mut tx = pool.begin().await.expect("begin");
                let id = MemoryItemId::from(id);
                let result = match op {
                    "update" => MemoryRepository::update_in_tx(&mut tx, &proof, id, memory_update()).await,
                    "extend_ttl" => MemoryRepository::extend_ttl_in_tx(&mut tx, &proof, id, ttl).await,
                    "revoke" => MemoryRepository::revoke_in_tx(&mut tx, &proof, id).await,
                    _ => {
                        // A different target than the seeded user scope, so the row really changes.
                        let project = ProjectId::new();
                        let widened = ScopedRead::from_validated_memberships(
                            proof.org_id(),
                            proof.user_id(),
                            proof.workspace_ids().to_vec(),
                            [],
                            [project],
                        );
                        let target = ScopedWrite::for_project(project, widened).expect("project write scope");
                        MemoryRepository::reclassify_scope_in_tx(&mut tx, &proof, id, &target).await
                    }
                };
                tx.commit().await.expect("commit");
                result.map(|item| item.id)
            }
        };

        let foreign = call(b.proof(), a_id).await;
        let missing = call(b.proof(), unknown).await;
        assert!(foreign.is_err(), "memory {op}: foreign write must fail");
        assert_foreign_is_unknown(
            &pool,
            ("memory_items", "id", a_id, b_id),
            shape(&foreign, a_id),
            shape(&missing, unknown),
            before.clone(),
        )
        .await;

        let own = call(a.proof(), a_id).await.unwrap_or_else(|err| panic!("memory {op}: own write failed: {err}"));
        assert_eq!(own.as_uuid(), a_id);
        assert_ne!(row(&pool, "memory_items", "id", a_id).await, before.0, "memory {op}: own write changed nothing");
        assert_eq!(row(&pool, "memory_items", "id", b_id).await, before.1, "memory {op}: B's row changed");
    }
}

// ----------------------------------------------------------------- skill ----

async fn seed_skill(pool: &PgPool, t: Tenant, state: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO skills (id, organization_id, workspace_id, scope_kind, scope_id, owner_user_id, name, content, state)
         VALUES ($1, $2, $2, 'user', $3, $3, $4, 'content', $5)",
    )
    .bind(id)
    .bind(t.org)
    .bind(t.user)
    .bind(format!("skill-{id}"))
    .bind(state)
    .execute(pool)
    .await
    .expect("seed skill");
    id
}

async fn load_skill(pool: &PgPool, id: Uuid) -> Skill {
    sqlx::query_as::<_, Skill>("SELECT * FROM skills WHERE id = $1").bind(id).fetch_one(pool).await.expect("load skill")
}

fn skill_update() -> UpdateSkillRecord<'static> {
    UpdateSkillRecord {
        name: Some("renamed"),
        description: None,
        trigger_pattern: None,
        content: None,
        enabled: None,
        state: None,
        sensitivity: None,
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn skill_writes_are_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;

    for op in ["update", "revoke", "restore", "promote"] {
        let state = if op == "promote" { "candidate" } else { "active" };
        let a_id = seed_skill(&pool, a, state).await;
        let b_id = seed_skill(&pool, b, state).await;
        let unknown = Uuid::new_v4();
        let snapshot = SkillSnapshot::from(&load_skill(&pool, a_id).await);
        let before = (row(&pool, "skills", "id", a_id).await, row(&pool, "skills", "id", b_id).await);

        let call = |scope: TenantScope, id: Uuid| {
            let pool = pool.clone();
            let snapshot = snapshot.clone();
            async move {
                let mut tx = pool.begin().await.expect("begin");
                let result = match op {
                    "update" => SkillRepository::update_in_tx(&mut tx, &scope, id, skill_update()).await,
                    "revoke" => SkillRepository::revoke_in_tx(&mut tx, &scope, id).await,
                    "restore" => SkillRepository::restore_from_snapshot_in_tx(&mut tx, &scope, id, &snapshot, 7).await,
                    _ => {
                        SkillRepository::promote_candidate_in_tx(
                            &mut tx,
                            &scope,
                            id,
                            "user",
                            scope.user_id().as_uuid(),
                            None,
                            "internal",
                        )
                        .await
                    }
                };
                tx.commit().await.expect("commit");
                result.map(|skill| skill.id.as_uuid())
            }
        };

        let foreign = call(b.scope(), a_id).await;
        let missing = call(b.scope(), unknown).await;
        assert!(foreign.is_err(), "skill {op}: foreign write must fail");
        assert_foreign_is_unknown(
            &pool,
            ("skills", "id", a_id, b_id),
            shape(&foreign, a_id),
            shape(&missing, unknown),
            before.clone(),
        )
        .await;

        call(a.scope(), a_id).await.unwrap_or_else(|err| panic!("skill {op}: own write failed: {err}"));
        assert_ne!(row(&pool, "skills", "id", a_id).await, before.0, "skill {op}: own write changed nothing");
        assert_eq!(row(&pool, "skills", "id", b_id).await, before.1, "skill {op}: B's row changed");
    }
}

// -------------------------------------------------------------- feedback ----

#[sqlx::test(migrations = "../db/migrations")]
async fn context_feedback_item_writes_are_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;

    for op in ["memory_useful", "memory_stale", "memory_needs_review", "memory_revoke", "skill_revoke"] {
        let (table, a_id, b_id) = if op == "skill_revoke" {
            ("skills", seed_skill(&pool, a, "active").await, seed_skill(&pool, b, "active").await)
        } else {
            ("memory_items", seed_memory(&pool, a).await, seed_memory(&pool, b).await)
        };
        if op == "memory_stale" {
            for id in [a_id, b_id] {
                exec(&pool, "UPDATE memory_items SET last_verified_at = now() WHERE id = $1", &[id]).await;
            }
        }
        let unknown = Uuid::new_v4();
        let before = (row(&pool, table, "id", a_id).await, row(&pool, table, "id", b_id).await);

        let call = |proof: ScopedRead, id: Uuid| {
            let pool = pool.clone();
            async move {
                let mut tx = pool.begin().await.expect("begin");
                let result: AppResult<Option<Uuid>> = match op {
                    "memory_useful" => ContextFeedbackRepository::mark_memory_useful_in_tx(&mut tx, &proof, id)
                        .await
                        .map(|m| Some(m.id.as_uuid())),
                    "memory_stale" => ContextFeedbackRepository::mark_memory_stale_in_tx(&mut tx, &proof, id)
                        .await
                        .map(|m| Some(m.id.as_uuid())),
                    "memory_needs_review" => {
                        ContextFeedbackRepository::mark_memory_needs_review_in_tx(&mut tx, &proof, id)
                            .await
                            .map(|m| Some(m.id.as_uuid()))
                    }
                    "memory_revoke" => ContextFeedbackRepository::revoke_memory_if_active_in_tx(&mut tx, &proof, id)
                        .await
                        .map(|m| m.map(|m| m.id.as_uuid())),
                    _ => ContextFeedbackRepository::revoke_skill_if_active_in_tx(&mut tx, &proof, id)
                        .await
                        .map(|s| s.map(|s| s.id.as_uuid())),
                };
                tx.commit().await.expect("commit");
                result
            }
        };

        let foreign = call(b.proof(), a_id).await;
        let missing = call(b.proof(), unknown).await;
        assert!(!matches!(foreign, Ok(Some(_))), "feedback {op}: foreign write must not report a changed row");
        assert_foreign_is_unknown(
            &pool,
            (table, "id", a_id, b_id),
            shape(&foreign, a_id),
            shape(&missing, unknown),
            before.clone(),
        )
        .await;

        let own = call(a.proof(), a_id).await.unwrap_or_else(|err| panic!("feedback {op}: own write failed: {err}"));
        assert_eq!(own, Some(a_id), "feedback {op}: own write must return the row");
        assert_ne!(row(&pool, table, "id", a_id).await, before.0, "feedback {op}: own write changed nothing");
        assert_eq!(row(&pool, table, "id", b_id).await, before.1, "feedback {op}: B's row changed");
    }
}

/// Agent + task + run for one tenant; returns `(agent_id, task_id, run_id)`.
async fn seed_run(pool: &PgPool, t: Tenant, idempotency_key: &str) -> (Uuid, Uuid, Uuid) {
    let (agent, task, run) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(
        pool,
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'agent', 'idle')",
        &[agent, t.org, t.user],
    )
    .await;
    exec(
        pool,
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by)
         VALUES ($1, $2, 'task', 'queued', $3)",
        &[task, t.org, t.user],
    )
    .await;
    sqlx::query(
        "INSERT INTO task_runs (id, organization_id, workspace_id, orchestration_task_id, agent_id, idempotency_key, status, started_at)
         VALUES ($1, $2, $2, $3, $4, $5, 'working', now())",
    )
    .bind(run)
    .bind(t.org)
    .bind(task)
    .bind(agent)
    .bind(idempotency_key)
    .execute(pool)
    .await
    .expect("seed task run");
    (agent, task, run)
}

async fn seed_feedback(pool: &PgPool, t: Tenant, run: Uuid, item: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO context_feedback (organization_id, workspace_id, run_id, item_id, item_kind, label, user_id)
         VALUES ($1, $1, $2, $3, 'memory', 'useful', $4) RETURNING id",
    )
    .bind(t.org)
    .bind(run)
    .bind(item)
    .bind(t.user)
    .fetch_one(pool)
    .await
    .expect("seed feedback")
}

/// The upsert's unique key `(run_id, item_id, item_kind, user_id)` has no
/// organization, so its conflict update must not touch another organization's row.
#[sqlx::test(migrations = "../db/migrations")]
async fn context_feedback_upsert_conflict_is_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let (_, _, a_run) = seed_run(&pool, a, "feedback").await;
    let (_, _, b_run) = seed_run(&pool, b, "feedback").await;
    let a_item = seed_memory(&pool, a).await;
    let b_item = seed_memory(&pool, b).await;
    let a_fb = seed_feedback(&pool, a, a_run, a_item).await;
    let b_fb = seed_feedback(&pool, b, b_run, b_item).await;
    let before = (row(&pool, "context_feedback", "id", a_fb).await, row(&pool, "context_feedback", "id", b_fb).await);

    let call = |proof: ScopedRead, run: Uuid, item: Uuid| {
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.expect("begin");
            let result = ContextFeedbackRepository::upsert_in_tx(
                &mut tx,
                &proof,
                CreateContextFeedbackRecord {
                    workspace_id: proof.workspace_ids()[0],
                    run_id: run,
                    item_id: item,
                    item_kind: "memory",
                    label: "stale",
                    note: Some("changed"),
                },
            )
            .await;
            tx.commit().await.expect("commit");
            result.map(|feedback| feedback.id)
        }
    };

    // Organization B, as A's user, on A's (run, item): the conflict hits A's row.
    let foreign = call(b.proof_as(a.user), a_run, a_item).await;
    let err = foreign.expect_err("a conflicting row of another organization must not be updated");
    assert!(matches!(err.kind, agentforge_core::ErrorKind::Forbidden(_)), "unexpected error: {err}");
    assert_eq!(row(&pool, "context_feedback", "id", a_fb).await, before.0, "A's feedback row changed");
    assert_eq!(row(&pool, "context_feedback", "id", b_fb).await, before.1, "B's feedback row changed");

    let own = call(a.proof(), a_run, a_item).await.expect("own conflict update");
    assert_eq!(own, a_fb, "own upsert must update the existing row");
    let label: String = sqlx::query_scalar("SELECT label FROM context_feedback WHERE id = $1")
        .bind(a_fb)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(label, "stale");
    assert_eq!(row(&pool, "context_feedback", "id", b_fb).await, before.1, "B's feedback row changed");
}

// ------------------------------------------------------ context candidate ----

#[sqlx::test(migrations = "../db/migrations")]
async fn context_candidate_state_update_is_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let seed = |t: Tenant| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO context_candidates (organization_id, workspace_id, item_kind, proposed_content, owner_user_id)
                 VALUES ($1, $1, 'memory', '{}'::jsonb, $2) RETURNING id",
            )
            .bind(t.org)
            .bind(t.user)
            .fetch_one(&pool)
            .await
            .expect("seed candidate")
        }
    };
    let (a_id, b_id, unknown) = (seed(a).await, seed(b).await, Uuid::new_v4());
    let before =
        (row(&pool, "context_candidates", "id", a_id).await, row(&pool, "context_candidates", "id", b_id).await);
    let call = |proof: ScopedRead, id: Uuid| {
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.expect("begin");
            let result = ContextCandidateRepository::update_state_in_tx(&mut tx, &proof, id, "rejected").await;
            tx.commit().await.expect("commit");
            result.map(|candidate| candidate.id)
        }
    };

    let foreign = call(b.proof(), a_id).await;
    let missing = call(b.proof(), unknown).await;
    assert!(foreign.is_err());
    assert_foreign_is_unknown(
        &pool,
        ("context_candidates", "id", a_id, b_id),
        shape(&foreign, a_id),
        shape(&missing, unknown),
        before.clone(),
    )
    .await;

    call(a.proof(), a_id).await.expect("own state update");
    assert_ne!(row(&pool, "context_candidates", "id", a_id).await, before.0);
    assert_eq!(row(&pool, "context_candidates", "id", b_id).await, before.1);
}

// ---------------------------------------------- collaborators and groups ----

#[sqlx::test(migrations = "../db/migrations")]
async fn agent_collaborator_writes_are_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let repo = AgentRepository::new(pool.clone());

    for op in ["update", "remove"] {
        let (a_agent, _, _) = seed_run(&pool, a, &format!("collab-{op}")).await;
        let (b_agent, _, _) = seed_run(&pool, b, &format!("collab-{op}")).await;
        for (agent, t) in [(a_agent, a), (b_agent, b)] {
            exec(
                &pool,
                "INSERT INTO agent_collaborators (agent_id, user_id, permission) VALUES ($1, $2, 'view')",
                &[agent, t.user],
            )
            .await;
        }
        let unknown = Uuid::new_v4();
        let before = (
            row(&pool, "agent_collaborators", "agent_id", a_agent).await,
            row(&pool, "agent_collaborators", "agent_id", b_agent).await,
        );
        let call = |scope: TenantScope, agent: Uuid| {
            let repo = &repo;
            async move {
                match op {
                    "update" => {
                        repo.update_collaborator(&scope, AgentId::from(agent), a.user, "edit").await.map(|_| ())
                    }
                    _ => repo.remove_collaborator(&scope, AgentId::from(agent), a.user).await,
                }
            }
        };

        let foreign = call(b.scope(), a_agent).await;
        let missing = call(b.scope(), unknown).await;
        assert!(foreign.is_err(), "collaborator {op}: foreign write must fail");
        assert_foreign_is_unknown(
            &pool,
            ("agent_collaborators", "agent_id", a_agent, b_agent),
            shape(&foreign, a_agent),
            shape(&missing, unknown),
            before.clone(),
        )
        .await;

        call(a.scope(), a_agent).await.unwrap_or_else(|err| panic!("collaborator {op}: own write failed: {err}"));
        assert_ne!(row(&pool, "agent_collaborators", "agent_id", a_agent).await, before.0);
        assert_eq!(row(&pool, "agent_collaborators", "agent_id", b_agent).await, before.1);
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn group_member_removal_is_constrained_by_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let repo = GroupRepository::new(pool.clone());
    let seed = |t: Tenant| {
        let pool = pool.clone();
        async move {
            let group = Uuid::new_v4();
            exec(
                &pool,
                "INSERT INTO groups (id, organization_id, name, created_by) VALUES ($1, $2, 'group', $3)",
                &[group, t.org, t.user],
            )
            .await;
            exec(&pool, "INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)", &[group, t.user]).await;
            group
        }
    };
    let (a_group, b_group, unknown) = (seed(a).await, seed(b).await, Uuid::new_v4());
    let before = (
        row(&pool, "group_members", "group_id", a_group).await,
        row(&pool, "group_members", "group_id", b_group).await,
    );

    let foreign = repo.remove_member(&b.scope(), GroupId::from(a_group), a.user).await;
    let missing = repo.remove_member(&b.scope(), GroupId::from(unknown), a.user).await;
    assert!(foreign.is_err());
    assert_foreign_is_unknown(
        &pool,
        ("group_members", "group_id", a_group, b_group),
        shape(&foreign, a_group),
        shape(&missing, unknown),
        before.clone(),
    )
    .await;

    repo.remove_member(&a.scope(), GroupId::from(a_group), a.user).await.expect("own member removal");
    assert_eq!(row(&pool, "group_members", "group_id", a_group).await, None);
    assert_eq!(row(&pool, "group_members", "group_id", b_group).await, before.1);
}

// ------------------------------------------------------------- task runs ----

async fn load_task(pool: &PgPool, id: Uuid) -> OrchestrationTask {
    sqlx::query_as::<_, OrchestrationTask>("SELECT * FROM orchestration_tasks WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("load task")
}

/// The upsert's conflict key `(orchestration_task_id, idempotency_key)` has no
/// organization; the source must prove the task belongs to the scope.
#[sqlx::test(migrations = "../db/migrations")]
async fn task_run_upsert_is_constrained_by_task_organization(pool: PgPool) {
    let (a, b) = tenants(&pool).await;
    let (a_agent, a_task, a_run) = seed_run(&pool, a, "assignment").await;
    let (b_agent, _, b_run) = seed_run(&pool, b, "assignment").await;
    let repo = TaskRunRepository::new(pool.clone());
    let before = (row(&pool, "task_runs", "id", a_run).await, row(&pool, "task_runs", "id", b_run).await);

    let call = |scope: TenantScope, mut task: OrchestrationTask, agent: Uuid| {
        let pool = pool.clone();
        let repo = &repo;
        async move {
            task.assigned_agent_id = Some(AgentId::from(agent));
            let mut tx = pool.begin().await.expect("begin");
            let result =
                repo.create_for_assignment_in_tx(&mut tx, &scope, &task, "assignment", serde_json::json!({})).await;
            tx.commit().await.expect("commit");
            result.map(|run| run.id)
        }
    };

    // Organization B with its own agent, pointed at A's task.
    let foreign = call(b.scope(), load_task(&pool, a_task).await, b_agent).await;
    let mut unknown_task = load_task(&pool, a_task).await;
    let unknown = Uuid::new_v4();
    unknown_task.id = unknown;
    let missing = call(b.scope(), unknown_task, b_agent).await;
    assert!(foreign.is_err(), "a run for another organization's task must not be written");
    assert_foreign_is_unknown(
        &pool,
        ("task_runs", "id", a_run, b_run),
        shape(&foreign, a_task),
        shape(&missing, unknown),
        before.clone(),
    )
    .await;
    let foreign_runs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_runs WHERE orchestration_task_id = $1 AND organization_id = $2")
            .bind(a_task)
            .bind(b.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(foreign_runs, 0, "no run may be inserted for another organization's task");

    let own = call(a.scope(), load_task(&pool, a_task).await, a_agent).await.expect("own conflict update");
    assert_eq!(own, a_run, "own upsert must return the existing run");
    assert_eq!(row(&pool, "task_runs", "id", b_run).await, before.1);
}
