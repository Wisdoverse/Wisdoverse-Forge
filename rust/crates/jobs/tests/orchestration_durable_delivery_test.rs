//! Integration coverage for Workstream D durable delivery semantics.
//!
//! 1. Assignment outbox rows must not publish before the DB transaction
//!    commits.
//! 2. Result replay with the same `delivery_id` must be idempotent.
//! 3. Result consumer replay after restart must not reapply task effects.
//! 4. Committed assignment outbox backlog must drain after publisher restart.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use agentforge_core::orchestration_protocol::{
    ORCHESTRATION_ASSIGNMENTS_STREAM, SignedEnvelope, TaskAssignment, TaskOutcome, TaskResult, assign_subject,
    assign_subject_wildcard,
};
use agentforge_infra::nats::connect_nats;
use agentforge_jobs::{
    OrchestrationOutboxPublisher, OrchestrationResultConsumerConfig, SqlxHmacSecretLookup, SqlxParticipantLookup,
    SqlxTaskWriter, TaskWriter, handle_message_with_subject_prefix, insert_assignment_outbox_in_tx, results_filter_for,
};
use async_nats::jetstream::consumer::{self, PullConsumer, pull};
use async_nats::jetstream::{self, stream};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

const CONTRACT_HMAC: &str = "durable-delivery-contract-hmac";

async fn try_connect() -> Option<async_nats::Client> {
    let candidates = nats_candidates();
    let mut failures = Vec::new();
    for (label, url) in candidates {
        match tokio::time::timeout(Duration::from_millis(500), connect_nats(&url)).await {
            Ok(Ok(client)) => return Some(client),
            Ok(Err(err)) => failures.push(format!("{label}: {err}")),
            Err(_) => failures.push(format!("{label}: timeout")),
        }
    }
    if failures.is_empty() {
        eprintln!("skipping: no NATS connection candidates available");
    } else {
        eprintln!("skipping: failed to connect to project NATS ({})", failures.join("; "));
    }
    None
}

fn nats_candidates() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let docker_env = read_docker_env();

    let mut push = |label: String, url: String| {
        if seen.insert(url.clone()) {
            out.push((label, url));
        }
    };

    if let Ok(url) = std::env::var("NATS_URL") {
        push("env:NATS_URL".to_string(), url);
    }
    if let Some(url) = docker_env.get("NATS_URL").cloned() {
        push("docker/.env:NATS_URL".to_string(), url);
    }

    let port = std::env::var("NATS_PORT")
        .ok()
        .or_else(|| docker_env.get("NATS_PORT").cloned())
        .unwrap_or_else(|| "4222".to_string());
    if let Some(password) =
        std::env::var("NATS_BACKEND_PASSWORD").ok().or_else(|| docker_env.get("NATS_BACKEND_PASSWORD").cloned())
    {
        push("docker/.env backend user".to_string(), format!("nats://backend:{password}@127.0.0.1:{port}"));
    }

    push("localhost anonymous".to_string(), format!("nats://127.0.0.1:{port}"));
    out
}

fn read_docker_env() -> HashMap<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docker/.env");
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

async fn ensure_assignments_stream(client: async_nats::Client) {
    let js = jetstream::new(client);
    js.create_or_update_stream(stream::Config {
        name: ORCHESTRATION_ASSIGNMENTS_STREAM.to_string(),
        subjects: vec![assign_subject_wildcard()],
        retention: stream::RetentionPolicy::WorkQueue,
        storage: stream::StorageType::File,
        max_age: Duration::from_secs(24 * 60 * 60),
        discard: stream::DiscardPolicy::Old,
        max_messages_per_subject: 1_000,
        ..Default::default()
    })
    .await
    .expect("ensure assignments stream");
}

fn result_contract_config() -> OrchestrationResultConsumerConfig {
    let suffix = Uuid::now_v7().simple().to_string();
    let subject_prefix = format!("orchestration.result.contract.{suffix}");
    OrchestrationResultConsumerConfig {
        stream_name: format!("ORCH_RESULTS_CONTRACT_{suffix}"),
        durable_name: format!("orch-result-contract-{suffix}"),
        filter_subject: results_filter_for(&subject_prefix),
        subject_prefix,
    }
}

fn result_contract_subject(config: &OrchestrationResultConsumerConfig, agent_id: Uuid) -> String {
    format!("{}.{}", config.subject_prefix, agent_id)
}

