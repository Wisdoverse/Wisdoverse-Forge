use agentforge_core::{CliToolKind, RuntimeCapability, RuntimeKind};
use sqlx::PgPool;

#[test]
fn cli_tool_kind_serializes_and_deserializes_all_known_variants() {
    for tool in CliToolKind::ALL {
        let json = serde_json::to_string(&tool).expect("serialize cli tool");
        assert_eq!(json, format!("\"{}\"", tool.as_str()));

        let decoded: CliToolKind = serde_json::from_str(&json).expect("deserialize cli tool");
        assert_eq!(decoded, tool);
    }
}

#[test]
fn zero_context_window_is_rejected_for_api_runtime() {
    let err = RuntimeCapability::new(None, RuntimeKind::Api, 0, false, false, false, false, false, false)
        .expect_err("zero-token API runtime should be rejected");

    assert_eq!(err.to_string(), "max_context_tokens must be greater than zero for api runtime");
}

#[test]
fn container_cli_image_input_matches_tool_vision_support() {
    // Claude Code, Codex, and Gemini CLIs accept local image file paths; opencode
    // has no documented image support, so it stays off until verified.
    assert!(RuntimeCapability::for_cli_tool(CliToolKind::Claude, RuntimeKind::Container).supports_image_input);
    assert!(RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container).supports_image_input);
    assert!(RuntimeCapability::for_cli_tool(CliToolKind::Gemini, RuntimeKind::Container).supports_image_input);
    assert!(!RuntimeCapability::for_cli_tool(CliToolKind::Opencode, RuntimeKind::Container).supports_image_input);
}

#[test]
fn host_cli_runtime_never_supports_image_input() {
    // Host-CLI agents run on the operator's own host with no shared /workspace
    // mount, so file-based image delivery is impossible regardless of tool.
    for tool in CliToolKind::ALL {
        assert!(
            !RuntimeCapability::for_cli_tool(tool, RuntimeKind::Cli).supports_image_input,
            "host-cli {tool} must not advertise image input"
        );
    }
}

#[test]
fn fallback_and_api_default_disable_image_input() {
    assert!(
        !RuntimeCapability::fallback_for_cli_tool(CliToolKind::Claude, RuntimeKind::Container).supports_image_input
    );
    assert!(!RuntimeCapability::api_default("anthropic").supports_image_input);
}

#[test]
fn with_image_input_enables_the_flag() {
    let cap = RuntimeCapability::api_default("openai").with_image_input(true);
    assert!(cap.supports_image_input);
}

#[test]
fn legacy_capability_profile_without_image_field_defaults_to_false() {
    // Capability_profile JSONB persisted before this feature predates
    // supports_image_input; it must deserialize to false so existing rows decode
    // instead of aborting startup.
    let legacy = serde_json::json!({
        "runtime_kind": "container",
        "max_context_tokens": 200_000,
        "supports_skills_mount": true,
        "supports_hooks": true,
        "supports_subagents": true,
        "supports_mcp_bridge": true,
        "supports_terminal": true
    });
    let profile: RuntimeCapability = serde_json::from_value(legacy).expect("legacy profile decodes");
    assert!(!profile.supports_image_input);
}

#[test]
fn same_cli_tool_differs_between_container_and_local_cli_runtime() {
    let container = RuntimeCapability::for_cli_tool(CliToolKind::Claude, RuntimeKind::Container);
    let local_cli = RuntimeCapability::for_cli_tool(CliToolKind::Claude, RuntimeKind::Cli);

    assert_eq!(container.cli_tool, Some(CliToolKind::Claude));
    assert_eq!(local_cli.cli_tool, Some(CliToolKind::Claude));
    assert_ne!(container.supports_mcp_bridge, local_cli.supports_mcp_bridge);
    assert_ne!(container.supports_terminal, local_cli.supports_terminal);
}

#[test]
fn runtime_capability_all_covers_container_and_cli_matrix() {
    let profiles = RuntimeCapability::all();

    assert_eq!(profiles.len(), CliToolKind::ALL.len() * 2);
    for tool in CliToolKind::ALL {
        assert!(profiles.contains(&RuntimeCapability::for_cli_tool(tool, RuntimeKind::Container)));
        assert!(profiles.contains(&RuntimeCapability::for_cli_tool(tool, RuntimeKind::Cli)));
    }
    assert!(!profiles.iter().any(|profile| profile.runtime_kind == RuntimeKind::Api));
}

#[test]
fn legacy_cli_tool_string_parses_to_canonical_variant() {
    let parsed = CliToolKind::parse_legacy(" Codex ").expect("legacy cli tool should normalize");

    assert_eq!(parsed, CliToolKind::Codex);
    assert_eq!(parsed.as_str(), "codex");
}

#[test]
fn malformed_legacy_cli_tool_returns_controlled_error() {
    let err = CliToolKind::parse_legacy("vim").expect_err("unknown cli tool should fail");

    assert_eq!(err.to_string(), "unsupported cli_tool: vim (expected claude|codex|gemini|opencode)");
    assert_eq!(err.status_label(), "cli_tool_unknown");
}

#[test]
fn legacy_runtime_kind_string_parses_to_canonical_variant() {
    let parsed = RuntimeKind::parse_legacy(" Container ").expect("legacy runtime kind should normalize");

    assert_eq!(parsed, RuntimeKind::Container);
    assert_eq!(parsed.as_str(), "container");
}

#[test]
fn parse_legacy_rejects_invented_aliases() {
    // rev1 spec invented these; rev2 forbids them.
    assert!(RuntimeKind::parse_legacy("host_cli").is_err());
    assert!(RuntimeKind::parse_legacy("host-cli").is_err());
    assert!(RuntimeKind::parse_legacy("provider").is_err());
    assert!(RuntimeKind::parse_legacy("container-cli").is_err());
}

#[test]
fn parse_legacy_accepts_canonical_only() {
    assert_eq!(RuntimeKind::parse_legacy("container").unwrap(), RuntimeKind::Container);
    assert_eq!(RuntimeKind::parse_legacy("cli").unwrap(), RuntimeKind::Cli);
    assert_eq!(RuntimeKind::parse_legacy("api").unwrap(), RuntimeKind::Api);
    assert_eq!(RuntimeKind::parse_legacy(" CLI ").unwrap(), RuntimeKind::Cli);
}

#[sqlx::test(migrations = false)]
async fn runtime_kind_sqlx_roundtrip(pool: PgPool) -> sqlx::Result<()> {
    // `sqlx::test` provisions a fresh isolated database per test, so a regular
    // `CREATE TABLE` is safe and avoids the per-connection scope of
    // `TEMP TABLE` (the pool may hand out different connections).
    sqlx::query("CREATE TABLE tmp_runtime_kind (id INT PRIMARY KEY, rk TEXT NOT NULL)").execute(&pool).await?;

    for &kind in &[RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
        sqlx::query("INSERT INTO tmp_runtime_kind (id, rk) VALUES ($1, $2)")
            .bind(kind as i32)
            .bind(kind)
            .execute(&pool)
            .await?;

        let row: (i32, RuntimeKind) = sqlx::query_as("SELECT id, rk FROM tmp_runtime_kind WHERE id = $1")
            .bind(kind as i32)
            .fetch_one(&pool)
            .await?;

        assert_eq!(row.1, kind);
    }
    Ok(())
}
