use std::sync::Arc;

use agentforge_orchestrator::config::Config;
use agentforge_orchestrator::state::AppState;
use agentforge_orchestrator::workflow::{
    MemoryStore as MemoryWorkflowStore, WorkflowRuntimeStatus, build_live_workflow_components_with_factory,
    build_workflow_runtime, signal_name_for_node,
};

#[tokio::test]
async fn test_workflow_state_uses_memory_runtime() {
    let state = AppState::test_workflow_internal_token("secret-token", "org-test", "cli-user");
    let service = state.workflow_service.expect("workflow service");
    assert_eq!(service.runtime_kind(), "memory");
}

#[tokio::test]
async fn live_state_without_temporal_keeps_workflow_service_unset() {
    let state = AppState::live(Config::default()).await.expect("live state");
    assert!(state.workflow_service.is_none());
}

// The LOW-LEVEL builder still surfaces a Temporal connect error as `Err`. This is
// the seam the boot path is built on — it does NOT describe boot behavior. The boot
// path no longer fails fast: it classifies this same error as `Unreachable` (see the
// next test). Keep this seam asserting `Err` so the classifier has something to catch.
#[tokio::test]
async fn low_level_builder_surfaces_temporal_connect_errors() {
    let config = Config {
        database_url: ["postgres://postgres:", "postgres", "@localhost/orchestrator"].concat(),
        temporal_enabled: true,
        temporal_host: "bad-host:7233".to_string(),
        mcp_endpoint: "http://localhost:4003/mcp".to_string(),
        mcp_token: "secret-token".to_string(),
        ..Config::default()
    };

    let result = build_live_workflow_components_with_factory(
        &config,
        Some(Arc::new(MemoryWorkflowStore::new())),
        None,
        None,
        |_cfg| async { anyhow::bail!("connect temporal: dial tcp: no such host") },
        |_client, _mcp, _store, _b| unreachable!(),
    )
    .await;

    assert!(result.is_err(), "the low-level builder surfaces connect errors for the classifier to catch");
    let err = result.err().unwrap();
    assert!(err.to_string().contains("connect temporal"));
}

// Boot-path contract: a real Temporal connect failure must DEGRADE (Unreachable),
// not abort. Uses the real connect against an unresolvable host bounded by a 1s
// preflight timeout, so this is the genuine `live_with_runtime` path, not a seam.
#[tokio::test]
async fn boot_path_classifies_temporal_connect_failure_as_unreachable() {
    let config = Config {
        temporal_enabled: true,
        temporal_host: "bad-host:7233".to_string(),
        temporal_connect_timeout_secs: 1,
        mcp_endpoint: "http://localhost:4003/mcp".to_string(),
        mcp_token: "secret-token".to_string(),
        ..Config::default()
    };

    let (components, status) =
        build_workflow_runtime(&config, Some(Arc::new(MemoryWorkflowStore::new())), None, None).await;

    assert!(components.is_none(), "no runtime components when Temporal is unreachable");
    assert_eq!(status, WorkflowRuntimeStatus::Unreachable, "boot must degrade, not abort");
}

#[test]
fn human_review_signal_name_matches_go_contract() {
    assert_eq!(signal_name_for_node("node-42"), "human-review-decision-node-42");
}
