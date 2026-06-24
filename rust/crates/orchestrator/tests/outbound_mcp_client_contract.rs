use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use agentforge_orchestrator::mcp::client::OutboundMcpClient;

#[derive(Clone, Default)]
struct TestServerState {
    seen_tools: Arc<Mutex<Vec<String>>>,
}

async fn spawn_server(app: Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve app");
    });
    (format!("http://{addr}/mcp"), handle)
}

async fn handle_mcp(State(state): State<TestServerState>, Json(request): Json<Value>) -> Json<Value> {
    let tool = request.pointer("/params/name").and_then(Value::as_str).expect("tool name").to_string();
    state.seen_tools.lock().expect("tools lock").push(tool.clone());

    let response = if tool == "wisdoverse.agent.status" {
        json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": r#"{"agentId":"agent-1","status":"completed"}"#
                    }
                ],
                "isError": false
            }
        })
    } else {
        json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "error": {
                "code": -32601,
                "message": format!("unknown tool: {tool}")
            }
        })
    };

    Json(response)
}

#[tokio::test]
async fn session_status_supports_streamable_http_mcp_contract() {
    let state = TestServerState::default();
    let app = Router::new().route("/mcp", post(handle_mcp)).with_state(state.clone());
    let (endpoint, handle) = spawn_server(app).await;

    let client = OutboundMcpClient::new(endpoint, String::new()).expect("client");
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), client.session_status("agent-1"))
        .await
        .expect("client request timeout");

    handle.abort();
    let _ = handle.await;

    assert!(result.is_ok(), "unexpected client result: {result:?}");
    let status = result.expect("session status");
    assert_eq!(status.session_id(), "agent-1");
    assert_eq!(status.status, "completed");
    assert_eq!(state.seen_tools.lock().expect("tools lock").as_slice(), ["wisdoverse.agent.status"]);
}

async fn handle_legacy_mcp(State(state): State<TestServerState>, Json(request): Json<Value>) -> Json<Value> {
    let tool = request.pointer("/params/name").and_then(Value::as_str).expect("tool name").to_string();
    state.seen_tools.lock().expect("tools lock").push(tool.clone());

    let response = match tool.as_str() {
        "wisdoverse.agent.status" => json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "error": {
                "code": -32601,
                "message": "unknown tool: wisdoverse.agent.status"
            }
        }),
        "agentforge.agent.status" => json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {
                "agentId": "agent-1",
                "status": "completed"
            }
        }),
        _ => json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "error": {
                "code": -32601,
                "message": format!("unknown tool: {tool}")
            }
        }),
    };

    Json(response)
}

#[tokio::test]
async fn session_status_falls_back_to_legacy_agentforge_tool_names() {
    let state = TestServerState::default();
    let app = Router::new().route("/mcp", post(handle_legacy_mcp)).with_state(state.clone());
    let (endpoint, handle) = spawn_server(app).await;

    let client = OutboundMcpClient::new(endpoint, String::new()).expect("client");
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), client.session_status("agent-1"))
        .await
        .expect("client request timeout");

    handle.abort();
    let _ = handle.await;

    assert!(result.is_ok(), "unexpected client result: {result:?}");
    let status = result.expect("session status");
    assert_eq!(status.session_id(), "agent-1");
    assert_eq!(status.status, "completed");
    assert_eq!(
        state.seen_tools.lock().expect("tools lock").as_slice(),
        ["wisdoverse.agent.status", "agentforge.agent.status"]
    );
}
