use std::fs;

use agent_context_helper::cli_adapter::claude::apply_claude_adapter;
use agent_context_helper::cli_adapter::codex::apply_codex_adapter;
use agentforge_core::context_envelope::{
    ContextEnvelope, ContextEnvelopeCapability, ContextEnvelopeItem, ContextEnvelopeItemKind, ContextEnvelopeSource,
    SkillMount,
};
use uuid::Uuid;

fn temp_home() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("agent-context-helper-codex-test-{}", Uuid::new_v4()));
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
            cli_tool: "codex".to_string(),
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
            path: "/home/agent/.codex/skills/project/prod-ext-check".to_string(),
        }],
        degradation: vec!["budget_truncated".to_string()],
    }
}

#[test]
fn codex_adapter_writes_global_agents_md_and_redacts_secret_items() {
    let home = temp_home();
    let report = apply_codex_adapter(&envelope(), &home).expect("apply codex adapter");

    let agents_md = fs::read_to_string(home.join(".codex/AGENTS.md")).expect("read AGENTS.md");
    assert!(agents_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(agents_md.contains("These instructions are generated for Codex"));
    assert!(agents_md.contains("## Applied Memory"));
    assert!(agents_md.contains("Prod deploy rule"));
    assert!(agents_md.contains("Run make prod-ext after main pipeline succeeds."));
    assert!(agents_md.contains("[redacted: secret_detected]"));
    assert!(!agents_md.contains("raw-token-should-not-be-written"));
    assert!(agents_md.contains("## Mounted Skills"));
    assert!(agents_md.contains("prod-ext-check v1"));
    assert!(agents_md.contains("/home/agent/.codex/skills/project/prod-ext-check"));
    assert!(agents_md.contains("Degradation: budget_truncated"));
    assert!(agents_md.contains("<!-- agentforge-context:end -->"));

    assert_eq!(report.adapter, "codex");
    assert_eq!(report.applied_items, 2);
    assert!(report.degradation.iter().any(|reason| reason == "budget_truncated"));
}

#[test]
fn codex_adapter_replaces_only_prior_agentforge_block() {
    let home = temp_home();
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create .codex");
    fs::write(
        codex_dir.join("AGENTS.md"),
        "User-owned instructions\n\n<!-- agentforge-context:start old -->\nstale\n<!-- agentforge-context:end -->\n\nKeep me\n",
    )
    .expect("seed AGENTS.md");

    apply_codex_adapter(&envelope(), &home).expect("apply codex adapter");
    let agents_md = fs::read_to_string(codex_dir.join("AGENTS.md")).expect("read AGENTS.md");

    assert!(agents_md.contains("User-owned instructions"));
    assert!(agents_md.contains("Keep me"));
    assert!(agents_md.contains("Prod deploy rule"));
    assert!(!agents_md.contains("stale"));
}

#[test]
fn codex_adapter_writes_minimal_header_for_empty_context() {
    let mut envelope = envelope();
    envelope.applied.clear();
    envelope.skills_mount.clear();
    envelope.degradation.clear();

    let home = temp_home();
    let report = apply_codex_adapter(&envelope, &home).expect("apply empty codex adapter");
    let agents_md = fs::read_to_string(home.join(".codex/AGENTS.md")).expect("read AGENTS.md");

    assert!(agents_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(agents_md.contains("No approved AgentForge context was applied to this run."));
    assert_eq!(report.applied_items, 0);
    assert!(report.degradation.is_empty());
}

#[test]
fn codex_adapter_matches_claude_reference_semantics() {
    let envelope = envelope();
    let home = temp_home();

    apply_claude_adapter(&envelope, &home).expect("apply claude adapter");
    apply_codex_adapter(&envelope, &home).expect("apply codex adapter");

    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    let codex_md = fs::read_to_string(home.join(".codex/AGENTS.md")).expect("read AGENTS.md");

    for expected in [
        "AgentForge Runtime Context",
        "Prod deploy rule",
        "Run make prod-ext after main pipeline succeeds.",
        "[redacted: secret_detected]",
        "prod-ext-check v1",
        "Degradation: budget_truncated",
    ] {
        assert!(claude_md.contains(expected), "Claude adapter missing {expected}");
        assert!(codex_md.contains(expected), "Codex adapter missing {expected}");
    }

    assert!(!claude_md.contains("raw-token-should-not-be-written"));
    assert!(!codex_md.contains("raw-token-should-not-be-written"));
}

#[test]
fn codex_adapter_fails_closed_when_home_path_is_not_directory() {
    let path = std::env::temp_dir().join(format!("agent-context-helper-codex-home-file-{}", Uuid::new_v4()));
    fs::write(&path, "not a directory").expect("seed home file");

    let err = apply_codex_adapter(&envelope(), &path).expect_err("home file must fail");
    let message = err.to_string();
    assert!(message.contains("create") || message.contains("Not a directory"), "unexpected error message: {message}");
}
