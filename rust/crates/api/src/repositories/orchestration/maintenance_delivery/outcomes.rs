use super::*;
use crate::domain::maintenance_delivery::OutcomeWindow;

#[derive(FromRow)]
pub(crate) struct SummaryRow {
    pub submitted: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub rework: i64,
    pub reopened: i64,
    pub awaiting_review: i64,
    pub stale_reviews: i64,
    pub failed: i64,
    pub canceled: i64,
    pub complete_effort_tasks: i64,
    pub human_minutes: Option<i64>,
    pub paired_baseline_tasks: i64,
    pub paired_human_minutes: Option<i64>,
    pub baseline_minutes: Option<i64>,
    pub accepted_in_period: i64,
    pub reopened_in_period: i64,
}

#[derive(FromRow)]
pub(crate) struct OutcomeRow {
    pub task_id: Uuid,
    pub title: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub report_data: Option<Value>,
    pub decision_data: Option<Value>,
    pub review_current: bool,
    pub lead_time_minutes: Option<i64>,
}

// The summary is over the entire submission cohort. The cursor is applied only
// to the detail query, after cohort selection and current-review classification.
const COHORT: &str = r#"WITH cohort AS (
    SELECT t.id AS task_id,t.title,t.status AS state,t.created_at,
        to_jsonb(r) AS report_data,to_jsonb(d) AS decision_data,d.id AS decision_id,d.verdict,
        COALESCE(r.revision=COALESCE(t.pr_head_sha,t.base_commit_sha,m.starting_sha)
            AND r.run_id IS NOT DISTINCT FROM run.id AND r.snapshot->>'title'=t.title
            AND r.snapshot->'brief' IS NOT DISTINCT FROM COALESCE(to_jsonb(t.description),'null'::jsonb),false) AS review_current,
        d.setup_minutes IS NOT NULL AND d.handling_minutes IS NOT NULL AND d.review_minutes IS NOT NULL
            AND d.recovery_minutes IS NOT NULL AND d.rework_minutes IS NOT NULL AND d.operation_minutes IS NOT NULL AS complete_effort,
        (d.setup_minutes::bigint+d.handling_minutes+d.review_minutes+d.recovery_minutes+d.rework_minutes+d.operation_minutes) AS minutes,
        d.baseline_minutes,
        GREATEST(0,FLOOR(EXTRACT(EPOCH FROM (d.created_at-t.created_at))/60))::bigint AS lead_time_minutes
    FROM maintenance_requests m
    JOIN orchestration_tasks t ON t.id=m.task_id AND t.organization_id=m.organization_id
    LEFT JOIN groups g ON g.id=t.group_id AND g.organization_id=t.organization_id
    LEFT JOIN LATERAL (SELECT * FROM maintenance_verification_reports WHERE organization_id=m.organization_id AND task_id=t.id ORDER BY created_at DESC,id DESC LIMIT 1) r ON true
    LEFT JOIN LATERAL (SELECT * FROM maintenance_review_decisions WHERE organization_id=m.organization_id AND task_id=t.id AND report_id=r.id ORDER BY created_at DESC,id DESC LIMIT 1) d ON true
    LEFT JOIN LATERAL (SELECT id FROM task_runs WHERE organization_id=m.organization_id AND orchestration_task_id=t.id ORDER BY started_at DESC,created_at DESC,id DESC LIMIT 1) run ON true
    WHERE m.organization_id=$1 AND m.created_at >= $2 AND m.created_at < $3
        AND ($4::uuid IS NULL OR g.project_id=$4)
) "#;

impl MaintenanceDeliveryRepository {
    pub(crate) async fn read_snapshot_in_tx(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut **tx).await?;
        Ok(())
    }
    pub(crate) async fn outcome_summary_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        window: &OutcomeWindow,
    ) -> AppResult<SummaryRow> {
        let sql = format!(
            r#"{COHORT} SELECT COUNT(*) AS submitted,
            COUNT(*) FILTER (WHERE review_current AND verdict='accepted') AS accepted,
            COUNT(*) FILTER (WHERE review_current AND verdict='rejected') AS rejected,
            COUNT(*) FILTER (WHERE review_current AND verdict='rework') AS rework,
            COUNT(*) FILTER (WHERE review_current AND verdict='reopened') AS reopened,
            COUNT(*) FILTER (WHERE decision_id IS NULL) AS awaiting_review,
            COUNT(*) FILTER (WHERE decision_id IS NOT NULL AND NOT review_current) AS stale_reviews,
            COUNT(*) FILTER (WHERE state='failed') AS failed,
            COUNT(*) FILTER (WHERE state='canceled') AS canceled,
            COUNT(*) FILTER (WHERE review_current AND complete_effort) AS complete_effort_tasks,
            (SUM(minutes) FILTER (WHERE review_current AND complete_effort))::bigint AS human_minutes,
            COUNT(*) FILTER (WHERE review_current AND complete_effort AND baseline_minutes IS NOT NULL) AS paired_baseline_tasks,
            (SUM(minutes) FILTER (WHERE review_current AND complete_effort AND baseline_minutes IS NOT NULL))::bigint AS paired_human_minutes,
            (SUM(baseline_minutes) FILTER (WHERE review_current AND complete_effort AND baseline_minutes IS NOT NULL))::bigint AS baseline_minutes,
            (SELECT COUNT(DISTINCT d.task_id) FROM maintenance_review_decisions d
                JOIN maintenance_requests m ON m.task_id=d.task_id AND m.organization_id=d.organization_id
                JOIN orchestration_tasks t ON t.id=d.task_id AND t.organization_id=d.organization_id
                LEFT JOIN groups g ON g.id=t.group_id AND g.organization_id=t.organization_id
                WHERE d.organization_id=$1 AND d.created_at >= $2 AND d.created_at < $3 AND d.verdict='accepted' AND ($4::uuid IS NULL OR g.project_id=$4)) AS accepted_in_period,
            (SELECT COUNT(DISTINCT d.task_id) FROM maintenance_review_decisions d
                JOIN maintenance_requests m ON m.task_id=d.task_id AND m.organization_id=d.organization_id
                JOIN orchestration_tasks t ON t.id=d.task_id AND t.organization_id=d.organization_id
                LEFT JOIN groups g ON g.id=t.group_id AND g.organization_id=t.organization_id
                WHERE d.organization_id=$1 AND d.created_at >= $2 AND d.created_at < $3 AND d.verdict='reopened' AND ($4::uuid IS NULL OR g.project_id=$4)) AS reopened_in_period
            FROM cohort"#
        );
        Ok(sqlx::query_as(&sql)
            .bind(scope.org_id().as_uuid())
            .bind(window.from)
            .bind(window.to)
            .bind(window.project_id)
            .fetch_one(&mut **tx)
            .await?)
    }

    pub(crate) async fn outcome_rows_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        window: &OutcomeWindow,
    ) -> AppResult<Vec<OutcomeRow>> {
        let sql = format!(
            "{COHORT} SELECT task_id,title,state,created_at,report_data,decision_data,review_current,lead_time_minutes FROM cohort WHERE ($5::uuid IS NULL OR task_id>$5) ORDER BY task_id LIMIT 101"
        );
        Ok(sqlx::query_as(&sql)
            .bind(scope.org_id().as_uuid())
            .bind(window.from)
            .bind(window.to)
            .bind(window.project_id)
            .bind(window.cursor)
            .fetch_all(&mut **tx)
            .await?)
    }
}
