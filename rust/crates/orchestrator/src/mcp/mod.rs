pub mod client;

use std::collections::HashMap;
use std::sync::Mutex;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use crate::audit::{AuditAction, AuditLog};
use crate::auth::{self, AuthContext, RequestIdentity};
use crate::review::{ReviewComment, ReviewError, ReviewFilter, ReviewState, VerdictError, apply_review_verdict};
use crate::state::AppState;
use crate::task::TaskState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub priority: String,
    pub assigned_to: Option<String>,
    pub created_by: String,
    pub org_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ponytail: task tools (task.list/create/get) remain an in-memory stub; their
// `Mutex<Store>` HashMap never touches the real DB. Productionizing them
// (store-backed, identity-aware) is a separate follow-up (#841 part-3 deferred
// the task tools; only the review tool group is wired to the real store here).
#[derive(Default)]
struct Store {
    task_seq: u64,
    tasks: HashMap<String, Task>,
}

pub struct McpServer {
    /// The configured fallback org for the in-memory task stub only. Review tools
    /// derive org from the per-request authenticated identity, never from this.
    org_id: String,
    store: Mutex<Store>,
}

impl McpServer {
    pub fn new(org_id: String) -> Self {
        Self { org_id, store: Mutex::new(Store::default()) }
    }

    /// Dispatch a single JSON-RPC request.
    ///
    /// `state` + `headers` are threaded so the review tool group can resolve the
    /// caller's authenticated org + actor and act on the real `review_store`. The
    /// task tools ignore them and operate on the in-memory stub.
    pub async fn handle_jsonrpc(&self, state: &AppState, headers: &HeaderMap, request: Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32600, "invalid request");
        };

        match method {
            "initialize" => self.initialize(id, &request),
            "tools/list" => jsonrpc_result(id, json!({ "tools": self.tools() })),
            "tools/call" => self.call_tool(state, headers, id, &request).await,
            "notifications/initialized" => jsonrpc_result(id, json!({})),
            _ => jsonrpc_error(id, -32601, "method not found"),
        }
    }

    fn initialize(&self, id: Value, request: &Value) -> Value {
        let protocol_version =
            request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("2024-11-05");
        jsonrpc_result(
            id,
            json!({
                "protocolVersion": protocol_version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "agentforge-orchestrator", "version": "1.0.0" }
            }),
        )
    }

    fn tools(&self) -> Vec<Value> {
        vec![
            tool(
                "orchestrator.task.list",
                "List tasks in the orchestrator. Optionally filter by state and/or assignee.",
                json!({
                    "type": "object",
                    "properties": {
                        "state": {"type": "string", "enum": ["pending", "assigned", "working", "review", "completed", "failed", "changes_requested"]},
                        "assignedTo": {"type": "string"}
                    }
                }),
            ),
            tool(
                "orchestrator.task.create",
                "Create a new task in the orchestrator.",
                json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "description": {"type": "string"},
                        "priority": {"type": "string", "enum": ["low", "normal", "high", "urgent"]}
                    },
                    "required": ["title"]
                }),
            ),
            tool(
                "orchestrator.task.get",
                "Get a task by ID.",
                json!({
                    "type": "object",
                    "properties": { "id": {"type": "string"} },
                    "required": ["id"]
                }),
            ),
            tool(
                "orchestrator.review.list",
                "List code reviews. Optionally filter by state and/or task ID.",
                json!({
                    "type": "object",
                    "properties": {
                        "state": {"type": "string", "enum": ["pending", "in_review", "approved", "changes_requested", "rejected"]},
                        "taskId": {"type": "string"}
                    }
                }),
            ),
            tool(
                "orchestrator.review.approve",
                "Approve a code review.",
                json!({
                    "type": "object",
                    "properties": { "id": {"type": "string"} },
                    "required": ["id"]
                }),
            ),
            tool(
                "orchestrator.review.reject",
                "Reject a code review with feedback.",
                json!({
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "feedback": {"type": "string"}
                    },
                    "required": ["id", "feedback"]
                }),
            ),
            tool(
                "orchestrator.review.comment",
                "Add a comment to a code review, optionally pinned to a file and line.",
                json!({
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "body": {"type": "string"},
                        "filePath": {"type": "string"},
                        "line": {"type": "number"}
                    },
                    "required": ["id", "body"]
                }),
            ),
        ]
    }

    async fn call_tool(&self, state: &AppState, headers: &HeaderMap, id: Value, request: &Value) -> Value {
        let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
            return tool_result(id, true, "missing required argument: name".to_string());
        };
        let arguments = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));

        let outcome = match name {
            // ponytail: task tools stay an in-memory stub (see `Store`); they ignore
            // the per-request identity and use the configured fallback org.
            "orchestrator.task.list" => self.handle_task_list(&arguments),
            "orchestrator.task.create" => self.handle_task_create(&arguments),
            "orchestrator.task.get" => self.handle_task_get(&arguments),
            // Review tools are wired to the real store + audit via the shared verdict path.
            "orchestrator.review.list" => handle_review_list(state, headers, &arguments).await,
            "orchestrator.review.approve" => handle_review_approve(state, headers, &arguments).await,
            "orchestrator.review.reject" => handle_review_reject(state, headers, &arguments).await,
            "orchestrator.review.comment" => handle_review_comment(state, headers, &arguments).await,
            _ => Err(format!("unknown tool: {name}")),
        };

        match outcome {
            Ok(text) => tool_result(id, false, text),
            Err(message) => tool_result(id, true, message),
        }
    }

    fn handle_task_create(&self, arguments: &Value) -> Result<String, String> {
        let Some(title) = arguments.get("title").and_then(Value::as_str) else {
            return Err("missing required argument: title".to_string());
        };

        let now = Utc::now();
        let mut store = self.store.lock().expect("mcp store lock poisoned");
        store.task_seq += 1;
        let task = Task {
            id: format!("task-{}", store.task_seq),
            title: title.to_string(),
            description: arguments.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
            state: "pending".to_string(),
            priority: arguments.get("priority").and_then(Value::as_str).unwrap_or("normal").to_string(),
            assigned_to: arguments.get("assignedTo").and_then(Value::as_str).map(ToString::to_string),
            created_by: "mcp".to_string(),
            org_id: self.org_id.clone(),
            created_at: now,
            updated_at: now,
        };
        store.tasks.insert(task.id.clone(), task.clone());
        serialize_json(&task)
    }

    fn handle_task_list(&self, arguments: &Value) -> Result<String, String> {
        let state_filter = arguments.get("state").and_then(Value::as_str);
        let assignee_filter = arguments.get("assignedTo").and_then(Value::as_str);
        let store = self.store.lock().expect("mcp store lock poisoned");
        let mut tasks: Vec<Task> = store
            .tasks
            .values()
            .filter(|task| state_filter.is_none_or(|state| task.state == state))
            .filter(|task| assignee_filter.is_none_or(|assignee| task.assigned_to.as_deref() == Some(assignee)))
            .cloned()
            .collect();
        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        serialize_json(&tasks)
    }

    fn handle_task_get(&self, arguments: &Value) -> Result<String, String> {
        let Some(task_id) = arguments.get("id").and_then(Value::as_str) else {
            return Err("missing required argument: id".to_string());
        };

        let store = self.store.lock().expect("mcp store lock poisoned");
        let Some(task) = store.tasks.get(task_id) else {
            return Err(format!("task {task_id} not found"));
        };
        serialize_json(task)
    }
}

