//! Unit 2.6 skeleton coverage for context governance classification and audit.

use agentforge_api::services::context_governance::{
    ContextAuditEvent, ContextGovernanceService, ContextScopeKind, GOVERNANCE_CONTEXT_ACTION_PREFIX,
    MAX_CLASSIFICATION_INPUT_BYTES, ScopeExpansionRejectionReason, ScopeExpansionRequest, Sensitivity,
};
use agentforge_api::test_support::seed_provider_agent;
use agentforge_core::ErrorKind;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn assembled(parts: &[&str]) -> String {
    parts.concat()
}

#[test]
fn classify_sensitivity_defaults_plain_content_to_internal() {
    let content = "Remember that project Atlas uses the standard staging deploy path.";

    let classification = ContextGovernanceService::classify_sensitivity(content);

    assert_eq!(classification.sensitivity, Sensitivity::Internal);
    assert!(classification.matched_patterns.is_empty());
    assert!(classification.redacted_preview.is_none());
}

#[test]
fn classify_sensitivity_detects_secret_corpus() {
    let aws = assembled(&["AK", "IA", "1234567890ABCDEF"]);
    let github_pat = assembled(&["gh", "p_", "1234567890", "abcdefghijklmnopqrstuvwxyz"]);
    let stripe_secret = assembled(&["sk", "_live_", "51N0000000000000000000000"]);
    let hex_token = assembled(&["0123456789abcdef", "0123456789abcdef"]);
    let jwt = assembled(&["eyJhbGciOiJIUzI1NiJ9", ".", "eyJzdWIiOiIxMjMifQ", ".", "Kc2gOQ7M73F2vE2xHqSQJZs0YQkY6Cg"]);
    let private_key_marker = assembled(&["BEGIN ", "PRIVATE KEY"]);
    let gcp_email = assembled(&["runner@forge-prod.", "iam.", "gserviceaccount.com"]);

    let cases = vec![
        ("aws", format!("AWS_ACCESS_KEY_ID={aws}"), aws),
        ("github_pat", format!("token={github_pat}"), github_pat),
        ("stripe_secret", format!("STRIPE_SECRET_KEY={stripe_secret}"), stripe_secret),
        ("hex_token", format!("api token: {hex_token}"), hex_token),
        ("jwt", format!("Authorization: Bearer {jwt}"), "eyJhbGciOiJIUzI1NiJ9".to_string()),
        (
            "google_service_account_json",
            format!(
                r#"{{"type":"service_account","private_key":"-----{}-----\nabc\n-----END PRIVATE KEY-----\n"}}"#,
                private_key_marker
            ),
            private_key_marker,
        ),
        ("gcp_service_account_email", gcp_email.clone(), gcp_email),
    ];

    for (name, content, raw_secret) in cases {
        let classification = ContextGovernanceService::classify_sensitivity(&content);
        assert_eq!(
            classification.sensitivity,
            Sensitivity::SecretDetected,
            "{name} should be classified as secret_detected"
        );
        assert!(!classification.matched_patterns.is_empty(), "{name} should record matched patterns");
        let preview = classification.redacted_preview.expect("secret classification should include redacted preview");
        assert!(!preview.contains(&raw_secret), "{name} preview should not contain raw secret material");
    }
}

#[test]
fn classify_sensitivity_fails_closed_for_extreme_input() {
    let content = "x".repeat(MAX_CLASSIFICATION_INPUT_BYTES + 1);

    assert_eq!(ContextGovernanceService::classify_sensitivity(&content).sensitivity, Sensitivity::SecretDetected);
}

#[test]
fn gate_scope_expansion_requires_confirmation_for_widening() {
    let rejected = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
        from_kind: ContextScopeKind::User,
        to_kind: ContextScopeKind::Team,
        confirm_expansion: false,
    })
    .expect_err("user to team expansion should require confirmation");
    assert_eq!(rejected.reason, ScopeExpansionRejectionReason::ConfirmationRequired);

    let allowed = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
        from_kind: ContextScopeKind::User,
        to_kind: ContextScopeKind::Team,
        confirm_expansion: true,
    })
    .expect("confirmed user to team expansion should pass");
    assert!(allowed.expanded);

    let rejected = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
        from_kind: ContextScopeKind::Team,
        to_kind: ContextScopeKind::Project,
        confirm_expansion: false,
    })
    .expect_err("team to project expansion should require confirmation");
    assert_eq!(rejected.reason, ScopeExpansionRejectionReason::ConfirmationRequired);

    let allowed = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
        from_kind: ContextScopeKind::Project,
        to_kind: ContextScopeKind::Team,
        confirm_expansion: false,
    })
    .expect("narrowing scope should not require confirmation");
    assert!(!allowed.expanded);
}

