//! Sidecar-specific configuration loaded from environment variables.

use serde::Deserialize;

/// Configuration for the sidecar process running inside each agent container.
#[derive(Debug, Deserialize, Clone)]
pub struct SidecarConfig {
    /// NATS server URL (required).
    pub nats_url: String,
    /// Agent identifier set by the platform on container creation.
    pub agent_id: String,
    /// Per-container HMAC secret injected at startup.
    pub hmac_secret: String,
    /// Path to the WAL directory (default: `/tmp/agentforge-wal`).
    pub wal_path: Option<String>,
    /// Heartbeat interval in seconds (default: 30).
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval_secs: u64,
    /// Which wrapped CLI the worker bridge should invoke when an orchestration
    /// assignment arrives. Mirrors `AGENTFORGE_CLI_TOOL` which agent-entrypoint
    /// already reads. Unset → orchestration subscriber stays disabled (legacy
    /// behaviour: DB moves to `working` but no work runs).
    ///
    /// Resolution order at runtime: the typed config field first, then the
    /// raw `AGENTFORGE_CLI_TOOL` env var (matches the existing entrypoint
    /// contract), then `None`.
    pub cli_tool: Option<String>,
    /// Optional model override passed to wrapped CLIs that support explicit
    /// model selection. The platform can inject `AGENTFORGE_CLI_MODEL` without
    /// relying on user-local CLI config files.
    #[serde(default)]
    pub cli_model: Option<String>,

    /// The agent's runtime kind (`container` | `cli` | `api`), used to namespace
    /// the NATS event-ingest subject (issue #457). Mirrors
    /// `AGENTFORGE_RUNTIME_KIND`, injected by the platform at container creation
    /// and enrollment. Unset (older images) → resolved to `container`.
    #[serde(default)]
    pub runtime_kind: Option<String>,

    /// Organisation the agent belongs to. Set by the platform at container
    /// creation (via `ORG_ID` env var) so the sidecar can embed it in
    /// credential-sync messages. The consumer re-resolves from DB either way,
    /// so this is convenience, not authority.
    #[serde(default)]
    pub org_id: Option<String>,

    /// Path inside the container to watch for CLI credential files. Derived
    /// from `CREDS_DIR` exported by `agent-entrypoint.sh` per CLI.
    #[serde(default)]
    pub creds_dir: Option<String>,

    /// Rollout gate (issue #41). When `false`, the watcher is not spawned.
    /// Default `false` so stale deploys don't publish until operators flip
    /// the env var.
    #[serde(default)]
    pub credential_sync_enabled: bool,
}

fn default_heartbeat() -> u64 {
    30
}

impl SidecarConfig {
    /// Build configuration from environment variables.
    ///
    /// Environment variables are matched case-insensitively and nested keys use
    /// `__` as a separator (e.g. `NATS_URL`, `AGENT_ID`).
    pub fn from_env() -> Result<Self, config::ConfigError> {
        config::Config::builder().add_source(config::Environment::default().separator("__")).build()?.try_deserialize()
    }

    /// Best-effort resolution of the CLI tool name: config field wins, otherwise
    /// fall back to the `AGENTFORGE_CLI_TOOL` env var baked into agent images.
    pub fn resolved_cli_tool(&self) -> Option<String> {
        if let Some(t) = &self.cli_tool {
            return Some(t.clone());
        }
        std::env::var("AGENTFORGE_CLI_TOOL").ok()
    }

    /// Resolve the runtime kind for event-ingest subject namespacing (#457).
    ///
    /// Resolution order mirrors `resolved_cli_tool`: typed config field first,
    /// then the raw `AGENTFORGE_RUNTIME_KIND` env baked into agent images. The
    /// value is canonicalised against the supported set; anything unrecognised
    /// (including unset, i.e. pre-#457 images) resolves to `container` so the
    /// sidecar always publishes on a valid, grantable subject.
    pub fn resolved_runtime_kind(&self) -> agentforge_core::RuntimeKind {
        let raw = self.runtime_kind.clone().or_else(|| std::env::var("AGENTFORGE_RUNTIME_KIND").ok());
        raw.as_deref()
            .and_then(|v| agentforge_core::RuntimeKind::parse_legacy(v).ok())
            .unwrap_or(agentforge_core::RuntimeKind::Container)
    }