/// Resolve the caller's authenticated org + actor and a CHECK-valid `actor_type`
/// for the audit log.
///
/// `actor_type` is honest, derived from the auth kind: a session JWT acts as a
/// `"human"`; the internal service token acts as the `"system"` identity. Both
/// satisfy the `audit_logs.actor_type IN ('human','agent','system')` CHECK.
/// Returns a tool-error string (not an HTTP `Response`) so review failures surface
/// as MCP `isError` tool results, not raw HTTP errors.
///
/// The failure messages are distinguished so the caller (and the LLM agent reading
/// the tool error) gets something actionable rather than one opaque string:
/// - genuine auth failure (auth disabled, or an invalid/missing token) →
///   "authentication required for review actions";
/// - the documented internal-token-without-`X-Org-ID` case →
///   "missing organization context for review actions (set X-Org-ID)";
/// - a provisioner/DB fault while resolving the participant → a generic infra
///   message (raw detail logged server-side, never leaked).
async fn resolve_actor(state: &AppState, headers: &HeaderMap) -> Result<(RequestIdentity, &'static str), String> {
    let actor_type = match auth::require_api_auth(state, headers) {
        Ok(AuthContext::Session(_)) => "human",
        Ok(AuthContext::InternalToken) => "system",
        // Anonymous means auth is disabled entirely; there is no honest actor to
        // attribute a review verdict to, so refuse rather than forge one.
        Ok(AuthContext::Anonymous) => return Err("authentication required for review actions".to_string()),
        Err(_) => return Err("authentication required for review actions".to_string()),
    };

    // `require_request_identity` returns an opaque HTTP `Response` that collapses
    // (a) the documented missing-org-context case and (b) a provisioner/DB fault
    // into one error. Recover which one it was so the caller gets an actionable
    // message: probe the org-context precondition first (cheap, no I/O), then treat
    // any remaining failure as an internal provisioner/infra fault.
    match auth::require_request_identity(state, headers).await {
        Ok(identity) => Ok((identity, actor_type)),
        Err(_) => {
            if auth::require_org_context(state, headers).is_err() {
                // Org context is the documented expected failure for an internal
                // token without X-Org-ID -- surface it as such, not as an auth error.
                Err("missing organization context for review actions (set X-Org-ID)".to_string())
            } else {
                // Org context resolved, so the identity failure was a provisioner /
                // DB fault. This is an infra fault, not a caller mistake: log it and
                // return a generic message (no raw internal detail to the client).
                tracing::warn!("failed to resolve review actor identity (provisioner/internal failure)");
                Err("failed to resolve review actor identity".to_string())
            }
        }
    }
}

