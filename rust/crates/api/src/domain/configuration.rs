//! Platform configuration domain rules.
//!
//! This module owns pure validation and normalization policies for
//! operator-managed configuration surfaces such as quotas, resource profiles,
//! dashboard tiles, and plugin catalog entries.

use agentforge_core::{AppResult, CliToolKind, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const VALID_QUOTA_RESOURCE_TYPES: &[&str] = &["agents", "storage", "events"];
const VALID_TILE_TYPES: &[&str] = &["agent", "feed", "chart", "custom"];
const VALID_RUNTIME_BACKENDS: &[&str] = &["container", "api"];
const VALID_GATEWAY_ROUTING_STRATEGIES: &[&str] = &["specified", "cost", "latency", "failover"];
const MAX_RESOURCE_PROFILE_NAME_LEN: usize = 100;
const MAX_PLUGIN_NAME_LEN: usize = 255;
const DEFAULT_PLUGIN_VERSION: &str = "0.1.0";
const DEFAULT_RUNTIME_BACKEND: &str = "container";
const DEFAULT_GATEWAY_ROUTING_STRATEGY: &str = "specified";
const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: u32 = 5;
const DEFAULT_CIRCUIT_BREAKER_RESET_MS: u32 = 30_000;

pub(crate) fn configuration_data_response<T: Serialize>(data: T) -> Value {
    serde_json::json!({ "ok": true, "data": data })
}

pub(crate) fn configuration_delete_response() -> Value {
    serde_json::json!({ "ok": true })
}

pub(crate) fn plugin_agent_plugins_response<T: Serialize>(plugins: T) -> Value {
    serde_json::json!({ "ok": true, "plugins": plugins })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSettings {
    pub(crate) default_runtime: String,
    pub(crate) available_runtimes: Vec<String>,
    pub(crate) default_cli_tool: String,
    pub(crate) available_cli_tools: Vec<String>,
}

impl RuntimeSettings {
    pub(crate) fn from_stored(value: Option<&Value>) -> Self {
        let mut settings = Self::default();
        if let Some(value) = value {
            if let Some(default_runtime) = value.get("defaultRuntime").and_then(Value::as_str)
                && let Some(default_runtime) = RuntimeSettingsPolicy::runtime_from_stored(default_runtime)
            {
                settings.default_runtime = default_runtime.to_string();
            }
            if let Some(default_cli_tool) = value.get("defaultCliTool").and_then(Value::as_str)
                && let Some(default_cli_tool) = RuntimeSettingsPolicy::cli_tool_from_stored(default_cli_tool)
            {
                settings.default_cli_tool = default_cli_tool.to_string();
            }
        }
        settings
    }

    pub(crate) fn apply_update(
        &mut self,
        default_runtime: Option<&str>,
        default_cli_tool: Option<&str>,
    ) -> AppResult<()> {
        if let Some(default_runtime) = default_runtime {
            self.default_runtime = RuntimeSettingsPolicy::canonical_runtime(default_runtime)?.to_string();
        }
        if let Some(default_cli_tool) = default_cli_tool {
            self.default_cli_tool = RuntimeSettingsPolicy::canonical_cli_tool(default_cli_tool)?.to_string();
        }
        Ok(())
    }
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            default_runtime: RuntimeSettingsPolicy::default_runtime().to_string(),
            available_runtimes: RuntimeSettingsPolicy::available_runtimes(),
            default_cli_tool: RuntimeSettingsPolicy::default_cli_tool().to_string(),
            available_cli_tools: RuntimeSettingsPolicy::available_cli_tools(),
        }
    }
}

