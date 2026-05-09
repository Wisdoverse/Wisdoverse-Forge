use std::fs;

use agent_context_helper::cli_adapter::claude::apply_claude_adapter;
use agentforge_core::context_envelope::{
    ContextEnvelope, ContextEnvelopeCapability, ContextEnvelopeItem, ContextEnvelopeItemKind, ContextEnvelopeSource,
    SkillMount,
};
use uuid::Uuid;

fn temp_home() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("agent-context-helper-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).expect("create temp home");
    path
}

fn envelope() -> ContextEnvelope {
    ContextEnvelope {
        envelope_version: "v1".to_string(),
        run_id: Uuid::new_v4(),
        task_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        capability: ContextEnvelopeCapability {
            cli_tool: "claude".to_string(),
            runtime_kind: "container".to_string(),
            max_context_tokens: 200_000,
            supports_skills_mount: true,
            supports_hooks: true,
            supports_subagents: true,
        },
        applied: vec![
            ContextEnvelopeItem {
                id: Uuid::new_v4(),
                kind: ContextEnvelopeItemKind::Memory,
                title: "Prod deploy rule".to_string(),
                content: "Run make prod-ext after main pipeline succeeds.".to_string(),
                content_ref: "memory_items/prod-deploy-rule".to_string(),
                sensitivity: "internal".to_string(),
                source: ContextEnvelopeSource {
                    source_type: "task_run".to_string(),
                    source_id: Some(Uuid::new_v4()),
                    title: Some("Previous governed context run".to_string()),
                },
            },
            ContextEnvelopeItem {
                id: Uuid::new_v4(),
                kind: ContextEnvelopeItemKind::Memory,
                title: "Do not expose secret".to_string(),
                content: "raw-token-should-not-be-written".to_string(),
                content_ref: "memory_items/secret".to_string(),
                sensitivity: "secret_detected".to_string(),
                source: ContextEnvelopeSource { source_type: "manual".to_string(), source_id: None, title: None },
            },
        ],
        skills_mount: vec![SkillMount {
            name: "prod-ext-check".to_string(),
            version: 1,
            path: "/home/agent/.agentforge/skills/project/prod-ext-check".to_string(),
        }],
        degradation: vec!["budget_truncated".to_string()],
    }
}

#[test]
fn claude_adapter_writes_tagged_context_and_redacts_secret_items() {
    let home = temp_home();
    let report = apply_claude_adapter(&envelope(), &home).expect("apply claude adapter");

    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    assert!(claude_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(claude_md.contains("## Applied Memory"));
    assert!(claude_md.contains("Prod deploy rule"));
    assert!(claude_md.contains("Run make prod-ext after main pipeline succeeds."));
    assert!(claude_md.contains("[redacted: secret_detected]"));
    assert!(!claude_md.contains("raw-token-should-not-be-written"));
    assert!(claude_md.contains("## Mounted Skills"));
    assert!(claude_md.contains("prod-ext-check v1"));
    assert!(claude_md.contains("Degradation: budget_truncated"));
    assert!(claude_md.contains("<!-- agentforge-context:end -->"));

    assert_eq!(report.adapter, "claude");
    assert_eq!(report.applied_items, 2);
    assert!(report.degradation.iter().any(|reason| reason == "budget_truncated"));
}

#[test]
fn claude_adapter_writes_minimal_header_for_empty_context() {
    let mut envelope = envelope();
    envelope.applied.clear();
    envelope.skills_mount.clear();
    envelope.degradation.clear();

    let home = temp_home();
    let report = apply_claude_adapter(&envelope, &home).expect("apply empty claude adapter");
    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");

    assert!(claude_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(claude_md.contains("No approved AgentForge context was applied to this run."));
    assert_eq!(report.applied_items, 0);
    assert!(report.degradation.is_empty());
}
