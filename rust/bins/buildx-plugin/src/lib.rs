use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::client::conn::http1;
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::net::UnixStream;
use url::form_urlencoded::Serializer;

pub const DEFAULT_DOCKER_PROXY_SOCKET: &str = "/tmp/docker-proxy.sock";
const DEFAULT_CONTAINER_WORKSPACE: &str = "/workspace";
const BUILD_API_PATH: &str = "/v1.45/build";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginMetadata {
    #[serde(rename = "SchemaVersion")]
    pub schema_version: String,
    #[serde(rename = "Vendor")]
    pub vendor: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "ShortDescription")]
    pub short_description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildCommand {
    pub context: String,
    pub dockerfile: String,
    pub tags: Vec<String>,
    pub build_args: BTreeMap<String, String>,
    pub no_cache: bool,
    pub pull: bool,
    pub target: String,
    pub cache_from: Vec<String>,
}

pub fn plugin_metadata() -> PluginMetadata {
    PluginMetadata {
        schema_version: "0.1.0".to_string(),
        vendor: "Wisdoverse Forge".to_string(),
        version: buildx_plugin_version().to_string(),
        short_description: "Wisdoverse Forge buildx sidecar proxy".to_string(),
    }
}

fn buildx_plugin_version() -> &'static str {
    option_env!("AGENTFORGE_BUILDX_PLUGIN_VERSION").unwrap_or("dev")
}

pub fn normalize_args(raw_args: &[String]) -> &[String] {
    if matches!(raw_args.first(), Some(first) if first == "buildx") { &raw_args[1..] } else { raw_args }
}

pub fn parse_build_args(args: &[String]) -> Result<BuildCommand> {
    let mut cmd = BuildCommand {
        context: String::new(),
        dockerfile: "Dockerfile".to_string(),
        tags: Vec::new(),
        build_args: BTreeMap::new(),
        no_cache: false,
        pull: false,
        target: String::new(),
        cache_from: Vec::new(),
    };

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-f" | "--file" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| anyhow!("-f requires argument"))?;
                cmd.dockerfile = value.clone();
            }
            "-t" | "--tag" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| anyhow!("-t requires argument"))?;
                cmd.tags.push(value.clone());
            }
            "--build-arg" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| anyhow!("--build-arg requires argument"))?;
                let (key, value) = split_build_arg(value);
                cmd.build_args.insert(key, value);
            }
            "--no-cache" => cmd.no_cache = true,
            "--pull" => cmd.pull = true,
            "--target" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| anyhow!("--target requires argument"))?;
                cmd.target = value.clone();
            }
            "--cache-from" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| anyhow!("--cache-from requires argument"))?;
                cmd.cache_from.push(value.clone());
            }
            "--load" => {}
            "--progress" | "--platform" | "--ssh" | "--secret" => {
                index += 1;
            }
            value if value.starts_with('-') => {
                if args.get(index + 1).is_some_and(|next| !next.starts_with('-')) {
                    index += 1;
                }
            }
            value => cmd.context = value.to_string(),
        }
        index += 1;
    }

    if cmd.context.is_empty() {
        bail!("build context path required");
    }

    Ok(cmd)
}

pub struct ProxyClient {
    socket_path: PathBuf,
}

impl ProxyClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self { socket_path: socket_path.into() }
    }

    pub async fn build<W>(&self, cmd: &BuildCommand, stdout: &mut W) -> Result<()>
    where
        W: Write,
    {
        let request = build_request(cmd).context("create request")?;

        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("connect docker proxy socket {}", self.socket_path.display()))?;
        let io = TokioIo::new(stream);
        let (mut sender, connection) = http1::handshake(io).await.context("proxy handshake")?;

        tokio::spawn(async move {
            let _ = connection.await;
        });

        let mut response = sender.send_request(request).await.context("proxy request")?;

        if response.status() != StatusCode::OK {
            let body = response.body_mut().collect().await.context("read error response body")?.to_bytes();
            bail!("build failed (HTTP {}): {}", response.status().as_u16(), String::from_utf8_lossy(&body));
        }

        while let Some(frame) = response.body_mut().frame().await {
            let frame = frame.context("read build response frame")?;
            if let Some(data) = frame.data_ref() {
                stdout.write_all(data).context("write build output")?;
            }
        }
        stdout.flush().context("flush build output")?;

        Ok(())
    }
}

