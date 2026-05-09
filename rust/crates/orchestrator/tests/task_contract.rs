use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;
use agentforge_orchestrator::task::{Task, TaskPriority, TaskState};

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn tasks_return_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn tasks_support_create_list_get_and_transition_round_trip() {
    let app = AppState::test_task_internal_token("secret-token", "org-test", "cli-user").router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Contract task","description":"Round trip","priority":"high"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["ok"], true);
    assert_eq!(created["task"]["title"], "Contract task");
    assert_eq!(created["task"]["description"], "Round trip");
    assert_eq!(created["task"]["state"], "pending");
    assert_eq!(created["task"]["priority"], "high");
    assert_eq!(created["task"]["createdBy"], "cli-user");
    assert_eq!(created["task"]["orgId"], "org-test");
    let task_id = created["task"]["id"].as_str().expect("task id").to_string();

    let list_req = Request::builder()
        .method("GET")
        .uri("/api/v1/tasks?state=pending")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed) = json_response(app.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["ok"], true);
    let tasks = listed["tasks"].as_array().expect("tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], task_id);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/tasks/{task_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched) = json_response(app.clone(), get_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["task"]["id"], task_id);
    assert_eq!(fetched["task"]["title"], "Contract task");

    let transition_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/tasks/{task_id}/transition"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"state":"assigned"}"#))
        .unwrap();
    let (status, transitioned) = json_response(app, transition_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(transitioned["task"]["id"], task_id);
    assert_eq!(transitioned["task"]["state"], "assigned");
}

#[tokio::test]
async fn tasks_reject_invalid_transitions() {
    let app = AppState::test_task_internal_token("secret-token", "org-test", "cli-user").router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Transition contract"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = created["task"]["id"].as_str().expect("task id");

    let invalid_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/tasks/{task_id}/transition"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"state":"completed"}"#))
        .unwrap();
    let (status, body) = json_response(app, invalid_req).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "invalid transition");
    assert_eq!(body["from"], "pending");
    assert_eq!(body["to"], "completed");
    assert_eq!(body["allowed"], serde_json::json!(["assigned"]));
}

#[tokio::test]
async fn assign_with_agent_provider_starts_session_and_moves_task_to_working() {
    let state = AppState::test_task_internal_token("secret-token", "org-test", "cli-user")
        .with_outbound_mcp_test_success("agent-42");
    let task_store = state.task_store.as_ref().expect("task store").clone();

    let mut task = Task {
        id: String::new(),
        workflow_id: None,
        title: "Implement runtime".to_string(),
        description: "Port Temporal worker startup".to_string(),
        state: TaskState::Pending,
        priority: TaskPriority::Normal,
        assigned_to: None,
        review_id: None,
        agentforge_session_id: None,
        depends_on: Vec::new(),
        created_by: "cli-user".to_string(),
        org_id: "org-test".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    task_store.create(&mut task).await.expect("create task");

    let app = state.router();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/tasks/{}/assign", task.id))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"participantId":"agent-user","agentProvider":"claude","projectId":"proj-1"}"#))
        .unwrap();

    let (status, body) = json_response(app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["task"]["state"], "assigned");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let refreshed = task_store.get_by_id(&task.id, "org-test").await.expect("reload task");
    assert_eq!(refreshed.agentforge_session_id.as_deref(), Some("agent-42"));
    assert_eq!(refreshed.state, TaskState::Working);
}
