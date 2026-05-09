use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::metrics::{DashboardMetrics, ReviewLatency, Store as MetricsStore};
use agentforge_orchestrator::review::{CodeReview, ReviewState};
use agentforge_orchestrator::state::AppState;
use agentforge_orchestrator::task::{Task, TaskPriority, TaskState};

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

fn metrics_request(path: &str, org_id: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", org_id)
        .body(Body::empty())
        .unwrap()
}

fn seeded_task(org_id: &str, created_by: &str, session_id: Option<&str>, title: &str) -> Task {
    Task {
        id: String::new(),
        workflow_id: None,
        title: title.to_string(),
        description: String::new(),
        state: TaskState::Pending,
        priority: TaskPriority::Normal,
        assigned_to: None,
        review_id: None,
        agentforge_session_id: session_id.map(ToString::to_string),
        depends_on: Vec::new(),
        created_by: created_by.to_string(),
        org_id: org_id.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

async fn seed_completed_task(state: &AppState, org_id: &str, created_by: &str, session_id: &str, title: &str) -> Task {
    let task_store = state.task_store.as_ref().expect("task store").clone();
    let mut task = seeded_task(org_id, created_by, Some(session_id), title);
    task_store.create(&mut task).await.expect("create task");
    task_store.update_state(&task.id, org_id, TaskState::Completed).await.expect("complete task");
    task
}

#[tokio::test]
async fn metrics_return_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let (status, body) = json_response(app, metrics_request("/api/v1/metrics/dashboard", "org-test")).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn dashboard_metrics_reflect_real_task_and_review_data() {
    let state = AppState::test_review_internal_token("secret-token", "org-test", "cli-user");
    let task_store = state.task_store.as_ref().expect("task store").clone();
    let review_store = state.review_store.as_ref().expect("review store").clone();

    let mut pending = seeded_task("org-test", "cli-user", None, "Pending");
    task_store.create(&mut pending).await.expect("create pending task");

    let mut completed = seeded_task("org-test", "cli-user", Some("session-1"), "Completed");
    task_store.create(&mut completed).await.expect("create completed task seed");
    task_store.update_state(&completed.id, "org-test", TaskState::Completed).await.expect("complete task");

    let mut review = CodeReview {
        id: String::new(),
        task_id: pending.id.clone(),
        session_id: "session-review".to_string(),
        diff_ref: "manual".to_string(),
        diff_snapshot: None,
        state: ReviewState::Pending,
        assigned_to: None,
        org_id: "org-test".to_string(),
        created_by: "cli-user".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    review_store.create(&mut review).await.expect("create review");

    let app = state.router();
    let (status, body) = json_response(app, metrics_request("/api/v1/metrics/dashboard", "org-test")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["metrics"]["activeTasks"], 1);
    assert_eq!(body["metrics"]["completedToday"], 1);
    assert_eq!(body["metrics"]["activeAgents"], 0);
    assert_eq!(body["metrics"]["pendingReviews"], 1);
}

#[tokio::test]
async fn agent_and_latency_metrics_aggregate_from_real_store_data() {
    let state = AppState::test_review_internal_token("secret-token", "org-test", "cli-user");
    let review_store = state.review_store.as_ref().expect("review store").clone();

    let task = seed_completed_task(&state, "org-test", "cli-user", "session-42", "Agent task").await;

    let mut review = CodeReview {
        id: String::new(),
        task_id: task.id.clone(),
        session_id: "session-review".to_string(),
        diff_ref: "manual".to_string(),
        diff_snapshot: None,
        state: ReviewState::Pending,
        assigned_to: None,
        org_id: "org-test".to_string(),
        created_by: "cli-user".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    review_store.create(&mut review).await.expect("create review");
    review_store.update_state(&review.id, "org-test", ReviewState::Approved).await.expect("approve review");

    let app = state.router();

    let (status, agents) = json_response(app.clone(), metrics_request("/api/v1/metrics/agents", "org-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(agents["ok"], true);
    assert!(agents.get("cached").is_none());
    let items = agents["agents"].as_array().expect("agents array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["participantId"], "session-42");
    assert_eq!(items[0]["tasksCompleted"], 1);
    assert_eq!(items[0]["provider"], "unknown");

    let (status, cached_agents) =
        json_response(app.clone(), metrics_request("/api/v1/metrics/agents", "org-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cached_agents["ok"], true);
    assert_eq!(cached_agents["cached"], true);
    assert_eq!(cached_agents["agents"], agents["agents"]);

    let (status, latency) =
        json_response(app.clone(), metrics_request("/api/v1/metrics/reviews/latency", "org-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latency["ok"], true);
    assert!(latency.get("cached").is_none());
    assert!(latency["latency"]["avgHours"].as_f64().expect("avgHours") >= 0.0);
    assert!(latency["latency"]["p50Hours"].as_f64().expect("p50Hours") >= 0.0);
    assert!(latency["latency"]["p95Hours"].as_f64().expect("p95Hours") >= 0.0);

    let (status, cached_latency) =
        json_response(app, metrics_request("/api/v1/metrics/reviews/latency", "org-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cached_latency["ok"], true);
    assert_eq!(cached_latency["cached"], true);
    assert_eq!(cached_latency["latency"], latency["latency"]);
}

#[tokio::test]
async fn agent_metrics_use_participant_display_name_and_provider() {
    let state = AppState::test_review_internal_token("secret-token", "org-test", "cli-user");
    let agent_directory = state.agent_directory.as_ref().expect("agent directory").clone();

    agent_directory
        .upsert_agent("org-test", "session-42", "claude", "Claude Reviewer")
        .await
        .expect("seed agent participant");

    seed_completed_task(&state, "org-test", "cli-user", "session-42", "Seed agent task").await;

    let app = state.router();
    let (status, body) = json_response(app, metrics_request("/api/v1/metrics/agents", "org-test")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["agents"][0]["displayName"], "Claude Reviewer");
    assert_eq!(body["agents"][0]["provider"], "claude");
}

#[tokio::test]
async fn agent_metrics_limit_matches_go_top_twenty() {
    let state = AppState::test_review_internal_token("secret-token", "org-overflow", "cli-user");

    for index in 0..21 {
        let session_id = format!("session-{index:02}");
        let title = format!("Task {index:02}");
        seed_completed_task(&state, "org-overflow", "cli-user", &session_id, &title).await;
    }

    let app = state.router();
    let (status, body) = json_response(app, metrics_request("/api/v1/metrics/agents", "org-overflow")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["agents"].as_array().expect("agents").len(), 20);
}

#[tokio::test]
async fn metrics_internal_failures_use_go_error_text() {
    let state = AppState::test_internal_token("secret-token").with_metrics_store(Arc::new(FailingMetricsStore));
    let app = state.router();

    let (status, body) = json_response(app, metrics_request("/api/v1/metrics/dashboard", "org-test")).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "metrics unavailable"}));
}

struct FailingMetricsStore;

#[async_trait::async_trait]
impl MetricsStore for FailingMetricsStore {
    async fn dashboard(&self, _org_id: &str) -> anyhow::Result<DashboardMetrics> {
        anyhow::bail!("boom")
    }

    async fn agent_leaderboard(
        &self,
        _org_id: &str,
    ) -> anyhow::Result<Vec<agentforge_orchestrator::metrics::AgentMetric>> {
        anyhow::bail!("boom")
    }

    async fn review_latency(&self, _org_id: &str) -> anyhow::Result<ReviewLatency> {
        anyhow::bail!("boom")
    }
}