#[test]
fn gate_scope_expansion_rejects_org_wide_target_in_mvp() {
    for from_kind in [ContextScopeKind::User, ContextScopeKind::Team, ContextScopeKind::Project] {
        let rejected = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
            from_kind,
            to_kind: ContextScopeKind::Org,
            confirm_expansion: true,
        })
        .expect_err("org-wide expansion should be rejected in MVP");
        assert_eq!(rejected.reason, ScopeExpansionRejectionReason::OrgWideUnsupported);
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn emit_audit_commits_with_caller_transaction(pool: PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let mut tx = pool.begin().await.expect("begin tx");

    let event = ContextGovernanceService::emit_audit(
        &mut tx,
        &seed.scope,
        ContextAuditEvent {
            action: "governance.context.memory_item.created",
            resource_type: "memory_item",
            resource_id: Some(Uuid::new_v4()),
            payload: json!({
                "classification": "internal",
                "scope": "workspace"
            }),
            ip_address: None,
        },
    )
    .await
    .expect("emit audit");
    tx.commit().await.expect("commit audit tx");

    assert_eq!(event.organization_id, seed.org_id);
    assert_eq!(event.user_id, Some(seed.user_id));
    assert_eq!(event.action, "governance.context.memory_item.created");
    assert_eq!(event.resource_type, "memory_item");
    assert_eq!(event.details["classification"], "internal");

    let persisted = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log WHERE id = $1")
        .bind(event.id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count persisted event");
    assert_eq!(persisted, 1, "audit event should persist after caller commit");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn emit_audit_rolls_back_with_caller_transaction(pool: PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let mut tx = pool.begin().await.expect("begin tx");

    let event = ContextGovernanceService::emit_audit(
        &mut tx,
        &seed.scope,
        ContextAuditEvent {
            action: "governance.context.memory_item.reclassified",
            resource_type: "memory_item",
            resource_id: Some(Uuid::new_v4()),
            payload: json!({ "classification": "secret_detected" }),
            ip_address: None,
        },
    )
    .await
    .expect("emit audit");
    tx.rollback().await.expect("rollback audit tx");

    let persisted = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log WHERE id = $1")
        .bind(event.id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count rolled back event");
    assert_eq!(persisted, 0, "audit event should roll back with the caller transaction");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn emit_audit_rejects_non_context_governance_event_type(pool: PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let mut tx = pool.begin().await.expect("begin tx");

    let err = ContextGovernanceService::emit_audit(
        &mut tx,
        &seed.scope,
        ContextAuditEvent {
            action: "agent.prompt.created",
            resource_type: "memory_item",
            resource_id: None,
            payload: json!({}),
            ip_address: None,
        },
    )
    .await
    .expect_err("invalid governance audit event type should be rejected");

    assert!(matches!(err.kind, ErrorKind::Validation(message) if message.contains(GOVERNANCE_CONTEXT_ACTION_PREFIX)));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn emit_audit_rejects_raw_secret_payload_keys(pool: PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let mut tx = pool.begin().await.expect("begin tx");
    let github_pat = assembled(&["gh", "p_", "1234567890", "abcdefghijklmnopqrstuvwxyz"]);

    let err = ContextGovernanceService::emit_audit(
        &mut tx,
        &seed.scope,
        ContextAuditEvent {
            action: "governance.context.memory_item.rejected",
            resource_type: "memory_item",
            resource_id: None,
            payload: json!({ "content": format!("token={github_pat}") }),
            ip_address: None,
        },
    )
    .await
    .expect_err("raw content must not enter governance audit details");

    assert!(matches!(err.kind, ErrorKind::Validation(message) if message.contains("secret-bearing audit detail")));
}
