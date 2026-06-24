use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[async_trait]
pub trait OutboundMcp: Send + Sync {
    async fn session_create(&self, args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult>;
    async fn session_prompt(&self, agent_id: &str, prompt: &str) -> anyhow::Result<()>;
    async fn session_destroy(&self, agent_id: &str) -> anyhow::Result<()>;
    async fn session_status(&self, agent_id: &str) -> anyhow::Result<SessionStatusResult>;
}

#[derive(Debug)]
pub struct OutboundMcpClient {
    endpoint: String,
    token: String,
    http: reqwest::Client,
    request_id: AtomicI64,
}

impl OutboundMcpClient {
    pub fn new(endpoint: String, token: String) -> anyhow::Result<Self> {
        let http =
            reqwest::Client::builder().timeout(Duration::from_secs(30)).build().context("build outbound MCP client")?;
        Ok(Self { endpoint, token, http, request_id: AtomicI64::new(0) })
    }

    pub fn build_tool_call(tool: &str, arguments: Value, id: i64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": arguments,
            }
        })
    }

    pub async fn session_create(&self, args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult> {
        self.call_tool_with_aliases(
            &["wisdoverse.agent.create", "agentforge.agent.create"],
            json!({
                "projectId": args.project_id,
                "cliTool": args.cli_tool,
                "name": args.name,
            }),
        )
        .await
    }

    pub async fn session_prompt(&self, agent_id: &str, prompt: &str) -> anyhow::Result<()> {
        let _: Value = self
            .call_tool_with_aliases(
                &["wisdoverse.agent.prompt", "agentforge.agent.prompt"],
                json!({
                    "agentId": agent_id,
                    "prompt": prompt,
                }),
            )
            .await?;
        Ok(())
    }

    pub async fn session_destroy(&self, agent_id: &str) -> anyhow::Result<()> {
        let _: Value = self
            .call_tool_with_aliases(
                &["wisdoverse.agent.destroy", "agentforge.agent.destroy"],
                json!({ "agentId": agent_id }),
            )
            .await?;
        Ok(())
    }

    pub async fn session_status(&self, agent_id: &str) -> anyhow::Result<SessionStatusResult> {
        self.call_tool_with_aliases(
            &["wisdoverse.agent.status", "agentforge.agent.status"],
            json!({ "agentId": agent_id }),
        )
        .await
    }

    async fn call_tool_with_aliases<T: DeserializeOwned>(&self, tools: &[&str], arguments: Value) -> anyhow::Result<T> {
        let mut last_err = None;
        for tool in tools {
            match self.call_tool(tool, arguments.clone()).await {
                Ok(value) => return Ok(value),
                Err(err) if is_unknown_tool_error(&err) => {
                    last_err = Some(err);
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("no MCP tool aliases configured")))
    }

    async fn call_tool<T: DeserializeOwned>(&self, tool: &str, arguments: Value) -> anyhow::Result<T> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed) + 1;
        let request = Self::build_tool_call(tool, arguments, id);
        let mut http_request = self.http.post(&self.endpoint).json(&request);
        if !self.token.is_empty() {
            http_request = http_request.bearer_auth(&self.token);
        }

        let response = http_request.send().await.context("send outbound MCP request")?;
        let status = response.status();
        let body = response.bytes().await.context("read outbound MCP response body")?;
        if !status.is_success() {
            bail!("MCP returned status {}: {}", status.as_u16(), String::from_utf8_lossy(&body));
        }

        let rpc: RpcResponse = serde_json::from_slice(&body).context("decode outbound MCP response")?;
        if let Some(error) = rpc.error {
            bail!("MCP error {}: {}", error.code, error.message);
        }

        let result = rpc.result.context("outbound MCP response missing result")?;
        let payload = decode_tool_payload(result)?;
        serde_json::from_value(payload).context("decode outbound MCP result")
    }
}

#[async_trait]
impl OutboundMcp for OutboundMcpClient {
    async fn session_create(&self, args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult> {
        Self::session_create(self, args).await
    }

    async fn session_prompt(&self, agent_id: &str, prompt: &str) -> anyhow::Result<()> {
        Self::session_prompt(self, agent_id, prompt).await
    }

    async fn session_destroy(&self, agent_id: &str) -> anyhow::Result<()> {
        Self::session_destroy(self, agent_id).await
    }

    async fn session_status(&self, agent_id: &str) -> anyhow::Result<SessionStatusResult> {
        Self::session_status(self, agent_id).await
    }
}

fn decode_tool_payload(result: Value) -> anyhow::Result<Value> {
    let Some(content) = result.get("content") else {
        return Ok(result);
    };

    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        let message = content
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("MCP tool returned isError=true");
        bail!("MCP tool error: {message}");
    }

    let text = content
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .context("outbound MCP tool result missing content[0].text")?;
    serde_json::from_str(text).context("decode outbound MCP tool content text")
}

fn is_unknown_tool_error(err: &anyhow::Error) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    message.contains("unknown tool") || message.contains("method not found")
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: i64,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionArgs {
    pub project_id: String,
    pub cli_tool: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResult {
    pub agent_id: String,
    pub status: String,
    pub name: String,
}

impl CreateSessionResult {
    pub fn session_id(&self) -> &str {
        &self.agent_id
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusResult {
    pub agent_id: String,
    pub status: String,
}

impl SessionStatusResult {
    pub fn session_id(&self) -> &str {
        &self.agent_id
    }
}