pub async fn run_build<W>(args: &[String], stdout: &mut W) -> Result<()>
where
    W: Write,
{
    let cmd = parse_build_args(args).context("parse args")?;
    let socket_path =
        std::env::var("AGENTFORGE_DOCKER_PROXY_SOCKET").unwrap_or_else(|_| DEFAULT_DOCKER_PROXY_SOCKET.to_string());
    let client = ProxyClient::new(socket_path);
    client.build(&cmd, stdout).await
}

fn build_request(cmd: &BuildCommand) -> Result<Request<Empty<Bytes>>> {
    let mut params = Serializer::new(String::new());
    params.append_pair("dockerfile", &cmd.dockerfile);
    params.append_pair("rm", "1");
    for tag in &cmd.tags {
        params.append_pair("t", tag);
    }
    if cmd.no_cache {
        params.append_pair("nocache", "1");
    }
    if cmd.pull {
        params.append_pair("pull", "1");
    }
    if !cmd.target.is_empty() {
        params.append_pair("target", &cmd.target);
    }
    if !cmd.build_args.is_empty() {
        let build_args = serde_json::to_string(&cmd.build_args).context("encode build args")?;
        params.append_pair("buildargs", &build_args);
    }

    let query = params.finish();
    let uri = format!("{BUILD_API_PATH}?{query}");

    let mut builder =
        Request::builder().method("POST").uri(uri).header(HOST, "localhost").header(CONTENT_TYPE, "application/x-tar");

    if let Some(subdir) = context_subdir(&cmd.context)? {
        builder = builder.header("X-Context-Subdir", subdir);
    }

    builder.body(Empty::new()).map_err(|err| anyhow!("invalid request: {err}"))
}

fn split_build_arg(value: &str) -> (String, String) {
    match value.split_once('=') {
        Some((key, value)) => (key.to_string(), value.to_string()),
        None => (value.to_string(), String::new()),
    }
}

fn context_subdir(context: &str) -> Result<Option<String>> {
    let abs_context = absolute_clean_path(Path::new(context)).context("resolve build context path")?;
    Ok(context_subdir_from_abs_path(&abs_context))
}

fn context_subdir_from_abs_path(abs_context: &Path) -> Option<String> {
    let workspace_root = Path::new(DEFAULT_CONTAINER_WORKSPACE);
    abs_context.strip_prefix(workspace_root).ok().map(|subdir| {
        let text = subdir.to_string_lossy();
        text.trim_start_matches('/').to_string()
    })
}

