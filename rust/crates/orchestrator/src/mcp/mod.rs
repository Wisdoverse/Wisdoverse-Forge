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

use crate::auth;
use crate::state::AppState;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReview {
    pub id: String,
    pub task_id: String,
    pub session_id: String,
    pub diff_ref: String,
    pub state: String,
    pub assigned_to: Option<String>,
    pub org_id: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    pub id: String,
    pub review_id: String,
    pub author_id: String,
    pub body: String,
    pub file_path: Option<String>,
    pub line: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Default)]
struct Store {
    task_seq: u64,
    comment_seq: u64,
    tasks: HashMap<String, Task>,
    reviews: HashMap<String, CodeReview>,
    comments: HashMap<String, Vec<ReviewComment>>,
}

pub struct McpServer {
    org_id: String,
    store: Mutex<Store>,
}

impl McpServer {
    pub fn new(org_id: String) -> Self {
        Self { org_id, store: Mutex::new(Store::default()) }
    }

    pub fn handle_jsonrpc(&self, request: Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32600, "invalid request");
        };

        match method {
            "initialize" => self.initialize(id, &request),
            "tools/list" => jsonrpc_result(id, json!({ "tools": self.tools() })),
            "tools/call" => self.call_tool(id, &request),
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

    fn call_tool(&self, id: Value, request: &Value) -> Value {
        let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
            return tool_result(id, true, "missing required argument: name".to_string());
        };
        let arguments = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));

        let outcome = match name {
            "orchestrator.task.list" => self.handle_task_list(&arguments),
            "orchestrator.task.create" => self.handle_task_create(&arguments),
            "orchestrator.task.get" => self.handle_task_get(&arguments),
            "orchestrator.review.list" => self.handle_review_list(&arguments),
            "orchestrator.review.approve" => self.handle_review_approve(&arguments),
            "orchestrator.review.reject" => self.handle_review_reject(&arguments),
            "orchestrator.review.comment" => self.handle_review_comment(&arguments),
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

    fn handle_review_list(&self, arguments: &Value) -> Result<String, String> {
        let state_filter = arguments.get("state").and_then(Value::as_str);
        let task_filter = arguments.get("taskId").and_then(Value::as_str);
        let store = self.store.lock().expect("mcp store lock poisoned");
        let mut reviews: Vec<CodeReview> = store
            .reviews
            .values()
            .filter(|review| state_filter.is_none_or(|state| review.state == state))
            .filter(|review| task_filter.is_none_or(|task_id| review.task_id == task_id))
            .cloned()
            .collect();
        reviews.sort_by(|left, right| left.id.cmp(&right.id));
        serialize_json(&reviews)
    }

    fn handle_review_approve(&self, arguments: &Value) -> Result<String, String> {
        let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
            return Err("missing required argument: id".to_string());
        };

        let mut store = self.store.lock().expect("mcp store lock poisoned");
        let Some(review) = store.reviews.get_mut(review_id) else {
            return Err(format!("review {review_id} not found"));
        };
        review.state = "approved".to_string();
        review.updated_at = Utc::now();
        Ok(format!("review {review_id} approved"))
    }

    fn handle_review_reject(&self, arguments: &Value) -> Result<String, String> {
        let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
            return Err("missing required argument: id".to_string());
        };
        let Some(feedback) = arguments.get("feedback").and_then(Value::as_str) else {
            return Err("missing required argument: feedback".to_string());
        };

        let mut store = self.store.lock().expect("mcp store lock poisoned");
        let Some(review) = store.reviews.get_mut(review_id) else {
            return Err(format!("review {review_id} not found"));
        };
        review.state = "rejected".to_string();
        review.updated_at = Utc::now();
        store.comment_seq += 1;
        let comment = ReviewComment {
            id: format!("comment-{}", store.comment_seq),
            review_id: review_id.to_string(),
            author_id: "mcp".to_string(),
            body: format!("[Rejection] {feedback}"),
            file_path: None,
            line: None,
            created_at: Utc::now(),
        };
        store.comments.entry(review_id.to_string()).or_default().push(comment);
        Ok(format!("review {review_id} rejected"))
    }

    fn handle_review_comment(&self, arguments: &Value) -> Result<String, String> {
        let Some(review_id) = arguments.get("id").and_then(Value::as_str) else {
            return Err("missing required argument: id".to_string());
        };
        let Some(body) = arguments.get("body").and_then(Value::as_str) else {
            return Err("missing required argument: body".to_string());
        };

        let mut store = self.store.lock().expect("mcp store lock poisoned");
        if !store.reviews.contains_key(review_id) {
            return Err(format!("review {review_id} not found"));
        }
        store.comment_seq += 1;
        let comment = ReviewComment {
            id: format!("comment-{}", store.comment_seq),
            review_id: review_id.to_string(),
            author_id: "mcp".to_string(),
            body: body.to_string(),
            file_path: arguments.get("filePath").and_then(Value::as_str).map(ToString::to_string),
            line: arguments
                .get("line")
                .and_then(Value::as_i64)
                .or_else(|| arguments.get("line").and_then(Value::as_f64).map(|line| line as i64)),
            created_at: Utc::now(),
        };
        store.comments.entry(review_id.to_string()).or_default().push(comment.clone());
        serialize_json(&comment)
    }
}

pub async fn handle_request(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if let Err(response) = auth::require_api_auth(&state, &headers) {
        return response;
    }

    let Some(server) = state.mcp_server.as_ref() else {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    };
    Json(server.handle_jsonrpc(body)).into_response()
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
