use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 128 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

fn mcp_request(body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri("/mcp").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn mcp_route_stays_unmounted_without_explicit_enablement() {
    let app = AppState::test_ready().router();
    let response = app
        .oneshot(mcp_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), None))
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mcp_requires_auth_when_enabled() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();
    let (status, body) =
        json_response(app, mcp_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), None))
            .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({"error": "missing authorization header"}));
}

#[tokio::test]
async fn mcp_initialize_and_tools_list_match_go_tool_surface() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();

    let (status, initialize) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "contract-test", "version": "1.0.0"}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initialize["jsonrpc"], "2.0");
    assert_eq!(initialize["id"], 1);
    assert_eq!(initialize["result"]["serverInfo"]["name"], "agentforge-orchestrator");
    assert_eq!(initialize["result"]["serverInfo"]["version"], "1.0.0");
    assert!(initialize["result"]["capabilities"]["tools"].is_object());

    let (status, tools_list) = json_response(
        app,
        mcp_request(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}), Some("secret-token")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tools_list["jsonrpc"], "2.0");
    assert_eq!(tools_list["id"], 2);
    let tool_names: Vec<&str> = tools_list["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(
        tool_names,
        vec![
            "orchestrator.task.list",
            "orchestrator.task.create",
            "orchestrator.task.get",
            "orchestrator.review.list",
            "orchestrator.review.approve",
            "orchestrator.review.reject",
            "orchestrator.review.comment",
        ]
    );
}

#[tokio::test]
async fn mcp_task_tools_support_create_list_and_get_round_trip() {
    let app = AppState::test_mcp_internal_token("secret-token", "org-test").router();

    let (status, created) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.create",
                    "arguments": {
                        "title": "Build feature X",
                        "description": "Implement the new feature",
                        "priority": "high"
                    }
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["result"]["isError"], false);
    let created_text = created["result"]["content"][0]["text"].as_str().expect("create text");
    let created_task: Value = serde_json::from_str(created_text).expect("created task json");
    let task_id = created_task["id"].as_str().expect("task id").to_string();
    assert_eq!(created_task["title"], "Build feature X");
    assert_eq!(created_task["priority"], "high");
    assert_eq!(created_task["state"], "pending");
    assert_eq!(created_task["createdBy"], "mcp");
    assert_eq!(created_task["orgId"], "org-test");

    let (status, listed) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.list",
                    "arguments": {"state": "pending"}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["result"]["isError"], false);
    let listed_text = listed["result"]["content"][0]["text"].as_str().expect("list text");
    let listed_tasks: Value = serde_json::from_str(listed_text).expect("listed tasks json");
    let tasks = listed_tasks.as_array().expect("tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], task_id);

    let (status, got) = json_response(
        app,
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "orchestrator.task.get",
                    "arguments": {"id": task_id}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["result"]["isError"], false);
    let got_text = got["result"]["content"][0]["text"].as_str().expect("get text");
    let got_task: Value = serde_json::from_str(got_text).expect("got task json");
    assert_eq!(got_task["title"], "Build feature X");
    assert_eq!(got_task["id"], task_id);
}
