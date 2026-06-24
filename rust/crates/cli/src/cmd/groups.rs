use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::interactive::confirm::confirm_or_force;
use crate::output::{self, Column, Pagination};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

pub const GROUP_COLUMNS: &[Column] = &[
    Column { header: "ID", field: "id" },
    Column { header: "NAME", field: "name" },
    Column { header: "DESCRIPTION", field: "description" },
    Column { header: "TEAM", field: "teamId" },
    Column { header: "PROJECT", field: "projectId" },
];

pub const WORKER_COLUMNS: &[Column] =
    &[Column { header: "AGENT", field: "agentId" }, Column { header: "NAME", field: "name" }];

#[derive(Debug, clap::Args)]
#[command(
    about = "Manage agent groups",
    long_about = "Create, inspect, and manage Wisdoverse Forge agent groups.",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct GroupsArgs {
    #[command(subcommand)]
    pub command: GroupsSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum GroupsSubcommand {
    /// List groups
    List(ListArgs),
    /// Create a group
    Create(CreateArgs),
    /// Get a group by ID
    Get(GetArgs),
    /// Update a group
    Update(UpdateArgs),
    /// Delete a group
    Delete(DeleteArgs),
    /// Manage group workers
    Workers(WorkersArgs),
    /// Dispatch a message to a group
    Dispatch(DispatchArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Filter by team ID
    #[arg(long)]
    pub team: Option<String>,
    /// Filter by project ID
    #[arg(long)]
    pub project: Option<String>,
    /// Maximum number of groups to return
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

#[derive(Debug, clap::Args)]
pub struct CreateArgs {
    /// Group name (required)
    #[arg(long)]
    pub name: String,
    /// Team ID (required)
    #[arg(long)]
    pub team: String,
    /// Project ID (optional)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct GetArgs {
    /// Group ID
    pub id: String,
}

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    /// Group ID
    pub id: String,
    /// New group name
    #[arg(long)]
    pub name: Option<String>,
    /// New group description
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct DeleteArgs {
    /// Group ID
    pub id: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct WorkersArgs {
    #[command(subcommand)]
    pub command: WorkersSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum WorkersSubcommand {
    /// List workers in a group
    List(WorkersListArgs),
    /// Add a worker to a group
    Add(WorkersAddArgs),
    /// Remove a worker from a group
    Remove(WorkersRemoveArgs),
}

#[derive(Debug, clap::Args)]
pub struct WorkersListArgs {
    /// Group ID
    pub group_id: String,
}

#[derive(Debug, clap::Args)]
pub struct WorkersAddArgs {
    /// Group ID
    pub group_id: String,
    /// Agent ID to add as worker (required)
    #[arg(long)]
    pub agent: String,
    /// Worker name (required)
    #[arg(long)]
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct WorkersRemoveArgs {
    /// Group ID
    pub group_id: String,
    /// Agent ID to remove (required)
    #[arg(long)]
    pub agent: String,
}

#[derive(Debug, clap::Args)]
pub struct DispatchArgs {
    /// Group ID
    pub group_id: String,
    /// Message to dispatch (required)
    #[arg(long)]
    pub message: String,
    /// Optional task description
    #[arg(long)]
    pub task: Option<String>,
}

pub async fn dispatch(
    args: GroupsArgs,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    match args.command {
        GroupsSubcommand::List(a) => list(a, ctx, stdout).await,
        GroupsSubcommand::Create(a) => create(a, ctx, stdout).await,
        GroupsSubcommand::Get(a) => get(a, ctx, stdout).await,
        GroupsSubcommand::Update(a) => update(a, ctx, stdout).await,
        GroupsSubcommand::Delete(a) => delete(a, ctx, stdin, stdout, stderr).await,
        GroupsSubcommand::Workers(a) => workers_dispatch(a, ctx, stdout).await,
        GroupsSubcommand::Dispatch(a) => dispatch_message(a, ctx, stdout).await,
    }
}

async fn workers_dispatch(args: WorkersArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    match args.command {
        WorkersSubcommand::List(a) => workers_list(a, ctx, stdout).await,
        WorkersSubcommand::Add(a) => workers_add(a, ctx, stdout).await,
        WorkersSubcommand::Remove(a) => workers_remove(a, ctx, stdout).await,
    }
}

async fn list(args: ListArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(t) = &args.team {
        q.append_pair("teamId", t);
    }
    if let Some(p) = &args.project {
        q.append_pair("projectId", p);
    }
    q.append_pair("limit", &args.limit.to_string());
    let path = format!("/api/v1/groups?{}", q.finish());

    let (items, total, limit, offset) = ctx.client.do_request_list(reqwest::Method::GET, &path, None).await?;

    let pag = Pagination {
        total: total as usize,
        limit: if limit > 0 { limit as usize } else { args.limit as usize },
        offset: offset as usize,
    };
    let data = Value::Array(items);
    output::format_with_jq(stdout, &ctx.format, GROUP_COLUMNS, &data, Some(&pag), &ctx.jq)
        .map_err(|e| CliError::Other(e.to_string()))
}

async fn create(args: CreateArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut body = serde_json::Map::new();
    body.insert("name".into(), Value::String(args.name));
    body.insert("teamId".into(), Value::String(args.team));
    if let Some(p) = args.project {
        body.insert("projectId".into(), Value::String(p));
    }
    let body = Value::Object(body);

    let result = ctx
        .client
        .do_request(reqwest::Method::POST, "/api/v1/groups", Some(&body), ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    output::format(stdout, &ctx.format, GROUP_COLUMNS, &result, None).map_err(|e| CliError::Other(e.to_string()))
}

async fn get(args: GetArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let result = ctx
        .client
        .do_request(reqwest::Method::GET, &format!("/api/v1/groups/{}", args.id), None, ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    output::format_with_jq(stdout, &ctx.format, GROUP_COLUMNS, &result, None, &ctx.jq)
        .map_err(|e| CliError::Other(e.to_string()))
}

async fn update(args: UpdateArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut body = serde_json::Map::new();
    if let Some(n) = args.name {
        body.insert("name".into(), Value::String(n));
    }
    if let Some(d) = args.description {
        body.insert("description".into(), Value::String(d));
    }
    if body.is_empty() {
        return Err(CliError::Other("at least one of --name or --description must be provided".into()));
    }
    let body = Value::Object(body);

    let result = ctx
        .client
        .do_request(reqwest::Method::PATCH, &format!("/api/v1/groups/{}", args.id), Some(&body), ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    output::format(stdout, &ctx.format, GROUP_COLUMNS, &result, None).map_err(|e| CliError::Other(e.to_string()))
}

async fn delete(
    args: DeleteArgs,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    let confirmed = confirm_or_force(args.force, &format!("Delete group {}?", args.id), stderr, stdin)?;
    if !confirmed {
        writeln!(stderr, "Aborted.").ok();
        return Ok(());
    }

    ctx.client
        .do_request(reqwest::Method::DELETE, &format!("/api/v1/groups/{}", args.id), None, ResponseKind::Auto)
        .await?;

    output::format_action(
        stdout,
        &ctx.format,
        &format!("Group {} deleted.", args.id),
        &json!({ "id": args.id, "deleted": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))
}

async fn workers_list(args: WorkersListArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = format!("/api/v1/groups/{}/workers", args.group_id);

    let (items, total, limit, _offset) = ctx.client.do_request_list(reqwest::Method::GET, &path, None).await?;

    let n = items.len();
    let pag = Pagination {
        total: if total > 0 { total as usize } else { n },
        limit: if limit > 0 { limit as usize } else { n },
        offset: 0,
    };
    let data = Value::Array(items);
    output::format(stdout, &ctx.format, WORKER_COLUMNS, &data, Some(&pag)).map_err(|e| CliError::Other(e.to_string()))
}

async fn workers_add(args: WorkersAddArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let body = json!({
        "agentId": args.agent,
        "name": args.name,
    });
    let path = format!("/api/v1/groups/{}/workers", args.group_id);

    let result = ctx
        .client
        .do_request(reqwest::Method::POST, &path, Some(&body), ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    // Response shape: `{workers: [...]}` — find the newly added worker by agentId.
    if let Some(workers) = result.get("workers").and_then(|v| v.as_array()) {
        for item in workers {
            if item.get("agentId").and_then(|v| v.as_str()) == Some(args.agent.as_str()) {
                return output::format(stdout, &ctx.format, WORKER_COLUMNS, item, None)
                    .map_err(|e| CliError::Other(e.to_string()));
            }
        }
    }

    // Fall back to action message if worker not found in response.
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Worker {} added to group {}.", args.agent, args.group_id),
        &json!({ "groupId": args.group_id, "agentId": args.agent, "added": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))
}

async fn workers_remove(args: WorkersRemoveArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = format!("/api/v1/groups/{}/workers/{}", args.group_id, args.agent);

    ctx.client.do_request(reqwest::Method::DELETE, &path, None, ResponseKind::Auto).await?;

    output::format_action(
        stdout,
        &ctx.format,
        &format!("Worker {} removed from group {}.", args.agent, args.group_id),
        &json!({ "groupId": args.group_id, "agentId": args.agent, "removed": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))
}

async fn dispatch_message(args: DispatchArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut body = serde_json::Map::new();
    body.insert("message".into(), Value::String(args.message));
    if let Some(t) = args.task {
        body.insert("task".into(), Value::String(t));
    }
    let body = Value::Object(body);

    let path = format!("/api/v1/groups/{}/dispatch", args.group_id);

    let result = ctx
        .client
        .do_request(reqwest::Method::POST, &path, Some(&body), ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);

    // For structured formats output the whole map; for table use a human text.
    match ctx.format.as_str() {
        "json" | "yaml" | "quiet" => {
            output::format(stdout, &ctx.format, &[], &result, None).map_err(|e| CliError::Other(e.to_string()))
        }
        _ => {
            let dispatched = result.get("dispatched").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            let total = result.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
            output::format_action(stdout, &ctx.format, &format!("Dispatched to {dispatched}/{total} workers."), &result)
                .map_err(|e| CliError::Other(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_ctx(server_uri: String, format: &str) -> CliContext {
        let client = crate::client::Client::new(crate::client::ClientOptions {
            server: server_uri,
            token: None,
            timeout: std::time::Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();
        CliContext {
            client: Arc::new(client),
            format: format.into(),
            jq: String::new(),
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn list_renders_json_with_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/groups"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "data": [
                    {"id":"g1","name":"alpha","description":"","teamId":"t1","projectId":"p1"},
                    {"id":"g2","name":"beta","description":"","teamId":"t1","projectId":null}
                ],
                "total": 2, "limit": 50, "offset": 0
            })))
            .mount(&server)
            .await;

        let ctx = make_ctx(server.uri(), "json");
        let args = ListArgs { team: None, project: None, limit: 50 };
        let mut out = Vec::new();
        list(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"id\": \"g1\""), "missing g1: {s}");
        assert!(s.contains("\"id\": \"g2\""), "missing g2: {s}");
        assert!(s.contains("\"total\": 2"), "missing pagination: {s}");
    }

    #[tokio::test]
    async fn create_posts_body_and_returns_group() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/groups"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "ok": true,
                "data": {"id":"g1","name":"mygroup","teamId":"t1"}
            })))
            .mount(&server)
            .await;

        let ctx = make_ctx(server.uri(), "json");
        let args = CreateArgs { name: "mygroup".into(), team: "t1".into(), project: None };
        let mut out = Vec::new();
        create(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"id\": \"g1\""), "missing group id: {s}");
    }

    #[tokio::test]
    async fn update_rejects_empty_fields() {
        let server = MockServer::start().await;
        let ctx = make_ctx(server.uri(), "table");
        let args = UpdateArgs { id: "g1".into(), name: None, description: None };
        let mut out = Vec::new();
        let err = update(args, &ctx, &mut out).await.unwrap_err();
        assert!(err.to_string().contains("at least one"), "expected validation error: {err}");
    }

    #[tokio::test]
    async fn delete_aborts_without_force_in_non_interactive() {
        crate::interactive::setup(true, false); // non-interactive
        let server = MockServer::start().await;
        let ctx = make_ctx(server.uri(), "table");
        let args = DeleteArgs { id: "g1".into(), force: false };
        let mut stdin = std::io::Cursor::new(b"".to_vec());
        let mut out = Vec::new();
        let mut err = Vec::new();
        let result = delete(args, &ctx, &mut stdin, &mut out, &mut err).await;
        // Should return ConfirmationRequired (exit 2) in non-interactive without force.
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn workers_add_extracts_new_worker_from_list() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/groups/g1/workers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "workers": [
                    {"agentId":"a1","name":"worker-1"},
                    {"agentId":"a2","name":"worker-2"}
                ]
            })))
            .mount(&server)
            .await;

        let ctx = make_ctx(server.uri(), "json");
        let args = WorkersAddArgs { group_id: "g1".into(), agent: "a2".into(), name: "worker-2".into() };
        let mut out = Vec::new();
        workers_add(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("a2"), "should output the matched worker: {s}");
        assert!(!s.contains("a1"), "should not output other workers: {s}");
    }

    #[tokio::test]
    async fn dispatch_table_format_uses_action_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/groups/g1/dispatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "total": 3,
                "dispatched": ["a1","a2"],
                "failed": 1
            })))
            .mount(&server)
            .await;

        let ctx = make_ctx(server.uri(), "table");
        let args = DispatchArgs { group_id: "g1".into(), message: "hello".into(), task: None };
        let mut out = Vec::new();
        dispatch_message(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("Dispatched to 2/3 workers."), "unexpected output: {s}");
    }

    #[tokio::test]
    async fn dispatch_json_format_returns_full_map() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/groups/g1/dispatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "total": 3,
                "dispatched": ["a1","a2"],
                "failed": 1
            })))
            .mount(&server)
            .await;

        let ctx = make_ctx(server.uri(), "json");
        let args = DispatchArgs { group_id: "g1".into(), message: "hello".into(), task: None };
        let mut out = Vec::new();
        dispatch_message(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"total\""), "json output missing total: {s}");
        assert!(s.contains("\"dispatched\""), "json output missing dispatched: {s}");
    }
}