pub(crate) fn runtime_settings_response(runtime: &RuntimeSettings) -> Value {
    serde_json::json!({
        "ok": true,
        "data": runtime,
        // Legacy cached frontends read settings fields from the top-level
        // response instead of the `data` envelope.
        "defaultRuntime": &runtime.default_runtime,
        "availableRuntimes": &runtime.available_runtimes,
        "defaultCliTool": &runtime.default_cli_tool,
        "availableCliTools": &runtime.available_cli_tools,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewaySettings {
    pub(crate) routing_strategy: String,
    pub(crate) circuit_breaker_threshold: u32,
    pub(crate) circuit_breaker_reset_ms: u32,
}

impl GatewaySettings {
    pub(crate) fn from_stored(value: Option<&Value>) -> Self {
        let mut settings = Self::default();
        if let Some(value) = value {
            if let Some(routing_strategy) = value.get("routingStrategy").and_then(Value::as_str)
                && let Some(routing_strategy) = GatewaySettingsPolicy::routing_strategy_from_stored(routing_strategy)
            {
                settings.routing_strategy = routing_strategy.to_string();
            }
            if let Some(threshold) =
                value.get("circuitBreakerThreshold").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok())
            {
                settings.circuit_breaker_threshold = threshold;
            }
            if let Some(reset_ms) =
                value.get("circuitBreakerResetMs").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok())
            {
                settings.circuit_breaker_reset_ms = reset_ms;
            }
        }
        settings
    }

    pub(crate) fn apply_update(
        &mut self,
        routing_strategy: Option<&str>,
        circuit_breaker_threshold: Option<u32>,
        circuit_breaker_reset_ms: Option<u32>,
    ) -> AppResult<()> {
        if let Some(routing_strategy) = routing_strategy {
            self.routing_strategy = GatewaySettingsPolicy::canonical_routing_strategy(routing_strategy)?.to_string();
        }
        if let Some(threshold) = circuit_breaker_threshold {
            self.circuit_breaker_threshold = threshold;
        }
        if let Some(reset_ms) = circuit_breaker_reset_ms {
            self.circuit_breaker_reset_ms = reset_ms;
        }
        Ok(())
    }
}

impl Default for GatewaySettings {
    fn default() -> Self {
        Self {
            routing_strategy: GatewaySettingsPolicy::default_routing_strategy().to_string(),
            circuit_breaker_threshold: GatewaySettingsPolicy::default_circuit_breaker_threshold(),
            circuit_breaker_reset_ms: GatewaySettingsPolicy::default_circuit_breaker_reset_ms(),
        }
    }
}

pub(crate) fn gateway_settings_response(gateway: &GatewaySettings) -> Value {
    serde_json::json!({
        "ok": true,
        "data": gateway,
        // Legacy cached frontends read settings fields from the top-level
        // response instead of the `data` envelope.
        "routingStrategy": &gateway.routing_strategy,
        "circuitBreakerThreshold": gateway.circuit_breaker_threshold,
        "circuitBreakerResetMs": gateway.circuit_breaker_reset_ms,
    })
}

/// Quota resource type tracked by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuotaResourceType<'a> {
    value: &'a str,
}

impl<'a> QuotaResourceType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_QUOTA_RESOURCE_TYPES.contains(&value) {
            return Err(ErrorKind::Validation(format!(
                "resource_type must be one of: {:?}",
                VALID_QUOTA_RESOURCE_TYPES
            ))
            .into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Platform runtime settings policy.
pub(crate) struct RuntimeSettingsPolicy;

impl RuntimeSettingsPolicy {
    pub(crate) fn default_runtime() -> &'static str {
        DEFAULT_RUNTIME_BACKEND
    }

    pub(crate) fn default_cli_tool() -> &'static str {
        CliToolKind::Claude.as_str()
    }

    pub(crate) fn available_runtimes() -> Vec<String> {
        VALID_RUNTIME_BACKENDS.iter().map(|runtime| (*runtime).to_string()).collect()
    }

    pub(crate) fn available_cli_tools() -> Vec<String> {
        CliToolKind::ALL.iter().map(|tool| tool.as_str().to_string()).collect()
    }

    pub(crate) fn canonical_runtime(value: &str) -> AppResult<&'static str> {
        VALID_RUNTIME_BACKENDS
            .iter()
            .copied()
            .find(|runtime| *runtime == value)
            .ok_or_else(|| ErrorKind::Validation(format!("invalid defaultRuntime '{value}'")).into())
    }

    pub(crate) fn canonical_cli_tool(value: &str) -> AppResult<&'static str> {
        CliToolKind::parse_legacy(value)
            .map(CliToolKind::as_str)
            .map_err(|err| ErrorKind::Validation(format!("invalid defaultCliTool '{value}': {err}")).into())
    }

    pub(crate) fn runtime_from_stored(value: &str) -> Option<&'static str> {
        Self::canonical_runtime(value).ok()
    }

    pub(crate) fn cli_tool_from_stored(value: &str) -> Option<&'static str> {
        Self::canonical_cli_tool(value).ok()
    }
}

/// LLM gateway settings policy.
pub(crate) struct GatewaySettingsPolicy;

