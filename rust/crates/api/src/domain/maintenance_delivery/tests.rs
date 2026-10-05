use super::*;

fn input() -> VerificationInput {
    VerificationInput {
        request_key: Uuid::new_v4(),
        expected_version: 1,
        expected_revision: "a".repeat(40),
        run_id: None,
        criteria: "tests pass".into(),
        scope: "one module".into(),
        environment_notes: "same fixture".into(),
        cli_version: Some("1.0".into()),
        change_summary: "updated module".into(),
        checks: vec![],
        unverified: vec!["production deployment".into()],
        no_artifact_reason: Some("execution not started".into()),
        comparison_key: Some("case-1".into()),
    }
}
fn comparison_row(cli: &str) -> ComparisonRow {
    ComparisonRow {
        report: VerificationReport {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            run_id: Some(Uuid::new_v4()),
            author_id: Uuid::new_v4(),
            task_version: 1,
            revision: "b".repeat(40),
            starting_revision: "a".repeat(40),
            criteria: "tests pass".into(),
            comparison_key: Some("case-1".into()),
            created_at: "2026-10-01T00:00:00Z".into(),
            snapshot: ReportSnapshot {
                title: "task".into(),
                brief: Some("same task".into()),
                repository: "example-org/example-repo".into(),
                revision_kind: "produced".into(),
                scope: "one module".into(),
                environment_notes: "same fixture".into(),
                cli_version: Some("1.0".into()),
                change_summary: "module updated".into(),
                reported_checks: vec![],
                unverified: vec![],
                no_artifact_reason: None,
                runtime: Some(RuntimeEvidence {
                    run_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    state: "completed".into(),
                    started_at: "2026-10-01T00:00:00Z".into(),
                    finished_at: Some("2026-10-01T00:01:00Z".into()),
                    runtime_kind: Some("container".into()),
                    cli_tool: Some(cli.into()),
                    provider_name: None,
                    image: None,
                }),
                evidence: vec![],
                evidence_complete: true,
                github: GithubVerification::absent("not_created"),
            },
        },
        decision: None,
    }
}
#[test]
fn unknown_time_is_not_zero_and_cumulative_totals_do_not_estimate_missing_categories() {
    assert_eq!(HumanMinutes::default().total(), None);
    let all_zero = HumanMinutes {
        setup: Some(0),
        handling: Some(0),
        review: Some(0),
        recovery: Some(0),
        rework: Some(0),
        operation: Some(0),
    };
    assert_eq!(all_zero.total(), Some(0));
    assert_eq!(HumanMinutes { review: Some(17), ..all_zero.clone() }.total(), Some(17));
    assert_eq!(HumanMinutes { review: None, ..all_zero }.total(), None);
}
#[test]
fn report_identity_and_text_limits_reject_ambiguous_or_unpersistable_evidence() {
    assert!(input().prepare().is_ok());
    let mut i = input();
    i.expected_revision = "A".repeat(40);
    assert!(i.prepare().is_err());
    let mut i = input();
    i.request_key = Uuid::nil();
    assert!(i.prepare().is_err());
    let mut i = input();
    i.run_id = Some(Uuid::nil());
    assert!(i.prepare().is_err());
    let mut i = input();
    i.scope = "界".repeat(1334);
    assert!(i.prepare().is_err());
    let mut i = input();
    i.environment_notes = "hidden\0value".into();
    assert!(i.prepare().is_err());
    let mut i = input();
    i.comparison_key = Some("case/other".into());
    assert!(i.prepare().is_err());
}
#[test]
fn comparable_records_require_equivalent_conditions_and_distinct_finished_supported_clis() {
    assert!(compare_reports(vec![comparison_row("codex"), comparison_row("claude")]).conditions_match);
    assert!(!compare_reports(vec![comparison_row("codex"), comparison_row("codex")]).conditions_match);
    let mut second = comparison_row("claude");
    second.report.snapshot.environment_notes = "different fixture".into();
    assert!(!compare_reports(vec![comparison_row("codex"), second]).conditions_match);
    let first = comparison_row("codex");
    let mut second = comparison_row("claude");
    second.report.run_id = first.report.run_id;
    assert!(!compare_reports(vec![first, second]).conditions_match);
    let mut second = comparison_row("claude");
    second.report.snapshot.runtime.as_mut().unwrap().finished_at = None;
    assert!(!compare_reports(vec![comparison_row("codex"), second]).conditions_match);
    let mut second = comparison_row("claude");
    second.report.comparison_key = None;
    assert!(!compare_reports(vec![comparison_row("codex"), second]).conditions_match);
    let mut second = comparison_row("claude");
    second.report.starting_revision = "c".repeat(40);
    assert!(!compare_reports(vec![comparison_row("codex"), second]).conditions_match);
}
#[test]
fn bounded_periods_and_distinct_report_references_guard_large_or_misleading_queries() {
    let now = chrono::Utc::now();
    assert!(OutcomeQuery { from: None, to: None, project_id: None, cursor: None }.prepare(now).is_ok());
    assert!(
        OutcomeQuery { from: Some(now - chrono::Duration::days(121)), to: Some(now), project_id: None, cursor: None }
            .prepare(now)
            .is_err()
    );
    assert!(OutcomeQuery { from: Some(now), to: Some(now), project_id: None, cursor: None }.prepare(now).is_err());
    let id = Uuid::new_v4();
    assert!(ComparisonQuery { report_ids: format!("{id},{id}") }.prepare().is_err());
    assert!(ComparisonQuery { report_ids: format!("{id},{}", Uuid::new_v4()) }.prepare().is_ok());
}