#[allow(clippy::result_large_err)]
fn require_review_store(state: &AppState) -> Result<&std::sync::Arc<dyn crate::review::Store>, String> {
    state.review_store.as_ref().ok_or_else(|| "review store not configured".to_string())
}

/// Map a [`VerdictError`] to an MCP tool-error string.
///
/// This text becomes the `isError` tool result, which an LLM agent reads into its
/// context, so it must not leak raw internal/DB error strings (CLAUDE.md: "Do not
/// leak internal errors to clients"). The specific, safe variants (not found,
/// illegal transition, self-approval, caller-supplied invalid input) are surfaced;
/// internal/store and audit failures return a generic string and the raw detail is
/// logged server-side.
fn map_verdict_error(err: VerdictError) -> String {
    match err {
        VerdictError::NotFound => "review not found".to_string(),
        VerdictError::IllegalTransition => "review cannot transition from its current state".to_string(),
        VerdictError::SelfApproval => "cannot approve your own review".to_string(),
        // NotFound is already specific; InvalidInput is caller-supplied (e.g. an
        // unsupported verdict state) and safe to surface. Internal wraps raw
        // SQLx/store text -- log it server-side and return a generic message.
        VerdictError::Review(ReviewError::NotFound) => "review not found".to_string(),
        VerdictError::Review(ReviewError::InvalidInput(message)) => message,
        VerdictError::Review(ReviewError::Internal(message)) => {
            tracing::warn!(error = %message, "internal review error in MCP verdict");
            "internal review error".to_string()
        }
        // Fail-closed audit: detail is already logged in `record_verdict_audit`.
        VerdictError::Audit(_) => "failed to record review audit".to_string(),
    }
}

/// Map a bare [`ReviewError`] (non-verdict store paths: list, add_comment) to an
/// MCP tool-error string with the same leak-safety rules as [`map_verdict_error`].
fn map_store_error(err: ReviewError) -> String {
    match err {
        ReviewError::NotFound => "review not found".to_string(),
        ReviewError::InvalidInput(message) => message,
        ReviewError::Internal(message) => {
            tracing::warn!(error = %message, "internal review error in MCP store call");
            "internal review error".to_string()
        }
    }
}

async fn handle_review_list(state: &AppState, headers: &HeaderMap, arguments: &Value) -> Result<String, String> {
    let (identity, _actor_type) = resolve_actor(state, headers).await?;
    let store = require_review_store(state)?;

    let state_filter = match arguments.get("state").and_then(Value::as_str) {
        Some(raw) => Some(raw.parse::<ReviewState>().map_err(|err| err.to_string())?),
        None => None,
    };
    let task_filter = arguments.get("taskId").and_then(Value::as_str).map(ToString::to_string);

    let reviews = store
        .list(ReviewFilter { org_id: identity.org_id, task_id: task_filter, state: state_filter, limit: 50, offset: 0 })
        .await
        .map_err(map_store_error)?;
    serialize_json(&reviews)
}

async fn handle_review_approve(state: &AppState, headers: &HeaderMap, arguments: &Value) -> Result<String, String> {
    let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
        return Err("missing required argument: id".to_string());
    };
    let (identity, actor_type) = resolve_actor(state, headers).await?;
    let store = require_review_store(state)?;

    apply_review_verdict(
        store.as_ref(),
        state.audit_store.as_deref(),
        &identity.org_id,
        &identity.user_id,
        actor_type,
        review_id,
        ReviewState::Approved,
        TaskState::Completed,
        None,
    )
    .await
    .map(|_| format!("review {review_id} → approved"))
    .map_err(map_verdict_error)
}

