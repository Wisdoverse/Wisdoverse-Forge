//! End-to-end contract test for the orchestration worker bridge.
//!
//! Simulates the sidecar side by publishing a signed `TaskResult` envelope on
//! `orchestration.result.<agent_id>` and asserts the backend consumer decodes
//! it, resolves the participant, and hands the outcome to the `TaskWriter`.
//! Skips gracefully when no local NATS server is reachable so CI without
//! infrastructure still passes (matches the pattern used by `event_consumer`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agentforge_infra::nats::connect_nats;
use anyhow::Result;
use async_nats::jetstream::{self, stream};
use async_trait::async_trait;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use agentforge_core::orchestration_protocol::{SignedEnvelope, TaskOutcome, TaskResult, assign_subject};
use agentforge_jobs::{
    HmacSecretLookup, OrchestrationResultConsumerConfig, OrchestrationResultWorker, ParticipantLookup, TaskWriter,
    results_filter_for,
};

const CONTRACT_HMAC: &str = "contract-test-hmac-key";

#[derive(Clone, Default)]
struct FakeHmac {
    by_agent: Arc<HashMap<Uuid, String>>,
}

impl FakeHmac {
    fn with(agent_id: Uuid, secret: &str) -> Self {
        Self { by_agent: Arc::new(HashMap::from([(agent_id, secret.to_string())])) }
    }
}

#[async_trait]
impl HmacSecretLookup for FakeHmac {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>> {
        Ok(self.by_agent.get(&agent_id).cloned())
    }
}

#[derive(Clone, Default)]
struct FakeLookup {
    by_agent: Arc<HashMap<Uuid, Uuid>>,
}

#[async_trait]
impl ParticipantLookup for FakeLookup {
    async fn find_org(&self, agent_id: Uuid) -> Result<Option<Uuid>> {
        Ok(self.by_agent.get(&agent_id).copied())
    }
}

#[derive(Clone, Default)]
struct FakeWriter {
    applied: Arc<Mutex<Vec<(Uuid, TaskResult)>>>,
}

#[async_trait]
impl TaskWriter for FakeWriter {
    async fn apply(&self, organization_id: Uuid, result: TaskResult) -> Result<()> {
        self.applied.lock().await.push((organization_id, result));
        Ok(())
    }
}

/// Attempt to connect to a local NATS server. Returns `None` when the server
/// is unavailable or the configured auth is wrong so the test can skip rather
/// than fail — the CI matrix runs with and without NATS, and developers should
/// get a passing test suite on a clean checkout.
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

fn contract_result_config() -> OrchestrationResultConsumerConfig {
    let suffix = Uuid::now_v7().simple().to_string();
    let subject_prefix = format!("orchestration.result.contract.{suffix}");
    OrchestrationResultConsumerConfig {
        stream_name: format!("ORCH_RESULTS_CONTRACT_{suffix}"),
        durable_name: format!("orch-result-contract-{suffix}"),
        filter_subject: results_filter_for(&subject_prefix),
        subject_prefix,
    }
}

