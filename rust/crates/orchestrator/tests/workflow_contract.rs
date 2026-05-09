use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn workflows_return_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn workflows_support_create_list_get_run_status_signal_history_and_cancel_round_trip() {
    let app = AppState::test_workflow_internal_token("secret-token", "org-test", "cli-user").router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Release Flow","description":"Deploy path","nodes":[{"name":"build","type":"agent_task"},{"name":"review","type":"human_review","dependsOn":["build"]}]}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["workflow"]["status"], "draft");
    assert_eq!(created["nodes"].as_array().expect("nodes").len(), 2);
    let workflow_id = created["workflow"]["id"].as_str().expect("workflow id").to_string();
    let review_node_id = created["nodes"][1]["id"].as_str().expect("review node id").to_string();

    let list_req = Request::builder()
        .method("GET")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed) = json_response(app.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["workflows"].as_array().expect("workflows").len(), 1);
    assert_eq!(listed["workflows"][0]["id"], workflow_id);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/workflows/{workflow_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched) = json_response(app.clone(), get_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["workflow"]["id"], workflow_id);
    assert_eq!(fetched["nodes"][1]["dependsOn"], serde_json::json!(["build"]));

    let run_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{workflow_id}/run"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, started) = json_response(app.clone(), run_req).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(started["status"], "started");
    assert_eq!(started["workflow"]["status"], "running");

    let status_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/workflows/{workflow_id}/status"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, running) = json_response(app.clone(), status_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(running["workflow"]["status"], "running");
    assert_eq!(running["nodes"][0]["status"], "completed");
    assert_eq!(running["nodes"][1]["status"], "running");

    let signal_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{workflow_id}/signal"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"nodeId":"{}","decision":"approve"}}"#, review_node_id)))
        .unwrap();
    let (status, signalled) = json_response(app.clone(), signal_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(signalled["status"], "signalled");

    let completed_status_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/workflows/{workflow_id}/status"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, completed) = json_response(app.clone(), completed_status_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["workflow"]["status"], "completed");
    assert_eq!(completed["nodes"][1]["status"], "completed");

    let history_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/workflows/{workflow_id}/history"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, history) = json_response(app.clone(), history_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["history"].as_array().expect("history").len(), 2);

    let create_cancel_req = Request::builder()
        .method("POST")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Cancelable Flow","nodes":[{"name":"approval","type":"human_review"}]}"#))
        .unwrap();
    let (status, created_cancel) = json_response(app.clone(), create_cancel_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let cancel_id = created_cancel["workflow"]["id"].as_str().expect("cancel workflow id").to_string();

    let run_cancel_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{cancel_id}/run"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, _) = json_response(app.clone(), run_cancel_req).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let cancel_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{cancel_id}/cancel"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, cancelled) = json_response(app, cancel_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled, serde_json::json!({"ok": true, "status": "cancelled"}));
}

#[tokio::test]
async fn workflows_validate_dag_and_temporal_guard() {
    let app = AppState::test_workflow_internal_token("secret-token", "org-test", "cli-user").router();

    let invalid_req = Request::builder()
        .method("POST")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Broken Flow","nodes":[{"name":"a","type":"agent_task","dependsOn":["b"]},{"name":"b","type":"agent_task","dependsOn":["a"]}]}"#))
        .unwrap();
    let (status, invalid) = json_response(app.clone(), invalid_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid["ok"], false);
    assert_eq!(invalid["error"], "cycle detected in workflow graph");

    let mut state = AppState::test_workflow_internal_token("secret-token", "org-test", "cli-user");
    state.workflow_service = None;
    let app = state.router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/workflows")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Stored Flow","nodes":[{"name":"approval","type":"human_review"}]}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let workflow_id = created["workflow"]["id"].as_str().expect("workflow id");

    let run_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{workflow_id}/run"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, run_req).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "temporal not configured"}));
}