fn absolute_clean_path(path: &Path) -> Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("resolve current working directory")?.join(path)
    };

    Ok(normalize_path(joined))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::StatusCode as AxumStatusCode;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Router, serve};
    use serde_json::{Value, json};
    use tempfile::tempdir;
    use tokio::net::UnixListener;

    use super::*;

    #[test]
    fn plugin_metadata_contains_expected_fields() {
        let data = serde_json::to_value(plugin_metadata()).expect("serialize metadata");
        assert_eq!(data["SchemaVersion"], "0.1.0");
        assert_eq!(data["Vendor"], "Wisdoverse Forge");
    }

    #[test]
    fn parse_build_args_basic() {
        let args = string_args(["-f", "Dockerfile.prod", "-t", "myapp:latest", "--build-arg", "NODE_ENV=prod", "."]);
        let cmd = parse_build_args(&args).expect("parse build args");
        assert_eq!(cmd.dockerfile, "Dockerfile.prod");
        assert_eq!(cmd.tags, vec!["myapp:latest"]);
        assert_eq!(cmd.context, ".");
        assert_eq!(cmd.build_args.get("NODE_ENV"), Some(&"prod".to_string()));
    }

    #[test]
    fn parse_build_args_multiple_tags() {
        let args = string_args(["-t", "app:v1", "-t", "app:latest", "/workspace/myproject"]);
        let cmd = parse_build_args(&args).expect("parse build args");
        assert_eq!(cmd.tags, vec!["app:v1", "app:latest"]);
        assert_eq!(cmd.context, "/workspace/myproject");
    }

    #[test]
    fn parse_build_args_flags() {
        let args = string_args(["--no-cache", "--pull", "--target", "builder", "--load", "."]);
        let cmd = parse_build_args(&args).expect("parse build args");
        assert!(cmd.no_cache);
        assert!(cmd.pull);
        assert_eq!(cmd.target, "builder");
    }

    #[test]
    fn parse_build_args_default_dockerfile() {
        let args = string_args(["-t", "app:latest", "."]);
        let cmd = parse_build_args(&args).expect("parse build args");
        assert_eq!(cmd.dockerfile, "Dockerfile");
    }

    #[test]
    fn parse_build_args_requires_context() {
        let args = string_args(["-t", "app:latest"]);
        let err = parse_build_args(&args).expect_err("missing context should fail");
        assert!(err.to_string().contains("build context path required"));
    }

    #[test]
    fn parse_build_args_cache_from() {
        let args = string_args(["--cache-from", "app:latest", "--cache-from", "app:v1", "."]);
        let cmd = parse_build_args(&args).expect("parse build args");
        assert_eq!(cmd.cache_from, vec!["app:latest", "app:v1"]);
    }

    #[test]
    fn normalize_args_strips_buildx_prefix() {
        let args = string_args(["buildx", "build", "."]);
        assert_eq!(normalize_args(&args), &args[1..]);
    }

    #[test]
    fn context_subdir_is_derived_from_workspace_root() {
        let abs_context = Path::new("/workspace/myproject");
        assert_eq!(context_subdir_from_abs_path(abs_context), Some("myproject".to_string()));
    }

    #[tokio::test]
    async fn proxy_client_build_streams_output() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("test.sock");

        let router = Router::new().route(
            BUILD_API_PATH,
            post(|request: Request| async move {
                assert_eq!(request.method(), "POST");
                assert_eq!(
                    request.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok()),
                    Some("application/x-tar")
                );
                assert_eq!(request.uri().query(), Some("dockerfile=Dockerfile.prod&rm=1&t=myapp%3Alatest"));
                (
                    [(CONTENT_TYPE, "application/json")],
                    Body::from(
                        "{\"stream\":\"Step 1/3 : FROM alpine\\n\"}\n{\"stream\":\"Successfully built abc123\\n\"}\n",
                    ),
                )
            }),
        );

        let _server = spawn_unix_server(&socket_path, router).await;

        let client = ProxyClient::new(&socket_path);
        let mut output = Vec::new();
        client
            .build(
                &BuildCommand {
                    context: ".".to_string(),
                    dockerfile: "Dockerfile.prod".to_string(),
                    tags: vec!["myapp:latest".to_string()],
                    build_args: BTreeMap::new(),
                    no_cache: false,
                    pull: false,
                    target: String::new(),
                    cache_from: Vec::new(),
                },
                &mut output,
            )
            .await
            .expect("build should succeed");

        let output = String::from_utf8(output).expect("utf8 output");
        assert!(output.contains("Step 1/3"));
    }

    #[tokio::test]
    async fn proxy_client_build_error_includes_response_body() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("test.sock");

        let router = Router::new().route(
            BUILD_API_PATH,
            post(|| async move {
                (AxumStatusCode::INTERNAL_SERVER_ERROR, Body::from("{\"message\":\"internal error\"}")).into_response()
            }),
        );

        let _server = spawn_unix_server(&socket_path, router).await;

        let client = ProxyClient::new(&socket_path);
        let mut output = Vec::new();
        let err = client
            .build(
                &BuildCommand {
                    context: ".".to_string(),
                    dockerfile: "Dockerfile".to_string(),
                    tags: vec!["app:latest".to_string()],
                    build_args: BTreeMap::new(),
                    no_cache: false,
                    pull: false,
                    target: String::new(),
                    cache_from: Vec::new(),
                },
                &mut output,
            )
            .await
            .expect_err("500 response should fail");

        assert!(err.to_string().contains("build failed (HTTP 500)"));
        assert!(err.to_string().contains("internal error"));
    }

    #[tokio::test]
    async fn proxy_client_preserves_multitag_and_buildargs() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("test.sock");
        let seen = Arc::new(Mutex::new(HashMap::<String, Value>::new()));
        let seen_clone = Arc::clone(&seen);

        let router = Router::new().route(
            BUILD_API_PATH,
            post(move |request: Request| {
                let seen = Arc::clone(&seen_clone);
                async move {
                    let query = request.uri().query().unwrap_or_default();
                    let parsed: Vec<(String, String)> =
                        url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();

                    let mut tags = Vec::new();
                    let mut values = HashMap::new();
                    for (key, value) in parsed {
                        if key == "t" {
                            tags.push(Value::String(value));
                        } else {
                            values.insert(key, Value::String(value));
                        }
                    }
                    values.insert("t".to_string(), Value::Array(tags));
                    *seen.lock().expect("lock seen") = values;

                    Body::from("{\"stream\":\"done\\n\"}").into_response()
                }
            }),
        );

        let _server = spawn_unix_server(&socket_path, router).await;

        let mut build_args = BTreeMap::new();
        build_args.insert("VERSION".to_string(), "2.0".to_string());
        build_args.insert("ENV".to_string(), "prod".to_string());

        let client = ProxyClient::new(&socket_path);
        let mut output = Vec::new();
        client
            .build(
                &BuildCommand {
                    context: ".".to_string(),
                    dockerfile: "Dockerfile".to_string(),
                    tags: vec!["app:v1".to_string(), "app:v2".to_string(), "app:latest".to_string()],
                    build_args,
                    no_cache: false,
                    pull: false,
                    target: String::new(),
                    cache_from: Vec::new(),
                },
                &mut output,
            )
            .await
            .expect("build should succeed");

        let captured = seen.lock().expect("lock seen");
        assert_eq!(captured.get("t"), Some(&json!(["app:v1", "app:v2", "app:latest"])));

        let build_args: HashMap<String, String> =
            serde_json::from_str(captured["buildargs"].as_str().expect("buildargs string")).expect("decode buildargs");
        assert_eq!(build_args.get("VERSION"), Some(&"2.0".to_string()));
        assert_eq!(build_args.get("ENV"), Some(&"prod".to_string()));
    }

    #[tokio::test]
    async fn proxy_client_sets_context_subdir_header() {
        let dir = tempdir().expect("tempdir");
        let socket_path = dir.path().join("test.sock");
        let captured_subdir = Arc::new(Mutex::new(String::new()));
        let captured_clone = Arc::clone(&captured_subdir);

        let router = Router::new().route(
            BUILD_API_PATH,
            post(move |request: Request| {
                let captured = Arc::clone(&captured_clone);
                async move {
                    *captured.lock().expect("lock captured") = request
                        .headers()
                        .get("X-Context-Subdir")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    Body::from("{\"stream\":\"done\\n\"}").into_response()
                }
            }),
        );

        let _server = spawn_unix_server(&socket_path, router).await;

        if let Err(err) = std::fs::create_dir_all("/workspace/myproject") {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                return;
            }
            panic!("create workspace path: {err}");
        }
        let original_dir = std::env::current_dir().expect("current dir");
        if let Err(err) = std::env::set_current_dir("/workspace/myproject") {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                return;
            }
            panic!("set workspace cwd: {err}");
        }

        let client = ProxyClient::new(&socket_path);
        let mut output = Vec::new();
        let result = client
            .build(
                &BuildCommand {
                    context: ".".to_string(),
                    dockerfile: "Dockerfile".to_string(),
                    tags: vec!["app:latest".to_string()],
                    build_args: BTreeMap::new(),
                    no_cache: false,
                    pull: false,
                    target: String::new(),
                    cache_from: Vec::new(),
                },
                &mut output,
            )
            .await;
        std::env::set_current_dir(original_dir).expect("restore cwd");

        result.expect("build should succeed");
        assert_eq!(captured_subdir.lock().expect("lock captured").as_str(), "myproject");
    }

    async fn spawn_unix_server(socket_path: &Path, router: Router) -> tokio::task::JoinHandle<()> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path).expect("remove stale socket");
        }

        let listener = UnixListener::bind(socket_path).expect("bind unix listener");
        tokio::spawn(async move {
            serve(listener, router).await.expect("serve unix router");
        })
    }

    fn string_args<const N: usize>(values: [&str; N]) -> Vec<String> {
        values.into_iter().map(str::to_string).collect()
    }
}
