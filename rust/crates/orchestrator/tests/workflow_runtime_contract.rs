use std::sync::Arc;

use agentforge_orchestrator::config::Config;
use agentforge_orchestrator::state::AppState;
use agentforge_orchestrator::workflow::{
    MemoryStore as MemoryWorkflowStore, build_live_workflow_components_with_factory, signal_name_for_node,
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

#[tokio::test]
async fn live_runtime_builder_bubbles_temporal_connect_errors() {
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
        |_cfg| async { anyhow::bail!("connect temporal: dial tcp: no such host") },
        |_client, _mcp, _store| unreachable!(),
    )
    .await;

    assert!(result.is_err(), "startup should fail fast");
    let err = result.err().unwrap();
    assert!(err.to_string().contains("connect temporal"));
}

#[test]
fn human_review_signal_name_matches_go_contract() {
    assert_eq!(signal_name_for_node("node-42"), "human-review-decision-node-42");
}
