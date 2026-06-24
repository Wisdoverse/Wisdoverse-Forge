use std::fs;

use agent_context_helper::cli_adapter::claude::apply_claude_adapter;
use agent_context_helper::cli_adapter::gemini::apply_gemini_adapter;
use agentforge_core::context_envelope::{
    ContextEnvelope, ContextEnvelopeCapability, ContextEnvelopeItem, ContextEnvelopeItemKind, ContextEnvelopeSource,
    SkillMount,
};
use uuid::Uuid;

fn temp_home() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("agent-context-helper-gemini-test-{}", Uuid::new_v4()));
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
            cli_tool: "gemini".to_string(),
            runtime_kind: "container".to_string(),
            max_context_tokens: 1_000_000,
            supports_skills_mount: true,
            supports_hooks: true,
            supports_subagents: false,
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
            path: "/home/agent/.gemini/skills/project/prod-ext-check".to_string(),
        }],
        degradation: vec!["budget_truncated".to_string()],
    }
}

#[test]
fn gemini_adapter_writes_global_gemini_md_and_redacts_secret_items() {
    let home = temp_home();
    let report = apply_gemini_adapter(&envelope(), &home).expect("apply gemini adapter");

    let gemini_md = fs::read_to_string(home.join(".gemini/GEMINI.md")).expect("read GEMINI.md");
    assert!(gemini_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(gemini_md.contains("These instructions are generated for Gemini CLI"));
    assert!(gemini_md.contains("~/.gemini/GEMINI.md"));
    assert!(gemini_md.contains("## Applied Memory"));
    assert!(gemini_md.contains("Prod deploy rule"));
    assert!(gemini_md.contains("Run make prod-ext after main pipeline succeeds."));
    assert!(gemini_md.contains("[redacted: secret_detected]"));
    assert!(!gemini_md.contains("raw-token-should-not-be-written"));
    assert!(gemini_md.contains("## Mounted Skills"));
    assert!(gemini_md.contains("prod-ext-check v1"));
    assert!(gemini_md.contains("/home/agent/.gemini/skills/project/prod-ext-check"));
    assert!(gemini_md.contains("Degradation: budget_truncated, no_subagents"));
    assert!(gemini_md.contains("<!-- agentforge-context:end -->"));

    assert_eq!(
        fs::read_to_string(home.join(".gemini/state.json")).expect("read state"),
        r#"{"hasCompletedOnboarding":true}"#
    );
    assert_eq!(
        fs::read_to_string(home.join(".gemini/trustedFolders.json")).expect("read trusted folders"),
        r#"{"/workspace":"TRUST_FOLDER","/":"TRUST_PARENT"}"#
    );
    assert_eq!(report.adapter, "gemini");
    assert_eq!(report.applied_items, 2);
    assert!(report.degradation.iter().any(|reason| reason == "budget_truncated"));
    assert!(report.degradation.iter().any(|reason| reason == "no_subagents"));
}

#[test]
fn gemini_adapter_replaces_only_prior_agentforge_block() {
    let home = temp_home();
    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create .gemini");
    fs::write(
        gemini_dir.join("GEMINI.md"),
        "User-owned instructions\n\n<!-- agentforge-context:start old -->\nstale\n<!-- agentforge-context:end -->\n\nKeep me\n",
    )
    .expect("seed GEMINI.md");

    apply_gemini_adapter(&envelope(), &home).expect("apply gemini adapter");
    let gemini_md = fs::read_to_string(gemini_dir.join("GEMINI.md")).expect("read GEMINI.md");

    assert!(gemini_md.contains("User-owned instructions"));
    assert!(gemini_md.contains("Keep me"));
    assert!(gemini_md.contains("Prod deploy rule"));
    assert!(!gemini_md.contains("stale"));
}

#[test]
fn gemini_adapter_preserves_existing_state_and_trust_files() {
    let home = temp_home();
    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create .gemini");
    fs::write(gemini_dir.join("state.json"), r#"{"custom":true}"#).expect("seed state");
    fs::write(gemini_dir.join("trustedFolders.json"), r#"{"/workspace":"TRUST_FOLDER"}"#).expect("seed trust");

    apply_gemini_adapter(&envelope(), &home).expect("apply gemini adapter");

    assert_eq!(fs::read_to_string(gemini_dir.join("state.json")).expect("read state"), r#"{"custom":true}"#);
    assert_eq!(
        fs::read_to_string(gemini_dir.join("trustedFolders.json")).expect("read trust"),
        r#"{"/workspace":"TRUST_FOLDER"}"#
    );
}

#[test]
fn gemini_adapter_writes_minimal_header_for_empty_context() {
    let mut envelope = envelope();
    envelope.applied.clear();
    envelope.skills_mount.clear();
    envelope.degradation.clear();

    let home = temp_home();
    let report = apply_gemini_adapter(&envelope, &home).expect("apply empty gemini adapter");
    let gemini_md = fs::read_to_string(home.join(".gemini/GEMINI.md")).expect("read GEMINI.md");

    assert!(gemini_md.contains("<!-- agentforge-context:start v1 -->"));
    assert!(gemini_md.contains("No approved AgentForge context was applied to this run."));
    assert_eq!(report.applied_items, 0);
    assert_eq!(report.degradation, vec!["no_subagents".to_string()]);
}

#[test]
fn gemini_adapter_matches_claude_reference_semantics() {
    let envelope = envelope();
    let home = temp_home();

    apply_claude_adapter(&envelope, &home).expect("apply claude adapter");
    apply_gemini_adapter(&envelope, &home).expect("apply gemini adapter");

    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    let gemini_md = fs::read_to_string(home.join(".gemini/GEMINI.md")).expect("read GEMINI.md");

    for expected in [
        "AgentForge Runtime Context",
        "Prod deploy rule",
        "Run make prod-ext after main pipeline succeeds.",
        "[redacted: secret_detected]",
        "prod-ext-check v1",
        "Degradation: budget_truncated",
    ] {
        assert!(claude_md.contains(expected), "Claude adapter missing {expected}");
        assert!(gemini_md.contains(expected), "Gemini adapter missing {expected}");
    }

    assert!(gemini_md.contains("no_subagents"));
    assert!(!claude_md.contains("raw-token-should-not-be-written"));
    assert!(!gemini_md.contains("raw-token-should-not-be-written"));
}

#[test]
fn gemini_adapter_fails_closed_when_home_path_is_not_directory() {
    let path = std::env::temp_dir().join(format!("agent-context-helper-gemini-home-file-{}", Uuid::new_v4()));
    fs::write(&path, "not a directory").expect("seed home file");

    let err = apply_gemini_adapter(&envelope(), &path).expect_err("home file must fail");
    let message = err.to_string();
    assert!(message.contains("create") || message.contains("Not a directory"), "unexpected error message: {message}");
}
