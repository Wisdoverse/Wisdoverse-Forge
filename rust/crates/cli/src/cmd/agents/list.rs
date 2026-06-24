use super::COLUMNS;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::{self, Pagination};
use serde_json::Value;
use std::io::Write;

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Filter by project ID
    #[arg(long)]
    pub project: Option<String>,
    /// Filter by status (idle|working|waiting|attention|offline)
    #[arg(long)]
    pub status: Option<String>,
    /// Maximum number of agents to return
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
    /// Offset for pagination
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
}

pub async fn run(args: ListArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.append_pair("scope", "visible");
    if let Some(p) = &args.project {
        q.append_pair("projectId", p);
    }
    if let Some(s) = &args.status {
        q.append_pair("status", s);
    }
    q.append_pair("limit", &args.limit.to_string());
    q.append_pair("offset", &args.offset.to_string());
    let path = format!("/api/v1/agents?{}", q.finish());

    let (items, total, limit, offset) = ctx.client.do_request_list(reqwest::Method::GET, &path, None).await?;

    let pag = Pagination {
        total: total as usize,
        limit: if limit > 0 { limit as usize } else { args.limit as usize },
        offset: if offset > 0 { offset as usize } else { args.offset as usize },
    };
    let data = Value::Array(items);
    output::format_with_jq(stdout, &ctx.format, COLUMNS, &data, Some(&pag), &ctx.jq)
        .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_renders_json_with_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/agents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "data": [
                    {"id":"a","name":"alpha","cliTool":"claude","status":"idle","projectId":"p1","createdAt":"2026-04-14T00:00:00Z"},
                    {"id":"b","name":"beta","cliTool":"codex","status":"working","projectId":"p1","createdAt":"2026-04-14T00:00:00Z"}
                ],
                "total": 2, "limit": 50, "offset": 0
            })))
            .mount(&server).await;

        let client = crate::client::Client::new(crate::client::ClientOptions {
            server: server.uri(),
            token: None,
            timeout: std::time::Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let ctx = CliContext {
            client: Arc::new(client),
            format: "json".into(),
            jq: String::new(),
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        let args = ListArgs { project: None, status: None, limit: 50, offset: 0 };
        let mut out = Vec::new();
        run(args, &ctx, &mut out).await.unwrap();

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"id\": \"a\""), "stdout missing agent a: {s}");
        assert!(s.contains("\"id\": \"b\""), "stdout missing agent b: {s}");
        assert!(s.contains("\"total\": 2"), "stdout missing pagination: {s}");
    }
}
