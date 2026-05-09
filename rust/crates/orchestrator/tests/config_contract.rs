use temp_env::with_vars;

use agentforge_orchestrator::config::Config;

#[test]
fn loads_go_compatible_knowledge_defaults() {
    with_vars(
        [
            ("ORCHESTRATOR_OPENSEARCH_URL", None::<&str>),
            ("ORCHESTRATOR_OPENSEARCH_ENABLED", None::<&str>),
            ("ORCHESTRATOR_EMBEDDING_API_URL", None::<&str>),
            ("ORCHESTRATOR_EMBEDDING_API_KEY", None::<&str>),
            ("ORCHESTRATOR_EMBEDDING_MODEL", None::<&str>),
            ("ORCHESTRATOR_DATABASE_URL", None::<&str>),
            ("ORCHESTRATOR_INTERNAL_TOKEN", None::<&str>),
            ("ORCHESTRATOR_JWT_SIGNING_KEY", None::<&str>),
        ],
        || {
            let config = Config::load().expect("config should load with defaults");
            assert_eq!(config.opensearch_url, "http://localhost:9200");
            assert!(!config.opensearch_enabled);
            assert_eq!(config.embedding_api_url, "");
            assert_eq!(config.embedding_api_key, "");
            assert_eq!(config.embedding_model, "text-embedding-3-small");
            assert_eq!(config.jwt_signing_key, None);
        },
    );
}

#[test]
fn loads_go_compatible_workflow_runtime_defaults() {
    with_vars(
        [
            ("ORCHESTRATOR_TEMPORAL_ENABLED", None::<&str>),
            ("ORCHESTRATOR_TEMPORAL_HOST", None::<&str>),
            ("ORCHESTRATOR_TEMPORAL_NAMESPACE", None::<&str>),
            ("ORCHESTRATOR_MCP_ENDPOINT", None::<&str>),
            ("ORCHESTRATOR_MCP_TOKEN", None::<&str>),
        ],
        || {
            let config = Config::load().expect("config should load with defaults");
            assert!(!config.temporal_enabled);
            assert_eq!(config.temporal_host, "localhost:7233");
            assert_eq!(config.temporal_namespace, "orchestrator");
            assert_eq!(config.mcp_endpoint, "http://localhost:4003/mcp");
            assert_eq!(config.mcp_token, "");
        },
    );
}

#[test]
fn overrides_knowledge_env_vars() {
    with_vars(
        [
            ("ORCHESTRATOR_OPENSEARCH_URL", Some("http://opensearch:9200")),
            ("ORCHESTRATOR_OPENSEARCH_ENABLED", Some("true")),
            ("ORCHESTRATOR_EMBEDDING_API_URL", Some("http://embedder:8080/v1")),
            ("ORCHESTRATOR_EMBEDDING_API_KEY", Some("abc123")),
            ("ORCHESTRATOR_EMBEDDING_MODEL", Some("text-embedding-3-large")),
            ("ORCHESTRATOR_JWT_SIGNING_KEY", Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")),
        ],
        || {
            let config = Config::load().expect("config should load");
            assert_eq!(config.opensearch_url, "http://opensearch:9200");
            assert!(config.opensearch_enabled);
            assert_eq!(config.embedding_api_url, "http://embedder:8080/v1");
            assert_eq!(config.embedding_api_key, "abc123");
            assert_eq!(config.embedding_model, "text-embedding-3-large");
            assert_eq!(
                config.jwt_signing_key.as_deref(),
                Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            );
        },
    );
}

#[test]
fn overrides_workflow_runtime_env_vars() {
    with_vars(
        [
            ("ORCHESTRATOR_TEMPORAL_ENABLED", Some("true")),
            ("ORCHESTRATOR_TEMPORAL_HOST", Some("temporal.example:7233")),
            ("ORCHESTRATOR_TEMPORAL_NAMESPACE", Some("prod")),
            ("ORCHESTRATOR_MCP_ENDPOINT", Some("http://mcp:4003/mcp")),
            ("ORCHESTRATOR_MCP_TOKEN", Some("secret-token")),
        ],
        || {
            let config = Config::load().expect("config should load");
            assert!(config.temporal_enabled);
            assert_eq!(config.temporal_host, "temporal.example:7233");
            assert_eq!(config.temporal_namespace, "prod");
            assert_eq!(config.mcp_endpoint, "http://mcp:4003/mcp");
            assert_eq!(config.mcp_token, "secret-token");
        },
    );
}

#[test]
fn temporal_enabled_requires_mcp_token() {
    with_vars([("ORCHESTRATOR_TEMPORAL_ENABLED", Some("true")), ("ORCHESTRATOR_MCP_TOKEN", Some(""))], || {
        let err = Config::load().expect_err("missing MCP token should fail when Temporal is enabled");
        assert!(err.to_string().contains("ORCHESTRATOR_MCP_TOKEN"));
    });
}

#[test]
fn rejects_short_jwt_signing_key() {
    with_vars([("ORCHESTRATOR_JWT_SIGNING_KEY", Some("abcd"))], || {
        let err = Config::load().expect_err("short signing key should fail");
        assert!(err.to_string().contains("at least 64 characters"));
    });
}

#[test]
fn rejects_non_hex_jwt_signing_key() {
    with_vars(
        [("ORCHESTRATOR_JWT_SIGNING_KEY", Some("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"))],
        || {
            let err = Config::load().expect_err("non-hex signing key should fail");
            assert!(err.to_string().contains("hex"));
        },
    );
}