async fn handle_review_reject(state: &AppState, headers: &HeaderMap, arguments: &Value) -> Result<String, String> {
    let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
        return Err("missing required argument: id".to_string());
    };
    // Require non-empty feedback, mirroring the HTTP reject contract.
    let Some(feedback) = arguments.get("feedback").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
    else {
        return Err("missing required argument: feedback".to_string());
    };
    let (identity, actor_type) = resolve_actor(state, headers).await?;
    let store = require_review_store(state)?;

    apply_review_verdict(
        store.as_ref(),
        state.audit_store.as_deref(),
        &identity.org_id,
        &identity.user_id,
        actor_type,
        review_id,
        ReviewState::ChangesRequested,
        TaskState::ChangesRequested,
        Some(feedback),
    )
    .await
    .map(|_| format!("review {review_id} → changes_requested"))
    .map_err(map_verdict_error)
}

async fn handle_review_comment(state: &AppState, headers: &HeaderMap, arguments: &Value) -> Result<String, String> {
    let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
        return Err("missing required argument: id".to_string());
    };
    let Some(body) = arguments.get("body").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else {
        return Err("missing required argument: body".to_string());
    };
    let (identity, actor_type) = resolve_actor(state, headers).await?;
    let store = require_review_store(state)?;

    // `review_comments.line` is `i32`; clamp/convert from the JSON number.
    let line = arguments
        .get("line")
        .and_then(Value::as_i64)
        .or_else(|| arguments.get("line").and_then(Value::as_f64).map(|line| line as i64))
        .and_then(|line| i32::try_from(line).ok());

    let mut comment = ReviewComment {
        id: String::new(),
        review_id: String::new(),
        author_id: identity.user_id.clone(),
        body: body.to_string(),
        file_path: arguments.get("filePath").and_then(Value::as_str).map(ToString::to_string),
        line,
        created_at: chrono::Utc::now(),
    };

    store.add_comment(review_id, &identity.org_id, &mut comment).await.map_err(map_store_error)?;

    // Best-effort audit, mirroring the HTTP `add_comment` path (`let _ = record_audit(...)`).
    // A comment is not a verdict, so an audit failure must not fail the tool; but it
    // must not be silently dropped either -- log it. The actor identity matches the
    // resolved review actor (honest `actor_type`).
    record_review_comment_audit(state, &identity, actor_type, review_id).await;

    serialize_json(&comment)
}

/// Write the best-effort `review.comment` audit row for an MCP comment.
///
/// Mirrors the HTTP `add_comment` path's `let _ = record_audit(... ReviewComment ...)`
/// semantics: when no audit store is configured this is a no-op, and a write failure
/// is logged (not silently dropped) but never fails the comment tool.
async fn record_review_comment_audit(state: &AppState, identity: &RequestIdentity, actor_type: &str, review_id: &str) {
    let Some(audit_store) = state.audit_store.as_ref() else {
        return;
    };
    let mut log = AuditLog {
        id: String::new(),
        action: AuditAction::ReviewComment,
        actor_id: identity.user_id.clone(),
        actor_type: actor_type.to_string(),
        resource: "review".to_string(),
        resource_id: Some(review_id.to_string()),
        org_id: identity.org_id.clone(),
        changes: None,
        ip_address: None,
        user_agent: None,
        created_at: chrono::Utc::now(),
    };
    if let Err(err) = audit_store.create(&mut log).await {
        tracing::warn!(review_id = %review_id, error = %err, "MCP review.comment audit write failed");
    }
}

pub async fn handle_request(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    // Gate the endpoint on token validity only (matching the historical contract:
    // an internal token without X-Org-ID can still drive the task-tool stub). The
    // review tool group re-resolves the full per-request identity + actor_type
    // inside its handlers, so a missing org surfaces as an `isError` tool result
    // for those tools rather than failing the whole request.
    if let Err(response) = auth::require_api_auth(&state, &headers) {
        return response;
    }

    let Some(server) = state.mcp_server.as_ref() else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    };
    let server = server.clone();
    Json(server.handle_jsonrpc(&state, &headers, body).await).into_response()
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn jsonrpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_result(id: Value, is_error: bool, text: String) -> Value {
    jsonrpc_result(
        id,
        json!({
            "content": [{"type": "text", "text": text}],
            "isError": is_error,
        }),
    )
}

fn serialize_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| format!("failed to marshal result: {err}"))
}