    pub fn resolved_cli_model(&self) -> Option<String> {
        if let Some(model) = &self.cli_model {
            let model = model.trim();
            if !model.is_empty() {
                return Some(model.to_string());
            }
        }
        std::env::var("AGENTFORGE_CLI_MODEL")
            .ok()
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_heartbeat() {
        assert_eq!(default_heartbeat(), 30);
    }

    #[test]
    fn test_deserialization_with_defaults() {
        // Build a config from an in-memory map to verify defaults.
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://localhost:4222")
            .unwrap()
            .set_override("agent_id", "agent-abc")
            .unwrap()
            .set_override("hmac_secret", "secret123")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();

        assert_eq!(cfg.nats_url, "nats://localhost:4222");
        assert_eq!(cfg.agent_id, "agent-abc");
        assert_eq!(cfg.hmac_secret, "secret123");
        assert!(cfg.wal_path.is_none());
        assert_eq!(cfg.heartbeat_interval_secs, 30);
        assert!(cfg.cli_model.is_none());
    }

    #[test]
    fn test_deserialization_with_overrides() {
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://remote:4222")
            .unwrap()
            .set_override("agent_id", "agent-xyz")
            .unwrap()
            .set_override("hmac_secret", "s3cret")
            .unwrap()
            .set_override("wal_path", "/data/wal")
            .unwrap()
            .set_override("heartbeat_interval_secs", 10_i64)
            .unwrap()
            .set_override("cli_model", "gpt-5.4-mini")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();

        assert_eq!(cfg.wal_path.as_deref(), Some("/data/wal"));
        assert_eq!(cfg.heartbeat_interval_secs, 10);
        assert_eq!(cfg.cli_model.as_deref(), Some("gpt-5.4-mini"));
    }

    #[test]
    fn test_runtime_kind_resolves_from_field_and_defaults_to_container() {
        // Explicit field wins and is canonicalised.
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://localhost:4222")
            .unwrap()
            .set_override("agent_id", "agent-abc")
            .unwrap()
            .set_override("hmac_secret", "secret123")
            .unwrap()
            .set_override("runtime_kind", "CLI")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();
        assert_eq!(cfg.resolved_runtime_kind(), agentforge_core::RuntimeKind::Cli);

        // An unrecognised value canonicalises to the container default rather
        // than failing — the sidecar must always publish on a grantable subject.
        // (Field is set so the env fallback isn't consulted, keeping the test
        // independent of the ambient process environment.)
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://localhost:4222")
            .unwrap()
            .set_override("agent_id", "agent-abc")
            .unwrap()
            .set_override("hmac_secret", "secret123")
            .unwrap()
            .set_override("runtime_kind", "wat")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();
        assert_eq!(cfg.resolved_runtime_kind(), agentforge_core::RuntimeKind::Container);
    }

    #[test]
    fn test_credential_sync_fields_default_off() {
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://localhost:4222")
            .unwrap()
            .set_override("agent_id", "agent-abc")
            .unwrap()
            .set_override("hmac_secret", "secret123")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();
        assert!(!cfg.credential_sync_enabled);
        assert!(cfg.creds_dir.is_none());
        assert!(cfg.org_id.is_none());
    }

    #[test]
    fn test_credential_sync_fields_populate_from_env() {
        let cfg = config::Config::builder()
            .set_override("nats_url", "nats://localhost:4222")
            .unwrap()
            .set_override("agent_id", "agent-abc")
            .unwrap()
            .set_override("hmac_secret", "secret123")
            .unwrap()
            .set_override("org_id", "00000000-0000-0000-0000-000000000001")
            .unwrap()
            .set_override("creds_dir", "/home/agent/.claude")
            .unwrap()
            .set_override("credential_sync_enabled", true)
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<SidecarConfig>()
            .unwrap();
        assert_eq!(cfg.org_id.as_deref(), Some("00000000-0000-0000-0000-000000000001"));
        assert_eq!(cfg.creds_dir.as_deref(), Some("/home/agent/.claude"));
        assert!(cfg.credential_sync_enabled);
    }
}
