//! #1315: an administrator adopts a changed pull-request head through an
//! explicit, audited action. Real HTTP/auth/SQL against the in-process fake
//! GitHub, whose calls can be paused so every interleaving is forced
//! explicitly. Its own test binary because `GITHUB_API_BASE` is process-global.

mod self_fix_harness;

use agentforge_api::test_support::mint_test_jwt;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use self_fix_harness::*;

const RUNNING: &str = "errors.self_fix.merge_running";
const MERGE_UNCONFIRMED: &str = "errors.self_fix.reconcile_merge_unconfirmed";

async fn reconcile(app: Router, jwt: String, task: Uuid, stored: char, new: char) -> (StatusCode, Value) {
    let body = json!({"expectedStoredHead": sha(stored), "expectedNewHead": sha(new)});
    call(&app, &jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/reconcile-head"), body).await
}

/// (stored head, review status, merge attempts, review window start)
async fn row(pool: &PgPool, task: Uuid) -> (String, String, i32, Option<chrono::DateTime<chrono::Utc>>) {
    sqlx::query_as(
        "SELECT pr_head_sha, review_status, merge_attempts, review_opened_at FROM orchestration_tasks WHERE id=$1",
    )
    .bind(task)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Audit rows for this task: (actor, details).
async fn audits(pool: &PgPool, task: Uuid) -> Vec<(Option<Uuid>, Value)> {
    sqlx::query_as("SELECT user_id, details FROM audit_log WHERE action='self_fix.head_reconciled' AND resource_id=$1")
        .bind(task)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// The comparison path segment `base...head` for two fake SHAs.
fn bh(base: char, head: char) -> String {
    format!("{}...{}", sha(base), sha(head))
}

/// GitHub comparison file objects for plain paths.
fn files(paths: &[&str]) -> Vec<Value> {
    paths.iter().map(|path| json!({"filename": path})).collect()
}

fn blocked_by(pid: i32) -> String {
    format!("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE {pid} = ANY(pg_blocking_pids(pid)))")
}
/// At least `n` sessions of THIS test database are waiting on a lock. (A
/// second waiter on the same row may be blocked by the first waiter, not by
/// the holder, so this counts blocked sessions rather than one blocker.)
fn blocked_count(n: usize) -> String {
    format!(
        "SELECT count(*) >= {n} FROM pg_stat_activity WHERE datname = current_database() AND cardinality(pg_blocking_pids(pid)) > 0"
    )
}

#[sqlx::test(migrations = "../db/migrations")]
async fn administrator_adopts_a_changed_head_with_live_checks_and_an_audit_record(pool: PgPool) {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email,is_admin) VALUES ($1,'dev@example.com',true)")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let (org, group) = destination(&pool, user).await;
    let (other_org, _) = destination(&pool, user).await;
    let member = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email,is_admin) VALUES ($1,$2,false)")
        .bind(member)
        .bind(format!("member-{member}@example.com"))
        .execute(&pool)
        .await
        .unwrap();
    let jwt = mint_test_jwt(org, user, "owner");
    let other_jwt = mint_test_jwt(other_org, user, "owner");
    let member_jwt = mint_test_jwt(org, member, "member");

    let (fake, base) = start_fake().await;
    unsafe { std::env::set_var("GITHUB_API_BASE", &base) };
    let f =
        Fixture { app: forge_app(pool.clone()).await, pool: pool.clone(), fake: fake.clone(), org, user, group, jwt };
    let app = f.app.clone();
    let jwt = f.jwt.clone();

    // Happy path: the outside push moved the head from c… to d…. The rejected
    // verdict on the old revision blocks the merge until the new head is adopted.
    let task = f.task("reconcile-happy", 91, 'c').await;
    let old_report = f.report(task).await;
    assert_eq!(f.verdict(task, old_report, "rejected").await.0, StatusCode::OK);
    f.fake.set_head(91, 'd');
    assert_eq!(f.rejected_flag(task).await, true);
    let before = row(&pool, task).await;
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["changed"], true);
    assert_eq!(body["data"]["sensitive"], false);
    assert_eq!(body["data"]["reviewStatus"], "in_review");
    let after = row(&pool, task).await;
    assert_eq!(after.0, sha('d'));
    assert_eq!(
        (after.1.as_str(), after.2, after.3),
        ("in_review", before.2, before.3),
        "status, attempts and review window unchanged"
    );
    let audit = audits(&pool, task).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].0, Some(user));
    assert_eq!(
        audit[0].1,
        json!({"previousHead": sha('c'), "newHead": sha('d'), "pullRequest": 91, "sensitive": false})
    );
    // The old report and its rejection are no longer current.
    assert_eq!(f.rejected_flag(task).await, false);
    // Acceptance needs a new report on the new revision.
    let (status, body) = f.verdict(task, old_report, "accepted").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let new_report = f.report(task).await;
    let (status, body) = f.verdict(task, new_report, "accepted").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The merge runs at the adopted head.
    let (status, body) = f.approve(task).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["mergedHeadSha"], sha('d'));
    // A repeat of the same request after success is an idempotent no-op.
    let task_noop = f.task("reconcile-repeat", 92, 'c').await;
    f.fake.set_head(92, 'd');
    assert_eq!(reconcile(app.clone(), jwt.clone(), task_noop, 'c', 'd').await.0, StatusCode::OK);
    let requests = fake.requests();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task_noop, 'c', 'd').await;
    assert_eq!((status, body["data"]["changed"].clone()), (StatusCode::OK, json!(false)), "{body}");
    assert_eq!(fake.requests(), requests, "a no-op makes no provider call");
    assert_eq!(audits(&pool, task_noop).await.len(), 1, "a no-op writes no audit record");

    // Same-head request: nothing to adopt.
    let task = f.task("reconcile-same", 93, 'c').await;
    let requests = fake.requests();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'c').await;
    assert_eq!((status, body["data"]["changed"].clone()), (StatusCode::OK, json!(false)), "{body}");
    assert_eq!((fake.requests(), audits(&pool, task).await.len()), (requests, 0));

    // The stored head is not the one the caller saw: refused before any provider call.
    let task = f.task("reconcile-stale", 94, 'c').await;
    f.fake.set_head(94, 'd');
    let requests = fake.requests();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'e', 'd').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "errors.self_fix.reconcile_stale"), "{body}");
    assert_eq!((row(&pool, task).await.0, fake.requests(), audits(&pool, task).await.len()), (sha('c'), requests, 0));

    // The live head is not the expected new head.
    let task = f.task("reconcile-moved", 95, 'c').await;
    f.fake.set_head(95, 'd');
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'e').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "errors.self_fix.reconcile_head_mismatch"), "{body}");
    assert_eq!((row(&pool, task).await.0, audits(&pool, task).await.len()), (sha('c'), 0));

    // The pull request is closed.
    let task = f.task("reconcile-closed", 96, 'c').await;
    f.fake.set_head(96, 'd');
    f.fake.closed.lock().unwrap().insert(96);
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "errors.self_fix.reconcile_pull_request_not_open"),
        "{body}"
    );
    assert_eq!(row(&pool, task).await.0, sha('c'));

    // The pull request was retargeted to another base branch.
    let task = f.task("reconcile-base", 97, 'c').await;
    f.fake.set_head(97, 'd');
    f.fake.bases.lock().unwrap().insert(97, "release".into());
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "errors.self_fix.reconcile_base_changed"), "{body}");
    assert_eq!(row(&pool, task).await.0, sha('c'));

    // Another tenant gets 404, and a non-administrator 403, without any provider request.
    let task = f.task("reconcile-tenant", 98, 'c').await;
    f.fake.set_head(98, 'd');
    let requests = fake.requests();
    assert_eq!(reconcile(app.clone(), other_jwt.clone(), task, 'c', 'd').await.0, StatusCode::NOT_FOUND);
    assert_eq!(reconcile(app.clone(), member_jwt.clone(), task, 'c', 'd').await.0, StatusCode::FORBIDDEN);
    assert_eq!(fake.requests(), requests, "no provider request for another tenant or a non-administrator");
    assert_eq!(row(&pool, task).await.0, sha('c'));

    // A sensitive new head sets `sensitive_blocked`; the merge is then refused.
    // The comparison is requested with exactly the validated base commit and
    // the confirmed new head.
    let task = f.task("reconcile-sensitive", 99, 'c').await;
    f.fake.set_head(99, 'd');
    f.fake.base_shas.lock().unwrap().insert(99, sha('9'));
    f.fake.comparisons.lock().unwrap().insert(bh('9', 'd'), files(&["rust/crates/db/migrations/999_backdoor.sql"]));
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        (body["data"]["sensitive"].clone(), body["data"]["reviewStatus"].clone()),
        (json!(true), json!("sensitive_blocked"))
    );
    assert_eq!(fake.comparisons_of(&bh('9', 'd')), 1, "compared exactly base commit ... expected new head");
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::FORBIDDEN, "errors.self_fix.sensitive_path_blocked"), "{body}");
    assert_eq!(audits(&pool, task).await[0].1["sensitive"], true);

    // GitHub documents at most 300 files per comparison: 300 counts as
    // possibly incomplete (sensitive); 299 clean files adopt normally.
    for (pr, reference, base, count, expected) in
        [(100, "reconcile-cap", '1', 300, "sensitive_blocked"), (111, "reconcile-under-cap", '0', 299, "in_review")]
    {
        let task = f.task(reference, pr, 'c').await;
        f.fake.set_head(pr, 'd');
        f.fake.base_shas.lock().unwrap().insert(pr, sha(base));
        let many: Vec<String> = (0..count).map(|n| format!("src/generated/file_{n}.rs")).collect();
        f.fake
            .comparisons
            .lock()
            .unwrap()
            .insert(bh(base, 'd'), files(&many.iter().map(String::as_str).collect::<Vec<_>>()));
        let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
        assert_eq!(status, StatusCode::OK, "{reference}: {body}");
        assert_eq!(row(&pool, task).await.1, expected, "{reference}");
    }

    // A sensitive file late in a long (under-cap) list, and renames where only
    // the old or only the new path is sensitive, are all detected.
    let mut late: Vec<Value> = (0..250).map(|n| json!({"filename": format!("src/generated/late_{n}.rs")})).collect();
    late.push(json!({"filename": "rust/crates/auth/src/jwt.rs"}));
    for (pr, reference, base, entries) in [
        (112, "reconcile-late-sensitive", '2', late),
        (
            113,
            "reconcile-rename-from-sensitive",
            '3',
            vec![
                json!({"filename": "src/renamed.rs", "previous_filename": "rust/crates/auth/src/jwt.rs", "status": "renamed"}),
            ],
        ),
        (
            114,
            "reconcile-rename-to-sensitive",
            '4',
            vec![
                json!({"filename": "rust/crates/auth/src/moved.rs", "previous_filename": "src/old.rs", "status": "renamed"}),
            ],
        ),
    ] {
        let task = f.task(reference, pr, 'c').await;
        f.fake.set_head(pr, 'd');
        f.fake.base_shas.lock().unwrap().insert(pr, sha(base));
        f.fake.comparisons.lock().unwrap().insert(bh(base, 'd'), entries);
        let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
        assert_eq!((status, body["data"]["sensitive"].clone()), (StatusCode::OK, json!(true)), "{reference}: {body}");
        assert_eq!(row(&pool, task).await.1, "sensitive_blocked", "{reference}");
    }

    // A FAILED comparison is a refusal like any other failed provider read:
    // 503, head and status unchanged, no audit record. A retry with a healthy
    // provider then adopts normally.
    let task = f.task("reconcile-compare-failed", 101, 'c').await;
    f.fake.set_head(101, 'd');
    f.fake.base_shas.lock().unwrap().insert(101, sha('5'));
    f.fake.failing_comparisons.lock().unwrap().insert(bh('5', 'd'));
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let unchanged = row(&pool, task).await;
    assert_eq!((unchanged.0, unchanged.1, audits(&pool, task).await.len()), (sha('c'), "in_review".into(), 0));
    f.fake.failing_comparisons.lock().unwrap().clear();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, body["data"]["sensitive"].clone()), (StatusCode::OK, json!(false)), "{body}");
    let adopted = row(&pool, task).await;
    assert_eq!((adopted.0, adopted.1, audits(&pool, task).await.len()), (sha('d'), "in_review".into(), 1));

    // The pull request was retargeted before the first read: refused before
    // any comparison is requested.
    let task = f.task("reconcile-retargeted-first", 115, 'c').await;
    f.fake.set_head(115, 'd');
    f.fake.bases.lock().unwrap().insert(115, "release".into());
    let compared = fake.compared.lock().unwrap().len();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "errors.self_fix.reconcile_base_changed"), "{body}");
    assert_eq!(fake.compared.lock().unwrap().len(), compared, "no comparison for an unexpected base");

    // The base or the head changes between the first read and the final
    // re-read (while the comparison runs): refused, nothing written.
    for (pr, reference, base, change, expected) in [
        (116, "reconcile-base-moved-during", '6', "base", "errors.self_fix.reconcile_base_changed"),
        (117, "reconcile-head-moved-during", '7', "head", "errors.self_fix.reconcile_head_mismatch"),
    ] {
        let task = f.task(reference, pr, 'c').await;
        f.fake.set_head(pr, 'd');
        f.fake.base_shas.lock().unwrap().insert(pr, sha(base));
        let (entered, release) = f.fake.hold_comparison(&bh(base, 'd'));
        let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
        entered.await.unwrap();
        if change == "base" {
            f.fake.bases.lock().unwrap().insert(pr, "release".into());
        } else {
            f.fake.set_head(pr, 'e');
        }
        release.send(()).unwrap();
        let (status, body) = reconciling.await.unwrap();
        assert_eq!((status, code(&body)), (StatusCode::CONFLICT, expected), "{reference}: {body}");
        assert_eq!((row(&pool, task).await.0, audits(&pool, task).await.len()), (sha('c'), 0), "{reference}");
    }

    // A sensitive file present only relative to the REAL base is detected even
    // though the pull request was pointed at another base in between: the
    // comparison uses the immutable base commit from the first read.
    let task = f.task("reconcile-retarget-round-trip", 118, 'c').await;
    f.fake.set_head(118, 'd');
    f.fake.base_shas.lock().unwrap().insert(118, sha('8'));
    f.fake.comparisons.lock().unwrap().insert(bh('8', 'd'), files(&["rust/crates/auth/src/jwt.rs"]));
    f.fake.comparisons.lock().unwrap().insert(bh('a', 'd'), files(&["src/lib.rs"]));
    let (entered, release) = f.fake.hold_comparison(&bh('8', 'd'));
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    entered.await.unwrap();
    f.fake.bases.lock().unwrap().insert(118, "release".into());
    f.fake.base_shas.lock().unwrap().insert(118, sha('a'));
    f.fake.bases.lock().unwrap().insert(118, "develop".into());
    release.send(()).unwrap();
    let (status, body) = reconciling.await.unwrap();
    assert_eq!((status, body["data"]["sensitive"].clone()), (StatusCode::OK, json!(true)), "{body}");
    assert_eq!(row(&pool, task).await.1, "sensitive_blocked");
    assert_eq!(fake.comparisons_of(&bh('a', 'd')), 0, "never compared against the other base");

    // Administrator authority lost while the provider is read: the decision
    // transaction re-checks it and refuses. Fails if the reconcile takes a
    // plain row lock instead of `lock_task`.
    let task = f.task("reconcile-authority-lost", 119, 'c').await;
    f.fake.set_head(119, 'd');
    f.fake.base_shas.lock().unwrap().insert(119, sha('e'));
    let (entered, release) = f.fake.hold_comparison(&bh('e', 'd'));
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    entered.await.unwrap();
    sqlx::query("UPDATE users SET is_admin=false WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    release.send(()).unwrap();
    let (status, body) = reconciling.await.unwrap();
    sqlx::query("UPDATE users SET is_admin=true WHERE id=$1").bind(user).execute(&pool).await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let unchanged = row(&pool, task).await;
    assert_eq!((unchanged.0, unchanged.1, audits(&pool, task).await.len()), (sha('c'), "in_review".into(), 0));

    // A task at the attempt limit with changes requested may still record the
    // newer revision; attempts are not reset and the status stays, so the
    // response tells the operator that Forge will not finish it.
    let task = f.task("reconcile-at-limit", 120, 'c').await;
    f.fake.set_head(120, 'd');
    sqlx::query("UPDATE orchestration_tasks SET review_status='changes_requested', merge_attempts=$2 WHERE id=$1")
        .bind(task)
        .bind(CAP)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, body["data"]["reviewStatus"].clone()), (StatusCode::OK, json!("changes_requested")), "{body}");
    let after = row(&pool, task).await;
    assert_eq!((after.0, after.1, after.2), (sha('d'), "changes_requested".into(), CAP));

    // An existing `sensitive_blocked` is never cleared by a clean new head.
    let task = f.task("reconcile-keep-blocked", 102, 'c').await;
    sqlx::query("UPDATE orchestration_tasks SET review_status='sensitive_blocked' WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    f.fake.set_head(102, 'd');
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, body["data"]["sensitive"].clone()), (StatusCode::OK, json!(false)), "{body}");
    assert_eq!(row(&pool, task).await.1, "sensitive_blocked");

    // While a merge is in progress or unconfirmed (`approved`), nothing is reconciled.
    let task = f.task("reconcile-approved", 103, 'c').await;
    sqlx::query("UPDATE orchestration_tasks SET review_status='approved' WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    f.fake.set_head(103, 'd');
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, MERGE_UNCONFIRMED), "{body}");
    assert_eq!(row(&pool, task).await.0, sha('c'));

    // A provider read failure fails closed: nothing written.
    let task = f.task("reconcile-provider-down", 104, 'c').await;
    f.fake.set_head(104, 'd');
    f.fake.failing_pulls.lock().unwrap().insert(104);
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert!(status.is_server_error(), "{status} {body}");
    assert_eq!((row(&pool, task).await.0, audits(&pool, task).await.len()), (sha('c'), 0));

    // Approve paused at the provider merge call: a reconcile is refused by the
    // merge mutex. Fails if the reconcile skips the mutex (it would see
    // `approved` and answer `reconcile_merge_unconfirmed` instead).
    let task = f.task("reconcile-vs-merge", 105, 'c').await;
    f.report(task).await;
    let (entered, release) = f.fake.hold(105);
    let merging = tokio::spawn(approve(app.clone(), jwt.clone(), task));
    entered.await.unwrap();
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{body}");
    release.send(()).unwrap();
    assert_eq!(merging.await.unwrap().0, StatusCode::OK);
    assert_eq!(row(&pool, task).await.0, sha('c'));

    // Reconcile paused at the provider comparison: an approve is refused by the mutex.
    let task = f.task("merge-vs-reconcile", 106, 'c').await;
    f.fake.set_head(106, 'd');
    f.fake.base_shas.lock().unwrap().insert(106, sha('f'));
    let (entered, release) = f.fake.hold_comparison(&bh('f', 'd'));
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    entered.await.unwrap();
    let (status, body) = f.approve(task).await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, RUNNING), "{body}");
    release.send(()).unwrap();
    assert_eq!(reconciling.await.unwrap().0, StatusCode::OK);
    assert_eq!(row(&pool, task).await.0, sha('d'));

    // Row lock — the test holds the task row while the reconcile waits for it,
    // and in that time another writer stores a different head. The reconcile
    // re-decides on the locked row and refuses as stale. Fails if the decision
    // skips `lock_task` (it would decide on the old row and overwrite the head).
    let task = f.task("reconcile-row-lock", 107, 'c').await;
    f.fake.set_head(107, 'd');
    let mut hold = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *hold).await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *hold)
        .await
        .unwrap();
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    wait_for(&pool, &blocked_by(pid)).await;
    sqlx::query("UPDATE orchestration_tasks SET pr_head_sha=$2 WHERE id=$1")
        .bind(task)
        .bind(sha('e'))
        .execute(&mut *hold)
        .await
        .unwrap();
    hold.commit().await.unwrap();
    let (status, body) = reconciling.await.unwrap();
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "errors.self_fix.reconcile_stale"), "{body}");
    assert_eq!((row(&pool, task).await.0, audits(&pool, task).await.len()), (sha('e'), 0));

    // Reconcile and a verdict racing on the row lock, both orders. Each waits
    // behind the test's row lock; PostgreSQL grants it in arrival order.
    // (a) The verdict first: it binds to the old report, which then stops
    //     being current, so its rejection no longer blocks.
    let task = f.task("race-verdict-first", 108, 'c').await;
    let report = f.report(task).await;
    f.fake.set_head(108, 'd');
    let data = f.delivery(task).await;
    let mut hold = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *hold).await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *hold)
        .await
        .unwrap();
    let verdict = tokio::spawn(record_rejection(app.clone(), jwt.clone(), task, report, data));
    wait_for(&pool, &blocked_by(pid)).await;
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    wait_for(&pool, &blocked_count(2)).await;
    hold.rollback().await.unwrap();
    assert_eq!(verdict.await.unwrap().0, StatusCode::OK);
    assert_eq!(reconciling.await.unwrap().0, StatusCode::OK);
    assert_eq!(row(&pool, task).await.0, sha('d'));
    assert_eq!(f.rejected_flag(task).await, false, "a verdict on the replaced revision does not block");
    // (b) The reconcile first: the verdict prepared against the old revision
    //     then fails its version check; nothing is recorded.
    let task = f.task("race-reconcile-first", 109, 'c').await;
    let report = f.report(task).await;
    f.fake.set_head(109, 'd');
    let data = f.delivery(task).await;
    let mut hold = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *hold).await.unwrap();
    sqlx::query("SELECT 1 FROM orchestration_tasks WHERE id=$1 FOR UPDATE")
        .bind(task)
        .execute(&mut *hold)
        .await
        .unwrap();
    let reconciling = tokio::spawn(reconcile(app.clone(), jwt.clone(), task, 'c', 'd'));
    wait_for(&pool, &blocked_by(pid)).await;
    let verdict = tokio::spawn(record_rejection(app.clone(), jwt.clone(), task, report, data));
    wait_for(&pool, &blocked_count(2)).await;
    hold.rollback().await.unwrap();
    assert_eq!(reconciling.await.unwrap().0, StatusCode::OK);
    assert_eq!(verdict.await.unwrap().0, StatusCode::CONFLICT);
    let recorded: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance_review_decisions WHERE task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(recorded, 0);

    // Atomicity — DEFERRED constraint triggers (test-only) on the task row and
    // on the audit log record, for every committed transaction touching this
    // task, the stored head and the number of reconcile audit rows. In every
    // committed state the new head and its audit record exist together. Fails
    // if the head is written outside the decision transaction (before it: a
    // head without audit; after it: an audit without the head).
    let task = f.task("reconcile-atomic", 110, 'c').await;
    f.fake.set_head(110, 'd');
    for statement in [
        "CREATE TABLE reconcile_commit_log (seq bigserial PRIMARY KEY, head text, audits bigint NOT NULL)".to_string(),
        "CREATE FUNCTION log_reconcile_commit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         DECLARE task uuid := COALESCE(to_jsonb(NEW)->>'resource_id', to_jsonb(NEW)->>'id')::uuid; BEGIN \
         INSERT INTO reconcile_commit_log (head, audits) SELECT t.pr_head_sha, (SELECT count(*) FROM audit_log a \
         WHERE a.action = 'self_fix.head_reconciled' AND a.resource_id = t.id) FROM orchestration_tasks t WHERE t.id = task; \
         RETURN NULL; END $$"
            .to_string(),
        format!(
            "CREATE CONSTRAINT TRIGGER log_reconcile_commit AFTER UPDATE ON orchestration_tasks DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN (NEW.id = '{task}') EXECUTE FUNCTION log_reconcile_commit()"
        ),
        format!(
            "CREATE CONSTRAINT TRIGGER log_reconcile_audit AFTER INSERT ON audit_log DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN (NEW.resource_id = '{task}') EXECUTE FUNCTION log_reconcile_commit()"
        ),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    let (status, body) = reconcile(app.clone(), jwt.clone(), task, 'c', 'd').await;
    let committed: Vec<(Option<String>, i64)> =
        sqlx::query_as("SELECT head, audits FROM reconcile_commit_log ORDER BY seq").fetch_all(&pool).await.unwrap();
    for statement in [
        "DROP TRIGGER log_reconcile_commit ON orchestration_tasks",
        "DROP TRIGGER log_reconcile_audit ON audit_log",
        "DROP FUNCTION log_reconcile_commit()",
        "DROP TABLE reconcile_commit_log",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!committed.is_empty());
    assert!(
        committed.iter().all(|(head, audits)| (head.as_deref() == Some(sha('d').as_str())) == (*audits == 1)),
        "the new head and its audit record were committed separately: {committed:?}"
    );
}

/// A rejection prepared from a delivery read taken BEFORE the race.
async fn record_rejection(app: Router, jwt: String, task: Uuid, report: Uuid, data: Value) -> (StatusCode, Value) {
    let input = json!({"requestKey":Uuid::new_v4(),"reportId":report,"expectedVersion":data["taskVersion"],
        "expectedRevision":data["currentRevision"],"verdict":"rejected","reason":"Reviewed the exact revision.","humanMinutes":{"review":3}});
    call(&app, &jwt, "POST", &format!("/api/v1/self-fix/tasks/{task}/decisions"), input).await
}
