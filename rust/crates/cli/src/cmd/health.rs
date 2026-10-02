use crate::client::ResponseKind;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output::{self, Column};
use serde_json::Value;
use std::io::Write;

const COLUMNS: &[Column] = &[Column { header: "STATUS", field: "status" }];

pub async fn run(ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let result = ctx
        .client
        .do_request(reqwest::Method::GET, "/api/health", None, ResponseKind::Auto)
        .await?
        .unwrap_or(Value::Null);
    output::format(stdout, &ctx.format, COLUMNS, &result, None).map_err(|e| CliError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_info::BuildInfo;
    use crate::client::{Client, ClientOptions};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn health_and_version_use_the_current_readiness_route() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok":true, "status":"ready", "checks":{"database":true,"redis":false,"nats":true,"docker":true}
            })))
            .expect(2)
            .mount(&server)
            .await;
        let ctx = CliContext {
            client: Arc::new(
                Client::new(ClientOptions {
                    server: server.uri(),
                    token: None,
                    timeout: Duration::from_secs(5),
                    insecure: false,
                    verbose: false,
                    debug: false,
                    trace: false,
                })
                .unwrap(),
            ),
            format: "json".into(),
            jq: String::new(),
            cancel: tokio_util::sync::CancellationToken::new(),
        };
        let mut out = Vec::new();
        run(&ctx, &mut out).await.unwrap();
        let health: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(health["data"]["status"], "ready");
        assert_eq!(health["data"]["checks"]["database"], true);
        assert_eq!(health["data"]["checks"]["redis"], false);
        out.clear();
        let mut stderr = Vec::new();
        super::super::version::run(&BuildInfo::from_env(), &ctx, &mut out, &mut stderr).await.unwrap();
        let version: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(version["data"]["serverVersion"], "(unknown)");
        assert!(stderr.is_empty());
    }
}
