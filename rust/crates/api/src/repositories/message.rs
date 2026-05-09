//! Message repository — tenant-scoped queries for `agent_messages`.
//!
//! Provider+prompt agents (issue #21) persist one pair of rows per turn:
//! a `role="user"` row on prompt submission, then a `role="assistant"` row
//! once the LLM stream drains. `PromptService` uses [`insert_with_id`] so
//! the `message_start` SSE frame's UUID matches the eventual DB row.
//!
//! The [`list`] helper reads newest-first (DESC + `LIMIT`) and reverses
//! in memory so callers get chronological ASC — this keeps the hot-path
//! history fetch cheap while preserving the UX-friendly iteration order.

use chrono::{DateTime, Utc};

use agentforge_core::{AgentId, AppResult, MessageId, TenantScope};
use agentforge_db::entities::AgentMessage;
use sqlx::PgPool;

/// Database access layer for agent chat messages. All queries enforce
/// tenant isolation via `WHERE organization_id = $N`.
pub struct MessageRepository {
    pool: PgPool,
}

impl MessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new message, auto-generating the `MessageId`.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        role: &str,
        content: &str,
        tokens_in: Option<i32>,
        tokens_out: Option<i32>,
        model: Option<&str>,
        finish_reason: Option<&str>,
    ) -> AppResult<AgentMessage> {
        self.insert_with_id(
            MessageId::new(),
            scope,
            agent_id,
            role,
            content,
            tokens_in,
            tokens_out,
            model,
            finish_reason,
        )
        .await
    }

    /// Insert with a caller-provided id. `PromptService` uses this so the
    /// `message_start` SSE frame's UUID matches the eventual DB row.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_with_id(
        &self,
        id: MessageId,
        scope: &TenantScope,
        agent_id: AgentId,
        role: &str,
        content: &str,
        tokens_in: Option<i32>,
        tokens_out: Option<i32>,
        model: Option<&str>,
        finish_reason: Option<&str>,
    ) -> AppResult<AgentMessage> {
        let row = sqlx::query_as::<_, AgentMessage>(
            r#"INSERT INTO agent_messages
                   (id, organization_id, agent_id, role, content,
                    tokens_in, tokens_out, model, finish_reason)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(role)
        .bind(content)
        .bind(tokens_in)
        .bind(tokens_out)
        .bind(model)
        .bind(finish_reason)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Fetch the newest page (DESC in SQL, reversed in-memory so callers get
    /// chronological ASC). `before` is an exclusive upper bound on
    /// `created_at` for paging further back in time.
    pub async fn list(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> AppResult<Vec<AgentMessage>> {
        let mut rows = if let Some(ts) = before {
            sqlx::query_as::<_, AgentMessage>(
                r#"SELECT * FROM agent_messages
                   WHERE organization_id = $1 AND agent_id = $2 AND created_at < $3
                   ORDER BY created_at DESC LIMIT $4"#,
            )
            .bind(scope.org_id().as_uuid())
            .bind(agent_id.as_uuid())
            .bind(ts)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, AgentMessage>(
                r#"SELECT * FROM agent_messages
                   WHERE organization_id = $1 AND agent_id = $2
                   ORDER BY created_at DESC LIMIT $3"#,
            )
            .bind(scope.org_id().as_uuid())
            .bind(agent_id.as_uuid())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        rows.reverse();
        Ok(rows)
    }

    /// Bulk-delete every message attached to an agent within the caller's
    /// org. Returns the number of rows removed.
    pub async fn delete_all_by_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<u64> {
        let r = sqlx::query(r#"DELETE FROM agent_messages WHERE organization_id = $1 AND agent_id = $2"#)
            .bind(scope.org_id().as_uuid())
            .bind(agent_id.as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    //! TDD suite for `MessageRepository`.
    //!
    //! Each case gets a fresh DB via `#[sqlx::test(migrations = ...)]`. The
    //! migrations path is relative to `CARGO_MANIFEST_DIR` (`rust/crates/api`),
    //! mirroring the tests in `rust/crates/api/tests/`.
    //!
    //! Seeding copies the shape used by `nav_regression_e2e_test.rs`'s
    //! `seed_user_with_org` — organizations + workspaces + users +
    //! organization_members — and extends it with an `agents` row so the
    //! `agent_messages.agent_id` FK is satisfiable. Two separate orgs are
    //! used for the tenant-isolation case.
    //!
    //! Tests deliberately stagger `pg_sleep(...)` between inserts so
    //! `created_at` is monotonically distinct and the ASC/DESC ordering
    //! assertions are unambiguous.
    use super::*;
    use sqlx::PgPool;
    use uuid::Uuid;

    /// Seed one org + workspace + user + membership, then an agent with the
    /// supplied `provider`. Returns the tenant scope and the agent id.
    async fn seed_agent_with_provider(pool: &PgPool, provider: &str) -> (TenantScope, AgentId) {
        let org_uuid = Uuid::new_v4();
        let user_uuid = Uuid::new_v4();
        let agent_uuid = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_uuid)
            .bind(format!("Org {org_uuid}"))
            .bind(format!("org-{org_uuid}"))
            .execute(pool)
            .await
            .expect("seed org");
        // Matches the convention in `nav_regression_e2e_test.rs`: reuse
        // `org_id` as `workspace_id` so FKs from `projects`/`agents` that
        // reference a workspace resolve without a separate lookup.
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_uuid)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_uuid)
            .bind(format!("u-{user_uuid}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org_uuid)
            .bind(user_uuid)
            .execute(pool)
            .await
            .expect("seed membership");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, provider, status)
             VALUES ($1, $2, $2, $3, $4, 'idle')",
        )
        .bind(agent_uuid)
        .bind(org_uuid)
        .bind(user_uuid)
        .bind(provider)
        .execute(pool)
        .await
        .expect("seed agent");

        let scope = crate::test_support::tenant_scope_for_ids(org_uuid, user_uuid);
        (scope, AgentId::from(agent_uuid))
    }

    /// Insert a user message followed by an assistant message; `list` returns
    /// both in chronological ASC order (which is DESC+reverse internally).
    #[sqlx::test(migrations = "../db/migrations")]
    async fn insert_and_list_ordered_asc(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "anthropic").await;
        let repo = MessageRepository::new(pool.clone());

        let user_row =
            repo.insert(&scope, agent_id, "user", "hello", None, None, None, None).await.expect("insert user message");
        // Ensure strictly later `created_at` on the assistant row so the ASC
        // assertion is deterministic regardless of clock resolution.
        sqlx::query("SELECT pg_sleep(0.01)").execute(&pool).await.expect("sleep");
        let asst_row = repo
            .insert(&scope, agent_id, "assistant", "hi back", Some(5), Some(3), Some("claude-4"), Some("stop"))
            .await
            .expect("insert assistant message");

        let listed = repo.list(&scope, agent_id, 50, None).await.expect("list messages");
        assert_eq!(listed.len(), 2, "expected both messages listed");
        assert_eq!(listed[0].id, user_row.id, "user message first (ASC)");
        assert_eq!(listed[1].id, asst_row.id, "assistant message second (ASC)");
        assert_eq!(listed[0].role, "user");
        assert_eq!(listed[1].role, "assistant");
        assert_eq!(listed[1].tokens_in, Some(5));
        assert_eq!(listed[1].model.as_deref(), Some("claude-4"));
    }

    /// With 5 messages `[m1..m5]` inserted in order and `limit=3`, `list`
    /// returns `[m3, m4, m5]` (newest 3, reversed to ASC). Paging with
    /// `before = m3.created_at` then returns `[m1, m2]`.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn list_pagination_before_cursor(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "openai").await;
        let repo = MessageRepository::new(pool.clone());

        let mut inserted = Vec::new();
        for i in 0..5 {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            let row = repo
                .insert(&scope, agent_id, role, &format!("msg-{i}"), None, None, None, None)
                .await
                .expect("insert message");
            inserted.push(row);
            sqlx::query("SELECT pg_sleep(0.01)").execute(&pool).await.expect("sleep");
        }

        // Newest 3 page, reversed to ASC → m3, m4, m5.
        let page1 = repo.list(&scope, agent_id, 3, None).await.expect("list newest 3");
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].id, inserted[2].id, "page1[0] == m3");
        assert_eq!(page1[1].id, inserted[3].id, "page1[1] == m4");
        assert_eq!(page1[2].id, inserted[4].id, "page1[2] == m5");

        // Page back past m3 → m1, m2 (in ASC, after DESC+reverse).
        let cursor = page1[0].created_at;
        let page2 = repo.list(&scope, agent_id, 50, Some(cursor)).await.expect("list before cursor");
        assert_eq!(page2.len(), 2, "only m1, m2 remain before m3");
        assert_eq!(page2[0].id, inserted[0].id, "page2[0] == m1");
        assert_eq!(page2[1].id, inserted[1].id, "page2[1] == m2");
    }

    /// Two distinct orgs — org A writes, org B reads empty. Tenant guard
    /// lives in the `WHERE organization_id = $N` clause, not in app logic.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tenant_isolation(pool: PgPool) {
        let (scope_a, agent_a) = seed_agent_with_provider(&pool, "anthropic").await;
        let (scope_b, agent_b) = seed_agent_with_provider(&pool, "anthropic").await;
        assert_ne!(scope_a.org_id(), scope_b.org_id(), "distinct orgs");

        let repo = MessageRepository::new(pool);
        repo.insert(&scope_a, agent_a, "user", "secret-a", None, None, None, None).await.expect("insert into org A");

        // Org B asking about its OWN agent → empty (nothing inserted there).
        let own_listing = repo.list(&scope_b, agent_b, 50, None).await.expect("list org B own");
        assert!(own_listing.is_empty(), "org B has no messages of its own");

        // Org B asking about org A's agent_id → still empty because the
        // `organization_id` predicate excludes A's rows. This is the
        // invariant that prevents cross-tenant leakage if a caller ever
        // passes a foreign agent id.
        let cross_listing = repo.list(&scope_b, agent_a, 50, None).await.expect("list cross-tenant");
        assert!(cross_listing.is_empty(), "org B cannot see org A's messages");
    }

    /// `ON DELETE CASCADE` from `agents(id)` — deleting the agent row wipes
    /// every attached message.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn cascade_on_agent_delete(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "anthropic").await;
        let repo = MessageRepository::new(pool.clone());

        repo.insert(&scope, agent_id, "user", "hello", None, None, None, None).await.expect("insert message");
        let before = repo.list(&scope, agent_id, 50, None).await.expect("list pre-delete");
        assert_eq!(before.len(), 1, "sanity: one message before agent delete");

        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(agent_id.as_uuid())
            .execute(&pool)
            .await
            .expect("delete agent");

        let after = repo.list(&scope, agent_id, 50, None).await.expect("list post-delete");
        assert!(after.is_empty(), "CASCADE wiped agent's messages");
    }

    /// `delete_all_by_agent` removes every row for an agent and returns the
    /// affected-row count; subsequent `list` is empty.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn delete_all_by_agent(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "anthropic").await;
        let repo = MessageRepository::new(pool);

        for i in 0..3 {
            repo.insert(&scope, agent_id, "user", &format!("m-{i}"), None, None, None, None)
                .await
                .expect("insert message");
        }

        let affected = repo.delete_all_by_agent(&scope, agent_id).await.expect("delete_all_by_agent");
        assert_eq!(affected, 3, "all three rows deleted");

        let remaining = repo.list(&scope, agent_id, 50, None).await.expect("list after delete_all");
        assert!(remaining.is_empty(), "no messages remain");
    }
}
