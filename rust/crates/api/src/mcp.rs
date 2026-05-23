use std::sync::Arc;

use axum::Extension;
use axum::Router;
use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;

use crate::domain::mcp::{
    app_error_message, auth_error_body, create_result_text, initialize_response, initialized_notification_response,
    jsonrpc_error, ok_result_text, parse_optional_uuid, parse_required_uuid, request_id, request_method,
    status_result_text, tool_arguments, tool_name, tool_result, tools_list_response,
};
use crate::services::mcp_agent::CreateSessionRequest;

pub use crate::services::mcp_agent::McpAgentTools;
pub use crate::services::mcp_live_components::build_live_mcp_components;

#[derive(Clone)]
struct McpState {
    internal_token: Arc<String>,
    tools: Arc<dyn McpAgentTools>,
}

pub fn mcp_router<S>(internal_token: impl Into<String>, tools: Arc<dyn McpAgentTools>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = McpState { internal_token: Arc::new(internal_token.into()), tools };
    Router::new().route("/mcp", post(handle_request)).layer(Extension(state))
}

async fn handle_request(
    Extension(state): Extension<McpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(response) = authorize(&headers, state.internal_token.as_str()) {
        return *response;
    }

    Json(handle_jsonrpc(state.tools.as_ref(), body).await).into_response()
}

fn authorize(headers: &HeaderMap, internal_token: &str) -> Result<(), Box<Response>> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("missing authorization header"))).into_response(),
        ));
    };
    let Ok(value) = value.to_str() else {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("invalid authorization token"))).into_response(),
        ));
    };
    if value != format!("Bearer {internal_token}") {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, Json(auth_error_body("invalid authorization token"))).into_response(),
        ));
    }
    Ok(())
}

async fn handle_jsonrpc(tools: &dyn McpAgentTools, request: Value) -> Value {
    let id = request_id(&request);
    let Some(method) = request_method(&request) else {
        return jsonrpc_error(id, -32600, "invalid request");
    };

    match method {
        "initialize" => initialize_response(id, &request, crate::VERSION),
        "tools/list" => tools_list_response(id),
        "tools/call" => call_tool(tools, id, &request).await,
        "notifications/initialized" => initialized_notification_response(id),
        _ => jsonrpc_error(id, -32601, "method not found"),
    }
}

async fn call_tool(tools: &dyn McpAgentTools, id: Value, request: &Value) -> Value {
    let Some(name) = tool_name(request) else {
        return tool_result(id, true, "missing required argument: name".to_string());
    };
    let arguments = tool_arguments(request);

    let result = match name {
        "wisdoverse.agent.create" | "agentforge.agent.create" => handle_create(tools, &arguments).await,
        "wisdoverse.agent.prompt" | "agentforge.agent.prompt" => handle_prompt(tools, &arguments).await,
        "wisdoverse.agent.status" | "agentforge.agent.status" => handle_status(tools, &arguments).await,
        "wisdoverse.agent.destroy" | "agentforge.agent.destroy" => handle_destroy(tools, &arguments).await,
        _ => Err(format!("unknown tool: {name}")),
    };

    match result {
        Ok(text) => tool_result(id, false, text),
        Err(message) => tool_result(id, true, message),
    }
}

async fn handle_create(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let cli_tool = arguments.get("cliTool").and_then(Value::as_str).unwrap_or("claude").to_string();
    let request = CreateSessionRequest {
        project_id: parse_optional_uuid(arguments.get("projectId"))?,
        cli_tool,
        name: arguments.get("name").and_then(Value::as_str).map(str::to_owned),
        org_id: parse_optional_uuid(arguments.get("orgId"))?,
        user_id: parse_optional_uuid(arguments.get("userId"))?,
    };

    let result = tools.create_session(request).await.map_err(app_error_message)?;
    create_result_text(result.agent_id, &result.status, &result.name)
}

async fn handle_prompt(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    let Some(prompt) = arguments.get("prompt").and_then(Value::as_str) else {
        return Err("missing required argument: prompt".to_string());
    };
    tools.send_prompt(agent_id, prompt).await.map_err(app_error_message)?;
    ok_result_text()
}

async fn handle_status(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    let status = tools.session_status(agent_id).await.map_err(app_error_message)?;
    status_result_text(status.agent_id, &status.status)
}

async fn handle_destroy(tools: &dyn McpAgentTools, arguments: &Value) -> Result<String, String> {
    let agent_id = parse_required_uuid(arguments, "agentId")?;
    tools.destroy_session(agent_id).await.map_err(app_error_message)?;
    ok_result_text()
}