fn contract_result_subject(config: &OrchestrationResultConsumerConfig, agent_id: Uuid) -> String {
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

/// Poll `FakeWriter` until the expected count of results has been applied or
/// the budget expires. `run()` delivers messages through an async channel so
/// we can't rely on publish order alone.
async fn wait_for_applied(writer: &FakeWriter, expected: usize) -> bool {
    for _ in 0..40 {
        if writer.applied.lock().await.len() >= expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn sidecar_to_backend_result_round_trip() {
    let Some(client) = try_connect().await else {
        return;
    };
    let consumer_config = contract_result_config();
    ensure_results_stream(client.clone(), &consumer_config).await;

    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
    let writer = FakeWriter::default();

    let hmac = FakeHmac::with(agent_id, CONTRACT_HMAC);
    let worker = OrchestrationResultWorker::connect_with_config(
        client.clone(),
        lookup,
        writer.clone(),
        hmac,
        consumer_config.clone(),
    )
    .await
    .expect("connect worker");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { worker.run(shutdown_rx).await });
    let js = jetstream::new(client.clone());

    let now_ts = chrono::Utc::now().timestamp();
    let completed = TaskResult {
        delivery_id: Some(Uuid::now_v7()),
        attempt: Some(1),
        task_id: Uuid::now_v7(),
        agent_id,
        outcome: TaskOutcome::Completed { stdout: "round trip ok".into() },
    };
    let env = SignedEnvelope::sign(CONTRACT_HMAC.as_bytes(), &agent_id.to_string(), now_ts, &completed)
        .expect("sign envelope");
    let bytes = serde_json::to_vec(&env).unwrap();
    js.publish(contract_result_subject(&consumer_config, agent_id), bytes.into())
        .await
        .expect("publish result accepted")
        .await
        .expect("publish result ack");

    let failed = TaskResult {
        delivery_id: Some(Uuid::now_v7()),
        attempt: Some(2),
        task_id: Uuid::now_v7(),
        agent_id,
        outcome: TaskOutcome::Failed { stderr: "bad".into(), exit_code: Some(7) },
    };
    let env =
        SignedEnvelope::sign(CONTRACT_HMAC.as_bytes(), &agent_id.to_string(), now_ts, &failed).expect("sign envelope");
    let bytes = serde_json::to_vec(&env).unwrap();
    js.publish(contract_result_subject(&consumer_config, agent_id), bytes.into())
        .await
        .expect("publish result accepted")
        .await
        .expect("publish result ack");

    assert!(wait_for_applied(&writer, 2).await, "worker did not observe both results within 2s");

    let applied = writer.applied.lock().await.clone();
    let orgs: Vec<Uuid> = applied.iter().map(|(o, _)| *o).collect();
    assert!(orgs.iter().all(|o| *o == org_id), "orgs = {orgs:?}");
    let task_ids: Vec<Uuid> = applied.iter().map(|(_, r)| r.task_id).collect();
    assert!(task_ids.contains(&completed.task_id), "missing completed id");
    assert!(task_ids.contains(&failed.task_id), "missing failed id");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    cleanup_results_stream(client.clone(), &consumer_config).await;
}

#[tokio::test]
async fn worker_ignores_results_for_unregistered_agent() {
    let Some(client) = try_connect().await else {
        return;
    };
    let consumer_config = contract_result_config();
    ensure_results_stream(client.clone(), &consumer_config).await;

    // Lookup is empty — no participant row exists, so the backend refuses the
    // message. This mirrors the real-world scenario of a container that was
    // removed from the DB but whose sidecar lingers briefly.
    let lookup = FakeLookup::default();
    let writer = FakeWriter::default();

    let agent_id = Uuid::now_v7();
    let hmac = FakeHmac::with(agent_id, CONTRACT_HMAC);
    let worker = OrchestrationResultWorker::connect_with_config(
        client.clone(),
        lookup,
        writer.clone(),
        hmac,
        consumer_config.clone(),
    )
    .await
    .expect("connect worker");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { worker.run(shutdown_rx).await });
    let js = jetstream::new(client.clone());

    let result = TaskResult {
        delivery_id: Some(Uuid::now_v7()),
        attempt: Some(1),
        task_id: Uuid::now_v7(),
        agent_id,
        outcome: TaskOutcome::Completed { stdout: String::new() },
    };
    let env =
        SignedEnvelope::sign(CONTRACT_HMAC.as_bytes(), &agent_id.to_string(), chrono::Utc::now().timestamp(), &result)
            .unwrap();
    js.publish(contract_result_subject(&consumer_config, agent_id), serde_json::to_vec(&env).unwrap().into())
        .await
        .expect("publish result accepted")
        .await
        .expect("publish result ack");

    // Give the worker time to receive-and-drop. A silent ignore is the
    // correct behaviour; no `applied` entries should accumulate.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(writer.applied.lock().await.is_empty(), "writer should have seen no results");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    cleanup_results_stream(client.clone(), &consumer_config).await;
}

#[test]
fn assignment_subject_is_scoped_per_agent() {
    // This must stay a pure subject-contract check. Publishing to the live
    // `orchestration.assigned.*` subject also writes into the shared
    // JetStream workqueue and leaves test messages for production workers.
    let agent_a = Uuid::now_v7();
    let agent_b = Uuid::now_v7();

    assert_ne!(assign_subject(agent_a), assign_subject(agent_b));
    assert_eq!(assign_subject(agent_a), format!("orchestration.assigned.{agent_a}"));
    assert_eq!(assign_subject(agent_b), format!("orchestration.assigned.{agent_b}"));
}
