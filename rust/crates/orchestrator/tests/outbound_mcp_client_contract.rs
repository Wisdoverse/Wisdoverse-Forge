use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tower::ServiceExt;

use agentforge_orchestrator::mcp::client::OutboundMcpClient;
use agentforge_orchestrator::observability::track_request_id;

const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Default)]
struct TestServerState {
    seen_tools: Arc<Mutex<Vec<String>>>,
    /// The `x-request-id` header value the mock MCP bridge saw on each inbound
    /// call (None when the header was absent).
    seen_request_ids: Arc<Mutex<Vec<Option<String>>>>,
}

fn record_request_id(state: &TestServerState, headers: &HeaderMap) {
    let id = headers.get(REQUEST_ID_HEADER).and_then(|v| v.to_str().ok()).map(ToString::to_string);
    state.seen_request_ids.lock().expect("request id lock").push(id);
}

async fn spawn_server(app: Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve app");
    });
    (format!("http://{addr}/mcp"), handle)
}

async fn handle_mcp(
    State(state): State<TestServerState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Json<Value> {
    record_request_id(&state, &headers);
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
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(2), client.session_status("org-1", "ws-1", "agent-1"))
            .await
            .expect("client request timeout");

    handle.abort();
    let _ = handle.await;

    assert!(result.is_ok(), "unexpected client result: {result:?}");
    let status = result.expect("session status");
    assert_eq!(status.session_id(), "agent-1");
    assert_eq!(status.status, "completed");
    assert_eq!(state.seen_tools.lock().expect("tools lock").as_slice(), ["wisdoverse.agent.status"]);
    // Called directly (no request-id middleware/scope) → graceful no-forward.
    assert_eq!(
        state.seen_request_ids.lock().expect("request id lock").as_slice(),
        [None],
        "a direct client call outside request context must not forward x-request-id"
    );
}

#[derive(Clone)]
struct ProxyState {
    endpoint: String,
}

/// A handler that calls the outbound MCP client, standing in for the
/// orchestrator's task handler that calls back into the API's MCP bridge.
async fn call_status_via_client(State(proxy): State<ProxyState>) -> String {
    let client = OutboundMcpClient::new(proxy.endpoint.clone(), String::new()).expect("client");
    match client.session_status("org-1", "ws-1", "agent-1").await {
        Ok(status) => status.status,
        Err(err) => format!("error: {err}"),
    }
}

/// End-to-end cross-hop propagation: when the outbound MCP call is made from
/// inside the request-id middleware (as the real task handler is), the inbound
/// `x-request-id` must ride the call through to the bridge — so the two
/// services' logs share one correlation id.
#[tokio::test]
async fn outbound_call_forwards_request_id_from_handler_scope() {
    let server_state = TestServerState::default();
    let server = Router::new().route("/mcp", post(handle_mcp)).with_state(server_state.clone());
    let (endpoint, handle) = spawn_server(server).await;

    let proxy = Router::new()
        .route("/call", get(call_status_via_client))
        .with_state(ProxyState { endpoint })
        .layer(axum::middleware::from_fn(track_request_id));
    let response = proxy
        .oneshot(Request::builder().uri("/call").header(REQUEST_ID_HEADER, "corr-fwd-1").body(Body::empty()).unwrap())
        .await
        .expect("proxy request");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    handle.abort();
    let _ = handle.await;

    assert_eq!(
        server_state.seen_request_ids.lock().expect("request id lock").as_slice(),
        [Some("corr-fwd-1".to_string())],
        "the outbound MCP call must forward the handler's request id to the bridge"
    );
}

async fn handle_legacy_mcp(
    State(state): State<TestServerState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Json<Value> {
    record_request_id(&state, &headers);
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
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(2), client.session_status("org-1", "ws-1", "agent-1"))
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