impl GatewaySettingsPolicy {
    pub(crate) fn default_routing_strategy() -> &'static str {
        DEFAULT_GATEWAY_ROUTING_STRATEGY
    }

    pub(crate) fn default_circuit_breaker_threshold() -> u32 {
        DEFAULT_CIRCUIT_BREAKER_THRESHOLD
    }

    pub(crate) fn default_circuit_breaker_reset_ms() -> u32 {
        DEFAULT_CIRCUIT_BREAKER_RESET_MS
    }

    pub(crate) fn canonical_routing_strategy(value: &str) -> AppResult<&'static str> {
        VALID_GATEWAY_ROUTING_STRATEGIES
            .iter()
            .copied()
            .find(|strategy| *strategy == value)
            .ok_or_else(|| ErrorKind::Validation(format!("invalid routingStrategy '{value}'")).into())
    }

    pub(crate) fn routing_strategy_from_stored(value: &str) -> Option<&'static str> {
        Self::canonical_routing_strategy(value).ok()
    }
}

/// Resource profile policy for container runtime limits.
pub(crate) struct ResourceProfilePolicy;

impl ResourceProfilePolicy {
    pub(crate) fn validate_create(
        name: &str,
        cpu_millicores: i32,
        memory_mb: i32,
        storage_mb: i32,
        max_pids: i32,
    ) -> AppResult<()> {
        Self::validate_name(name)?;
        Self::validate_positive("cpu_millicores", cpu_millicores)?;
        Self::validate_positive("memory_mb", memory_mb)?;
        Self::validate_positive("storage_mb", storage_mb)?;
        Self::validate_positive("max_pids", max_pids)
    }

    pub(crate) fn validate_update(
        name: Option<&str>,
        cpu_millicores: Option<i32>,
        memory_mb: Option<i32>,
        storage_mb: Option<i32>,
        max_pids: Option<i32>,
    ) -> AppResult<()> {
        if let Some(name) = name {
            Self::validate_name(name)?;
        }
        if let Some(value) = cpu_millicores {
            Self::validate_positive("cpu_millicores", value)?;
        }
        if let Some(value) = memory_mb {
            Self::validate_positive("memory_mb", value)?;
        }
        if let Some(value) = storage_mb {
            Self::validate_positive("storage_mb", value)?;
        }
        if let Some(value) = max_pids {
            Self::validate_positive("max_pids", value)?;
        }
        Ok(())
    }

    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > MAX_RESOURCE_PROFILE_NAME_LEN {
            return Err(
                ErrorKind::Validation(format!("name must be 1-{MAX_RESOURCE_PROFILE_NAME_LEN} characters")).into()
            );
        }
        Ok(())
    }

    fn validate_positive(field: &str, value: i32) -> AppResult<()> {
        if value <= 0 {
            return Err(ErrorKind::Validation(format!("{field} must be positive")).into());
        }
        Ok(())
    }
}

/// Dashboard tile type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileType<'a> {
    value: &'a str,
}

