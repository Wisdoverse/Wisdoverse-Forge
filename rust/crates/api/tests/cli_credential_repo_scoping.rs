//! Repository-level integration tests. Everything that keys off `user_id`
//! (find_encrypted, upsert, delete, list_for_user, find_all_active_by_cli_tool)
//! gets a cross-user seeding + assertion so a regression that drops the
//! `WHERE user_id = $N` clause would show up as a cross-tenant leak.

mod common;

use agentforge_api::repositories::cli_credential::CliCredentialRepository;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../db/migrations")]
async fn find_encrypted_is_per_user_scoped(pool: PgPool) {
    let alice = common::seed_user(&pool).await;
    let bob = common::seed_user(&pool).await;
    let repo = CliCredentialRepository::new(pool.clone());

    repo.upsert_encrypted(&common::scope_for(alice), "codex", "alice-blob").await.unwrap();
    repo.upsert_encrypted(&common::scope_for(bob), "codex", "bob-blob").await.unwrap();

    assert_eq!(repo.find_encrypted(&common::scope_for(alice), "codex").await.unwrap().as_deref(), Some("alice-blob"));
    assert_eq!(repo.find_encrypted(&common::scope_for(bob), "codex").await.unwrap().as_deref(), Some("bob-blob"));
    // Cross-user lookup must return None, not the other user's blob.
    assert_eq!(repo.find_encrypted(&common::scope_for(alice), "claude").await.unwrap(), None);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upsert_conflict_preserves_created_at_bumps_updated_at(pool: PgPool) {
    let user = common::seed_user(&pool).await;
    let repo = CliCredentialRepository::new(pool.clone());

    repo.upsert_encrypted(&common::scope_for(user), "codex", "v1").await.unwrap();
    let (created_v1, updated_v1): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, updated_at FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2")
            .bind(user)
            .bind("codex")
            .fetch_one(&pool)
            .await
            .unwrap();

    // Force a measurable clock gap. NOW() resolution is microsecond but the
    // test can race — sleep 5ms so updated_at is strictly greater without
    // flaking on fast machines.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    repo.upsert_encrypted(&common::scope_for(user), "codex", "v2").await.unwrap();

    let (created_v2, updated_v2): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, updated_at FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2")
            .bind(user)
            .bind("codex")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(created_v1, created_v2, "created_at must not change on conflict");
    assert!(updated_v2 > updated_v1, "updated_at must advance on conflict (got v1={updated_v1} v2={updated_v2})");
    assert_eq!(
        repo.find_encrypted(&common::scope_for(user), "codex").await.unwrap().as_deref(),
        Some("v2"),
        "ciphertext replaced"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn list_for_user_returns_only_own_rows(pool: PgPool) {
    let alice = common::seed_user(&pool).await;
    let bob = common::seed_user(&pool).await;
    let repo = CliCredentialRepository::new(pool.clone());

    repo.upsert_encrypted(&common::scope_for(alice), "codex", "a").await.unwrap();
    repo.upsert_encrypted(&common::scope_for(alice), "claude", "b").await.unwrap();
    repo.upsert_encrypted(&common::scope_for(bob), "codex", "c").await.unwrap();

    let alice_rows = repo.list_for_user(&common::scope_for(alice)).await.unwrap();
    let tools: Vec<&str> = alice_rows.iter().map(|r| r.cli_tool.as_str()).collect();
    assert_eq!(tools, vec!["claude", "codex"], "sorted alphabetically, alice-only");

    let bob_rows = repo.list_for_user(&common::scope_for(bob)).await.unwrap();
    assert_eq!(bob_rows.len(), 1);
    assert_eq!(bob_rows[0].cli_tool, "codex");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn find_all_active_by_cli_tool_excludes_revoked(pool: PgPool) {
    let alice = common::seed_user(&pool).await;
    let bob = common::seed_user(&pool).await;
    let repo = CliCredentialRepository::new(pool.clone());

    repo.upsert_encrypted(&common::scope_for(alice), "codex", "a").await.unwrap();
    repo.upsert_encrypted(&common::scope_for(bob), "codex", "b").await.unwrap();

    // Revoke bob's row directly via SQL — production code path is
    // `bump_fail_count_or_revoke` but this test only cares about the filter.
    sqlx::query("UPDATE user_cli_credentials SET revoked_at = NOW(), revoke_reason = 'test' WHERE user_id = $1")
        .bind(bob)
        .execute(&pool)
        .await
        .unwrap();

    let active = repo.find_all_active_by_cli_tool("codex").await.unwrap();
    let user_ids: Vec<Uuid> = active.iter().map(|(u, _)| *u).collect();
    assert_eq!(user_ids, vec![alice], "bob's revoked row filtered out");

    // Sanity: find_all_by_cli_tool (no filter) sees both.
    let all = repo.find_all_by_cli_tool("codex").await.unwrap();
    assert_eq!(all.len(), 2);
}
