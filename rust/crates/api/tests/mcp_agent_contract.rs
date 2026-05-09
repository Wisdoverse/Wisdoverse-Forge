use std::sync::{Arc, Mutex};

use agentforge_api::mcp::{McpAgentTools, mcp_router};
use agentforge_api::services::mcp_agent::{CreateSessionRequest, CreateSessionResult, SessionStatus};
use agentforge_core::{AppResult, ErrorKind};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone, Default)]
struct TestTools {
    created: Arc<Mutex<Vec<CreateSessionRequest>>>,
    prompted: Arc<Mutex<Vec<(Uuid, String)>>>,
    destroyed: Arc<Mutex<Vec<Uuid>>>,
}

#[async_trait]
impl McpAgentTools for TestTools {
    async fn create_session(&self, request: CreateSessionRequest) -> AppResult<CreateSessionResult> {
        self.created.lock().expect("created").push(request);
        Ok(CreateSessionResult {
            agent_id: Uuid::parse_str("11111111-1111-7111-8111-111111111111").expect("uuid"),
            status: "idle".to_string(),
            name: "Workflow worker".to_string(),
        })
    }

    async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> AppResult<()> {
        self.prompted.lock().expect("prompted").push((agent_id, prompt.to_string()));
        Ok(())
    }

    async fn destroy_session(&self, agent_id: Uuid) -> AppResult<()> {
        self.destroyed.lock().expect("destroyed").push(agent_id);
        Ok(())
    }

    async fn session_status(&self, agent_id: Uuid) -> AppResult<SessionStatus> {
        Ok(SessionStatus { agent_id, status: "working".to_string() })
    }
}

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
async fn mcp_requires_internal_token() {
    let app = mcp_router("secret-token", Arc::new(TestTools::default()));
    let (status, body) =
        json_response(app, mcp_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}), None))
            .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({"error": "missing authorization header"}));
}

#[tokio::test]
async fn mcp_initialize_and_tools_list_expose_agent_surface() {
    let app = mcp_router("secret-token", Arc::new(TestTools::default()));

    let (status, init) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(init["result"]["serverInfo"]["name"], "agentforge-api");

    let (status, tools) = json_response(
        app,
        mcp_request(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}), Some("secret-token")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "wisdoverse.agent.create",
            "wisdoverse.agent.prompt",
            "wisdoverse.agent.status",
            "wisdoverse.agent.destroy",
        ]
    );
}

#[tokio::test]
async fn mcp_tool_calls_round_trip_create_prompt_status_and_destroy() {
    let tools = Arc::new(TestTools::default());
    let app = mcp_router("secret-token", tools.clone());
    let agent_id = "11111111-1111-7111-8111-111111111111";

    let (status, created) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "wisdoverse.agent.create",
                    "arguments": {
                        "projectId": "22222222-2222-7222-8222-222222222222",
                        "cliTool": "codex",
                        "name": "Workflow worker",
                        "orgId": "33333333-3333-7333-8333-333333333333",
                        "userId": "44444444-4444-7444-8444-444444444444"
                    }
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["result"]["isError"], false);

    let create_text = created["result"]["content"][0]["text"].as_str().expect("create text");
    let create_payload: Value = serde_json::from_str(create_text).expect("create payload");
    assert_eq!(create_payload, json!({"agentId": agent_id, "status": "idle", "name": "Workflow worker"}));

    let (status, prompted) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "wisdoverse.agent.prompt",
                    "arguments": {"agentId": agent_id, "prompt": "ship it"}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prompted["result"]["content"][0]["text"], json!({"ok": true}).to_string());

    let (status, status_body) = json_response(
        app.clone(),
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "agentforge.agent.status",
                    "arguments": {"agentId": agent_id}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let status_text = status_body["result"]["content"][0]["text"].as_str().expect("status text");
    let status_payload: Value = serde_json::from_str(status_text).expect("status payload");
    assert_eq!(status_payload, json!({"agentId": agent_id, "status": "working"}));

    let (status, destroyed) = json_response(
        app,
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "agentforge.agent.destroy",
                    "arguments": {"agentId": agent_id}
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(destroyed["result"]["content"][0]["text"], json!({"ok": true}).to_string());

    let created_calls = tools.created.lock().expect("created calls");
    assert_eq!(created_calls.len(), 1);
    assert_eq!(created_calls[0].cli_tool, "codex");
    assert_eq!(created_calls[0].project_id, Some(Uuid::parse_str("22222222-2222-7222-8222-222222222222").unwrap()));
    assert_eq!(created_calls[0].org_id, Some(Uuid::parse_str("33333333-3333-7333-8333-333333333333").unwrap()));
    assert_eq!(created_calls[0].user_id, Some(Uuid::parse_str("44444444-4444-7444-8444-444444444444").unwrap()));
    assert_eq!(
        tools.prompted.lock().expect("prompted calls").as_slice(),
        &[(Uuid::parse_str(agent_id).unwrap(), "ship it".to_string())]
    );
    assert_eq!(tools.destroyed.lock().expect("destroyed calls").as_slice(), &[Uuid::parse_str(agent_id).unwrap()]);
}

#[derive(Clone, Default)]
struct FailingTools;

#[async_trait]
impl McpAgentTools for FailingTools {
    async fn create_session(&self, _request: CreateSessionRequest) -> AppResult<CreateSessionResult> {
        Err(ErrorKind::Validation("bad create".into()).into())
    }

    async fn send_prompt(&self, _agent_id: Uuid, _prompt: &str) -> AppResult<()> {
        Err(ErrorKind::Validation("bad prompt".into()).into())
    }

    async fn destroy_session(&self, _agent_id: Uuid) -> AppResult<()> {
        Err(ErrorKind::Validation("bad destroy".into()).into())
    }

    async fn session_status(&self, _agent_id: Uuid) -> AppResult<SessionStatus> {
        Err(ErrorKind::Validation("bad status".into()).into())
    }
}

#[tokio::test]
async fn mcp_tool_validation_errors_are_returned_as_tool_errors() {
    let app = mcp_router("secret-token", Arc::new(FailingTools));
    let (status, body) = json_response(
        app,
        mcp_request(
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {
                    "name": "wisdoverse.agent.prompt",
                    "arguments": {
                        "agentId": "11111111-1111-7111-8111-111111111111",
                        "prompt": "bad"
                    }
                }
            }),
            Some("secret-token"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true);
    assert!(body["result"]["content"][0]["text"].as_str().unwrap().contains("validation error: bad prompt"));
}