impl<'a> TileType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_TILE_TYPES.contains(&value) {
            return Err(ErrorKind::Validation(format!("tile_type must be one of: {:?}", VALID_TILE_TYPES)).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Dashboard tile layout policy.
pub(crate) struct TileLayoutPolicy;

impl TileLayoutPolicy {
    pub(crate) fn validate_dimensions(width: i32, height: i32) -> AppResult<()> {
        if width < 1 || height < 1 {
            return Err(ErrorKind::Validation("width and height must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_width(width: i32) -> AppResult<()> {
        if width < 1 {
            return Err(ErrorKind::Validation("width must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_height(height: i32) -> AppResult<()> {
        if height < 1 {
            return Err(ErrorKind::Validation("height must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_bulk_layout(tiles: &[(Uuid, i32, i32, i32, i32)]) -> AppResult<()> {
        if tiles.is_empty() {
            return Err(ErrorKind::Validation("tiles array must not be empty".into()).into());
        }
        for &(_, _, _, width, height) in tiles {
            Self::validate_dimensions(width, height)?;
        }
        Ok(())
    }
}

/// Plugin catalog display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PluginName<'a> {
    value: &'a str,
}

impl<'a> PluginName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_PLUGIN_NAME_LEN {
            return Err(ErrorKind::Validation("plugin name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Plugin catalog version defaulting policy.
pub(crate) struct PluginVersion<'a> {
    value: &'a str,
}

impl<'a> PluginVersion<'a> {
    pub(crate) fn from_optional(value: Option<&'a str>) -> Self {
        Self { value: value.unwrap_or(DEFAULT_PLUGIN_VERSION) }
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_resource_type_accepts_known_resources() {
        assert_eq!(QuotaResourceType::parse("agents").unwrap().value(), "agents");
        assert_eq!(QuotaResourceType::parse("storage").unwrap().value(), "storage");
        assert_eq!(QuotaResourceType::parse("events").unwrap().value(), "events");
    }

    #[test]
    fn quota_resource_type_rejects_unknown_resources() {
        assert!(QuotaResourceType::parse("cpu").is_err());
        assert!(QuotaResourceType::parse("").is_err());
    }

    #[test]
    fn runtime_settings_policy_exposes_defaults_and_available_values() {
        assert_eq!(RuntimeSettingsPolicy::default_runtime(), "container");
        assert_eq!(RuntimeSettingsPolicy::default_cli_tool(), "claude");
        assert_eq!(RuntimeSettingsPolicy::available_runtimes(), vec!["container".to_string(), "api".to_string()]);
        assert_eq!(
            RuntimeSettingsPolicy::available_cli_tools(),
            vec!["claude".to_string(), "codex".to_string(), "gemini".to_string(), "opencode".to_string()]
        );
    }

    #[test]
    fn runtime_settings_policy_validates_runtime_and_cli_tool() {
        assert_eq!(RuntimeSettingsPolicy::canonical_runtime("api").unwrap(), "api");
        assert_eq!(RuntimeSettingsPolicy::canonical_cli_tool(" CODEX ").unwrap(), "codex");
        assert!(RuntimeSettingsPolicy::canonical_runtime("desktop").is_err());
        assert!(RuntimeSettingsPolicy::canonical_cli_tool("unknown").is_err());
        assert_eq!(RuntimeSettingsPolicy::runtime_from_stored("container"), Some("container"));
        assert_eq!(RuntimeSettingsPolicy::runtime_from_stored("desktop"), None);
        assert_eq!(RuntimeSettingsPolicy::cli_tool_from_stored("Gemini"), Some("gemini"));
        assert_eq!(RuntimeSettingsPolicy::cli_tool_from_stored("unknown"), None);
    }

    #[test]
    fn runtime_settings_defaults_are_frontend_contract() {
        let defaults = RuntimeSettings::default();
        assert_eq!(defaults.default_runtime, "container");
        assert_eq!(defaults.default_cli_tool, "claude");
        assert!(defaults.available_runtimes.contains(&"api".to_string()));
        assert!(defaults.available_cli_tools.contains(&"claude".to_string()));
    }

    #[test]
    fn runtime_settings_from_stored_keeps_valid_overrides() {
        let stored = serde_json::json!({
            "defaultRuntime": "api",
            "defaultCliTool": "CODEX",
            "availableRuntimes": ["ignored"],
        });
        let runtime = RuntimeSettings::from_stored(Some(&stored));

        assert_eq!(runtime.default_runtime, "api");
        assert_eq!(runtime.default_cli_tool, "codex");
        assert_eq!(runtime.available_runtimes, RuntimeSettingsPolicy::available_runtimes());
    }

    #[test]
    fn runtime_settings_from_stored_ignores_invalid_overrides() {
        let stored = serde_json::json!({
            "defaultRuntime": "desktop",
            "defaultCliTool": "unknown",
        });
        let runtime = RuntimeSettings::from_stored(Some(&stored));

        assert_eq!(runtime, RuntimeSettings::default());
    }

    #[test]
    fn runtime_settings_update_validates_values() {
        let mut runtime = RuntimeSettings::default();

        runtime.apply_update(Some("api"), Some("gemini")).unwrap();
        assert_eq!(runtime.default_runtime, "api");
        assert_eq!(runtime.default_cli_tool, "gemini");

        assert!(runtime.apply_update(Some("desktop"), None).is_err());
        assert!(runtime.apply_update(None, Some("unknown")).is_err());
    }

    #[test]
    fn runtime_settings_response_keeps_legacy_top_level_fields() {
        let runtime = RuntimeSettings::default();
        let body = runtime_settings_response(&runtime);

        assert_eq!(body["ok"], true);
        assert_eq!(body["data"]["defaultRuntime"], "container");
        assert_eq!(body["defaultRuntime"], "container");
        assert_eq!(body["availableRuntimes"], body["data"]["availableRuntimes"]);
        assert_eq!(body["availableCliTools"], body["data"]["availableCliTools"]);
    }

    #[test]
    fn gateway_settings_policy_exposes_defaults_and_validates_strategy() {
        assert_eq!(GatewaySettingsPolicy::default_routing_strategy(), "specified");
        assert_eq!(GatewaySettingsPolicy::default_circuit_breaker_threshold(), 5);
        assert_eq!(GatewaySettingsPolicy::default_circuit_breaker_reset_ms(), 30_000);
        assert_eq!(GatewaySettingsPolicy::canonical_routing_strategy("latency").unwrap(), "latency");
        assert_eq!(GatewaySettingsPolicy::routing_strategy_from_stored("failover"), Some("failover"));
        assert_eq!(GatewaySettingsPolicy::routing_strategy_from_stored("random"), None);
        assert!(GatewaySettingsPolicy::canonical_routing_strategy("random").is_err());
    }

    #[test]
    fn gateway_settings_from_stored_keeps_valid_overrides() {
        let stored = serde_json::json!({
            "routingStrategy": "latency",
            "circuitBreakerThreshold": 9,
            "circuitBreakerResetMs": 45_000,
        });
        let gateway = GatewaySettings::from_stored(Some(&stored));

        assert_eq!(gateway.routing_strategy, "latency");
        assert_eq!(gateway.circuit_breaker_threshold, 9);
        assert_eq!(gateway.circuit_breaker_reset_ms, 45_000);
    }

    #[test]
    fn gateway_settings_from_stored_ignores_invalid_strategy() {
        let stored = serde_json::json!({
            "routingStrategy": "random",
            "circuitBreakerThreshold": 7,
        });
        let gateway = GatewaySettings::from_stored(Some(&stored));

        assert_eq!(gateway.routing_strategy, "specified");
        assert_eq!(gateway.circuit_breaker_threshold, 7);
    }

    #[test]
    fn gateway_settings_update_validates_strategy() {
        let mut gateway = GatewaySettings::default();

        gateway.apply_update(Some("failover"), Some(12), Some(60_000)).unwrap();
        assert_eq!(gateway.routing_strategy, "failover");
        assert_eq!(gateway.circuit_breaker_threshold, 12);
        assert_eq!(gateway.circuit_breaker_reset_ms, 60_000);

        assert!(gateway.apply_update(Some("random"), None, None).is_err());
    }

    #[test]
    fn gateway_settings_response_keeps_legacy_top_level_fields() {
        let gateway = GatewaySettings::default();
        let body = gateway_settings_response(&gateway);

        assert_eq!(body["ok"], true);
        assert_eq!(body["data"]["routingStrategy"], "specified");
        assert_eq!(body["routingStrategy"], "specified");
        assert_eq!(body["circuitBreakerThreshold"], body["data"]["circuitBreakerThreshold"]);
        assert_eq!(body["circuitBreakerResetMs"], body["data"]["circuitBreakerResetMs"]);
    }

    #[test]
    fn resource_profile_create_policy_matches_existing_bounds() {
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 2048, 128).is_ok());
        assert!(ResourceProfilePolicy::validate_create("", 1000, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create(&"a".repeat(101), 1000, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 0, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 0, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 0, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 2048, 0).is_err());
    }

    #[test]
    fn resource_profile_update_policy_allows_partial_updates() {
        assert!(ResourceProfilePolicy::validate_update(None, None, None, None, None).is_ok());
        assert!(ResourceProfilePolicy::validate_update(Some("medium"), Some(1000), None, None, None).is_ok());
        assert!(ResourceProfilePolicy::validate_update(Some(""), None, None, None, None).is_err());
        assert!(ResourceProfilePolicy::validate_update(None, Some(-1), None, None, None).is_err());
    }

    #[test]
    fn tile_type_accepts_supported_surfaces() {
        assert_eq!(TileType::parse("agent").unwrap().value(), "agent");
        assert_eq!(TileType::parse("feed").unwrap().value(), "feed");
        assert_eq!(TileType::parse("chart").unwrap().value(), "chart");
        assert_eq!(TileType::parse("custom").unwrap().value(), "custom");
    }

    #[test]
    fn tile_type_rejects_unknown_surfaces() {
        assert!(TileType::parse("widget").is_err());
        assert!(TileType::parse("").is_err());
    }

    #[test]
    fn tile_layout_policy_preserves_dimension_rules() {
        assert!(TileLayoutPolicy::validate_dimensions(1, 1).is_ok());
        assert!(TileLayoutPolicy::validate_dimensions(0, 1).is_err());
        assert!(TileLayoutPolicy::validate_width(0).is_err());
        assert!(TileLayoutPolicy::validate_height(0).is_err());
    }

    #[test]
    fn tile_layout_policy_rejects_empty_bulk_updates() {
        assert!(TileLayoutPolicy::validate_bulk_layout(&[]).is_err());
    }

    #[test]
    fn plugin_name_policy_trims_and_bounds_names() {
        assert_eq!(PluginName::parse(" my-plugin ").unwrap().value(), "my-plugin");
        assert!(PluginName::parse("").is_err());
        assert!(PluginName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn plugin_version_defaults_to_existing_version() {
        assert_eq!(PluginVersion::from_optional(None).value(), "0.1.0");
        assert_eq!(PluginVersion::from_optional(Some("1.2.3")).value(), "1.2.3");
    }
}
