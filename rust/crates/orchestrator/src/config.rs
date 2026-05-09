use serde::Deserialize;
use std::env;

fn default_port() -> u16 {
    4010
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_opensearch_url() -> String {
    "http://localhost:9200".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_temporal_host() -> String {
    "localhost:7233".to_string()
}

fn default_temporal_namespace() -> String {
    "orchestrator".to_string()
}

fn default_mcp_endpoint() -> String {
    "http://localhost:4003/mcp".to_string()
}

fn config_message(message: impl Into<String>) -> config::ConfigError {
    config::ConfigError::Message(message.into())
}

fn validate_jwt_signing_key(jwt_signing_key: Option<&str>) -> Result<(), config::ConfigError> {
    let Some(signing_key) = jwt_signing_key else {
        return Ok(());
    };

    if signing_key.len() < 64 {
        return Err(config_message(format!(
            "ORCHESTRATOR_JWT_SIGNING_KEY must be at least 64 characters (32 bytes hex-encoded), got {}",
            signing_key.len()
        )));
    }

    let decoded = hex::decode(signing_key)
        .map_err(|err| config_message(format!("ORCHESTRATOR_JWT_SIGNING_KEY must be valid hex: {err}")))?;
    if decoded.len() < 32 {
        return Err(config_message(format!(
            "ORCHESTRATOR_JWT_SIGNING_KEY must decode to at least 32 bytes, got {}",
            decoded.len()
        )));
    }

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default)]
    pub database_url: String,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default)]
    pub internal_token: Option<String>,

    #[serde(default)]
    pub jwt_signing_key: Option<String>,

    #[serde(default)]
    pub mcp_server_enabled: bool,

    #[serde(default)]
    pub mcp_server_org: String,

    #[serde(default = "default_opensearch_url")]
    pub opensearch_url: String,

    #[serde(default)]
    pub opensearch_enabled: bool,

    #[serde(default)]
    pub embedding_api_url: String,

    #[serde(default)]
    pub embedding_api_key: String,

    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    #[serde(default)]
    pub temporal_enabled: bool,

    #[serde(default = "default_temporal_host")]
    pub temporal_host: String,

    #[serde(default = "default_temporal_namespace")]
    pub temporal_namespace: String,

    #[serde(default = "default_mcp_endpoint")]
    pub mcp_endpoint: String,

    #[serde(default)]
    pub mcp_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
            database_url: String::new(),
            log_level: default_log_level(),
            internal_token: None,
            jwt_signing_key: None,
            mcp_server_enabled: false,
            mcp_server_org: String::new(),
            opensearch_url: default_opensearch_url(),
            opensearch_enabled: false,
            embedding_api_url: String::new(),
            embedding_api_key: String::new(),
            embedding_model: default_embedding_model(),
            temporal_enabled: false,
            temporal_host: default_temporal_host(),
            temporal_namespace: default_temporal_namespace(),
            mcp_endpoint: default_mcp_endpoint(),
            mcp_token: String::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, config::ConfigError> {
        let jwt_signing_key = read("ORCHESTRATOR_JWT_SIGNING_KEY");
        validate_jwt_signing_key(jwt_signing_key.as_deref())?;

        let config = Self {
            port: read("ORCHESTRATOR_PORT").and_then(|value| value.parse().ok()).unwrap_or_else(default_port),
            host: read("ORCHESTRATOR_HOST").unwrap_or_else(default_host),
            database_url: read("ORCHESTRATOR_DATABASE_URL").unwrap_or_default(),
            log_level: read("ORCHESTRATOR_LOG_LEVEL").unwrap_or_else(default_log_level),
            internal_token: read("ORCHESTRATOR_INTERNAL_TOKEN"),
            jwt_signing_key,
            mcp_server_enabled: read("ORCHESTRATOR_MCP_SERVER_ENABLED")
                .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
                .unwrap_or(false),
            mcp_server_org: read("ORCHESTRATOR_MCP_SERVER_ORG").unwrap_or_default(),
            opensearch_url: read("ORCHESTRATOR_OPENSEARCH_URL").unwrap_or_else(default_opensearch_url),
            opensearch_enabled: read("ORCHESTRATOR_OPENSEARCH_ENABLED")
                .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
                .unwrap_or(false),
            embedding_api_url: read("ORCHESTRATOR_EMBEDDING_API_URL").unwrap_or_default(),
            embedding_api_key: read("ORCHESTRATOR_EMBEDDING_API_KEY").unwrap_or_default(),
            embedding_model: read("ORCHESTRATOR_EMBEDDING_MODEL").unwrap_or_else(default_embedding_model),
            temporal_enabled: read("ORCHESTRATOR_TEMPORAL_ENABLED")
                .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
                .unwrap_or(false),
            temporal_host: read("ORCHESTRATOR_TEMPORAL_HOST").unwrap_or_else(default_temporal_host),
            temporal_namespace: read("ORCHESTRATOR_TEMPORAL_NAMESPACE").unwrap_or_else(default_temporal_namespace),
            mcp_endpoint: read("ORCHESTRATOR_MCP_ENDPOINT").unwrap_or_else(default_mcp_endpoint),
            mcp_token: read("ORCHESTRATOR_MCP_TOKEN").unwrap_or_default(),
        };

        config.validate_runtime()?;
        Ok(config)
    }

    pub fn from_env() -> Result<Self, config::ConfigError> {
        Self::load()
    }

    fn validate_runtime(&self) -> Result<(), config::ConfigError> {
        if self.temporal_enabled && self.mcp_token.trim().is_empty() {
            return Err(config_message("ORCHESTRATOR_MCP_TOKEN is required when ORCHESTRATOR_TEMPORAL_ENABLED=true"));
        }

        Ok(())
    }
}

fn read(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}