async fn ensure_results_stream(client: async_nats::Client, config: &OrchestrationResultConsumerConfig) {
    let js = jetstream::new(client);
    js.create_or_update_stream(stream::Config {
        name: config.stream_name.clone(),
        subjects: vec![config.filter_subject.clone()],
        retention: stream::RetentionPolicy::WorkQueue,
        storage: stream::StorageType::File,
        max_age: Duration::from_secs(24 * 60 * 60),
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .expect("ensure results stream");
}

async fn cleanup_results_stream(client: async_nats::Client, config: &OrchestrationResultConsumerConfig) {
    let js = jetstream::new(client);
    let _ = js.delete_stream(&config.stream_name).await;
}

async fn connect_result_consumer(
    client: async_nats::Client,
    config: &OrchestrationResultConsumerConfig,
    ack_wait: Duration,
) -> PullConsumer {
    let js = jetstream::new(client);
    let stream = js.get_stream(&config.stream_name).await.expect("get results stream");
    stream
        .get_or_create_consumer(
            &config.durable_name,
            pull::Config {
                durable_name: Some(config.durable_name.clone()),
                ack_policy: consumer::AckPolicy::Explicit,
                ack_wait,
                filter_subject: config.filter_subject.clone(),
                max_deliver: 5,
                ..Default::default()
            },
        )
        .await
        .expect("create temporary result consumer")
}

async fn next_assignment(consumer: &PullConsumer, timeout: Duration) -> Option<async_nats::jetstream::Message> {
    let mut messages = consumer.fetch().max_messages(1).expires(timeout).messages().await.ok()?;
    match tokio::time::timeout(timeout + Duration::from_millis(100), futures::StreamExt::next(&mut messages)).await {
        Ok(Some(Ok(message))) => Some(message),
        _ => None,
    }
}

async fn wait_for_published_at(pool: &PgPool, delivery_id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
    for _ in 0..40 {
        let published_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT published_at FROM orchestration_outbox WHERE id = $1")
                .bind(delivery_id)
                .fetch_one(pool)
                .await
                .expect("query outbox row");
        if published_at.is_some() {
            return published_at;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

async fn seed_org_user_agent(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

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
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'test-agent', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");

    (org_id, user_id, agent_id)
}

fn signed_result(secret: &str, agent_id: Uuid, result: &TaskResult) -> Vec<u8> {
    let envelope =
        SignedEnvelope::sign(secret.as_bytes(), &agent_id.to_string(), chrono::Utc::now().timestamp(), result)
            .expect("sign result envelope");
    serde_json::to_vec(&envelope).expect("encode result envelope")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn assignment_outbox_publishes_only_after_commit(pool: PgPool) {
    let Some(client) = try_connect().await else {
        return;
    };
    ensure_assignments_stream(client.clone()).await;

    let (org_id, _user_id, agent_id) = seed_org_user_agent(&pool).await;
    let task_id = Uuid::now_v7();
    let delivery_id = Uuid::now_v7();
    let subject = assign_subject(agent_id);
    let consumer_name = format!("orch-assign-contract-{}", Uuid::now_v7().simple());
    let assignment = TaskAssignment {
        delivery_id: Some(delivery_id),
        attempt: Some(1),
        lease_expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(15)),
        task_id,
        agent_id,
        title: "durable assignment".into(),
        task: "echo durable".into(),
        message: "publish after commit".into(),
        priority: "normal".into(),
        context_envelope: None,
    };

    let js = jetstream::new(client.clone());
    let stream = js.get_stream(ORCHESTRATION_ASSIGNMENTS_STREAM).await.expect("get assignments stream");
    let consumer: PullConsumer = stream
        .get_or_create_consumer(
            &consumer_name,
            pull::Config {
                durable_name: Some(consumer_name.clone()),
                ack_policy: consumer::AckPolicy::Explicit,
                filter_subject: subject.clone(),
                ..Default::default()
            },
        )
        .await
        .expect("create temporary assignment consumer");

    let publisher = OrchestrationOutboxPublisher::new(pool.clone(), client.clone());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(async move { publisher.run(shutdown_rx).await });

    let mut tx = pool.begin().await.expect("begin outbox tx");
    insert_assignment_outbox_in_tx(&mut tx, org_id, task_id, &assignment).await.expect("insert outbox row");

    assert!(
        next_assignment(&consumer, Duration::from_millis(300)).await.is_none(),
        "assignment published before commit"
    );

    tx.commit().await.expect("commit outbox tx");

    let message =
        next_assignment(&consumer, Duration::from_secs(2)).await.expect("assignment was not published after commit");
    let published: TaskAssignment = serde_json::from_slice(&message.payload).expect("decode assignment payload");
    assert_eq!(published.delivery_id, assignment.delivery_id);
    assert_eq!(published.task_id, assignment.task_id);
    assert_eq!(published.agent_id, assignment.agent_id);
    message.ack().await.expect("ack assignment");

    let published_at = wait_for_published_at(&pool, delivery_id).await;
    assert!(published_at.is_some(), "outbox row was not marked published");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), worker).await;
    stream.delete_consumer(&consumer_name).await.expect("delete temporary consumer");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn assignment_outbox_backlog_drains_after_publisher_restart(pool: PgPool) {
    let Some(client) = try_connect().await else {
        return;
    };
    ensure_assignments_stream(client.clone()).await;

    let (org_id, _user_id, agent_id) = seed_org_user_agent(&pool).await;
    let task_id = Uuid::now_v7();
    let delivery_id = Uuid::now_v7();
    let subject = assign_subject(agent_id);
    let consumer_name = format!("orch-assign-contract-{}", Uuid::now_v7().simple());
    let assignment = TaskAssignment {
        delivery_id: Some(delivery_id),
        attempt: Some(1),
        lease_expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(15)),
        task_id,
        agent_id,
        title: "durable backlog assignment".into(),
        task: "echo backlog".into(),
        message: "publish after publisher restart".into(),
        priority: "normal".into(),
        context_envelope: None,
    };

    let js = jetstream::new(client.clone());
    let stream = js.get_stream(ORCHESTRATION_ASSIGNMENTS_STREAM).await.expect("get assignments stream");
    let consumer: PullConsumer = stream
        .get_or_create_consumer(
            &consumer_name,
            pull::Config {
                durable_name: Some(consumer_name.clone()),
                ack_policy: consumer::AckPolicy::Explicit,
                filter_subject: subject,
                ..Default::default()
            },
        )
        .await
        .expect("create temporary assignment consumer");

    let mut tx = pool.begin().await.expect("begin backlog tx");
    insert_assignment_outbox_in_tx(&mut tx, org_id, task_id, &assignment).await.expect("insert backlog outbox row");
    tx.commit().await.expect("commit backlog tx");

    assert!(
        next_assignment(&consumer, Duration::from_millis(300)).await.is_none(),
        "assignment published before publisher restart"
    );

    let publisher = OrchestrationOutboxPublisher::new(pool.clone(), client.clone());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(async move { publisher.run(shutdown_rx).await });

    let message = next_assignment(&consumer, Duration::from_secs(2)).await.expect("assignment backlog was not drained");
    let published: TaskAssignment =
        serde_json::from_slice(&message.payload).expect("decode backlog assignment payload");
    assert_eq!(published.delivery_id, assignment.delivery_id);
    assert_eq!(published.task_id, assignment.task_id);
    message.ack().await.expect("ack backlog assignment");

    let published_at = wait_for_published_at(&pool, delivery_id).await;
    assert!(published_at.is_some(), "backlog outbox row was not marked published");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), worker).await;
    stream.delete_consumer(&consumer_name).await.expect("delete temporary consumer");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn result_replay_is_idempotent_by_delivery_id(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_user_agent(&pool).await;
    let delivery_id = Uuid::now_v7();
    let task_id = Uuid::now_v7();

    sqlx::query(
        r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'test-agent', ARRAY['codex'], 'busy', NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("seed participant");

    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
           VALUES ($1, $2, 'durable result', 'working', $3, $4, 'normal', 1, NOW() + INTERVAL '15 minutes', $5, NOW())"#,
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(delivery_id)
    .execute(&pool)
    .await
    .expect("seed working task");
    sqlx::query(
        r#"INSERT INTO task_runs
               (organization_id, workspace_id, orchestration_task_id, agent_id,
                idempotency_key, status, started_at, capability_profile)
           VALUES ($1, $1, $2, $3, $4, 'working', NOW(), '{}')"#,
    )
    .bind(org_id)
    .bind(task_id)
    .bind(agent_id)
    .bind(delivery_id.to_string())
    .execute(&pool)
    .await
    .expect("seed task run");

    let writer = SqlxTaskWriter::new(pool.clone());
    let result = TaskResult {
        delivery_id: Some(delivery_id),
        attempt: Some(1),
        task_id,
        agent_id,
        outcome: TaskOutcome::Completed { stdout: "done".into() },
    };

    writer.apply(org_id, result.clone()).await.expect("first apply");
    writer.apply(org_id, result).await.expect("replay apply");

    let status: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("query task status");
    assert_eq!(status, "completed");

    let participant_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(agent_id)
            .fetch_one(&pool)
            .await
            .expect("query participant status");
    assert_eq!(participant_status, "available");

    let inbox_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_inbox WHERE delivery_id = $1")
        .bind(delivery_id)
        .fetch_one(&pool)
        .await
        .expect("count inbox rows");
    assert_eq!(inbox_rows, 1, "replay must not insert a second inbox row");

    let (run_status, finished_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, finished_at FROM task_runs WHERE idempotency_key = $1")
            .bind(delivery_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("query task run");
    assert_eq!(run_status, "completed");
    assert!(finished_at.is_some(), "result apply must close the matching task_run");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn failed_result_persists_owner_inbox_notification(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_user_agent(&pool).await;
    let delivery_id = Uuid::now_v7();
    let task_id = Uuid::now_v7();

    sqlx::query(
        r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'test-agent', ARRAY['codex'], 'busy', NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("seed participant");

    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
           VALUES ($1, $2, 'durable failed result', 'working', $3, $4, 'normal', 1, NOW() + INTERVAL '15 minutes', $5, NOW())"#,
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(delivery_id)
    .execute(&pool)
    .await
    .expect("seed working task");

    let writer = SqlxTaskWriter::new(pool.clone());
    writer
        .apply(
            org_id,
            TaskResult {
                delivery_id: Some(delivery_id),
                attempt: Some(1),
                task_id,
                agent_id,
                outcome: TaskOutcome::Failed { stderr: "boom".into(), exit_code: Some(42) },
            },
        )
        .await
        .expect("apply failed result");

    let notification_id = format!("task-owner:{task_id}:failed");
    let (notification_type, task_title, message, read): (String, String, String, bool) = sqlx::query_as(
        r#"SELECT notification_type, task_title, message, read
           FROM inbox_notifications
           WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
    )
    .bind(notification_id)
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("query owner inbox notification");

    assert_eq!(notification_type, "failed");
    assert_eq!(task_title, "durable failed result");
    assert!(message.contains("test-agent failed to complete this task: boom"), "message = {message}");
    assert!(!read, "new failure notification must be unread");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn result_redelivery_after_consumer_restart_is_idempotent(pool: PgPool) {
    let Some(client) = try_connect().await else {
        return;
    };
    let consumer_config = result_contract_config();
    ensure_results_stream(client.clone(), &consumer_config).await;

    let (org_id, user_id, agent_id) = seed_org_user_agent(&pool).await;
    let delivery_id = Uuid::now_v7();
    let task_id = Uuid::now_v7();
    let ack_wait = Duration::from_secs(1);

    sqlx::query("UPDATE agents SET hmac_secret = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(CONTRACT_HMAC)
        .execute(&pool)
        .await
        .expect("seed agent hmac");
    sqlx::query(
        r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'test-agent', ARRAY['codex'], 'busy', NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("seed participant");
    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
           VALUES ($1, $2, 'durable replay', 'working', $3, $4, 'normal', 1, NOW() + INTERVAL '15 minutes', $5, NOW())"#,
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(delivery_id)
    .execute(&pool)
    .await
    .expect("seed working task");

    let lookup = SqlxParticipantLookup::new(pool.clone());
    let writer = SqlxTaskWriter::new(pool.clone());
    let hmac = SqlxHmacSecretLookup::new(pool.clone());
    let result = TaskResult {
        delivery_id: Some(delivery_id),
        attempt: Some(1),
        task_id,
        agent_id,
        outcome: TaskOutcome::Completed { stdout: "done after restart".into() },
    };

    let js = jetstream::new(client.clone());
    js.publish(
        result_contract_subject(&consumer_config, agent_id),
        signed_result(CONTRACT_HMAC, agent_id, &result).into(),
    )
    .await
    .expect("publish result accepted")
    .await
    .expect("publish result ack");

    {
        let consumer = connect_result_consumer(client.clone(), &consumer_config, ack_wait).await;
        let first_delivery =
            next_assignment(&consumer, Duration::from_secs(2)).await.expect("fetch first result delivery");
        let subject = first_delivery.subject.to_string();
        let payload = first_delivery.payload.to_vec();
        handle_message_with_subject_prefix(
            &lookup,
            &writer,
            &hmac,
            &consumer_config.subject_prefix,
            &subject,
            &payload,
        )
        .await
        .expect("apply first result delivery");
    }

    let status_after_first_apply: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("query task after first apply");
    assert_eq!(status_after_first_apply, "completed");

    tokio::time::sleep(ack_wait + Duration::from_millis(250)).await;

    let restarted_consumer = connect_result_consumer(client.clone(), &consumer_config, ack_wait).await;
    let replayed =
        next_assignment(&restarted_consumer, Duration::from_secs(2)).await.expect("fetch replayed result delivery");
    let subject = replayed.subject.to_string();
    let payload = replayed.payload.to_vec();
    handle_message_with_subject_prefix(&lookup, &writer, &hmac, &consumer_config.subject_prefix, &subject, &payload)
        .await
        .expect("replay after restart");
    replayed.ack().await.expect("ack replayed result");

    let final_status: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("query final task status");
    assert_eq!(final_status, "completed");

    let participant_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(agent_id)
            .fetch_one(&pool)
            .await
            .expect("query participant status");
    assert_eq!(participant_status, "available");

    let inbox_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_inbox WHERE delivery_id = $1")
        .bind(delivery_id)
        .fetch_one(&pool)
        .await
        .expect("count inbox rows");
    assert_eq!(inbox_rows, 1, "replayed delivery must not insert a second inbox row");

    cleanup_results_stream(client, &consumer_config).await;
}
