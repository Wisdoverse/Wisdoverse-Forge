use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

use super::errors::{Result, WorkflowError};
use super::model::{MissingMark, NodeHistory, NodeStatus, NodeType, Workflow, WorkflowNode, WorkflowStatus};
use super::store::Store;

struct WorkflowRecord {
    workflow: Workflow,
    nodes: Vec<WorkflowNode>,
}

pub struct MemoryStore {
    workflow_seq: AtomicU64,
    node_seq: AtomicU64,
    records: Mutex<HashMap<String, WorkflowRecord>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { workflow_seq: AtomicU64::new(1), node_seq: AtomicU64::new(1), records: Mutex::new(HashMap::new()) }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, workflow: &mut Workflow, nodes: &mut Vec<WorkflowNode>) -> Result<()> {
        let now = Utc::now();
        workflow.id = format!("workflow-{}", self.workflow_seq.fetch_add(1, Ordering::Relaxed));
        workflow.created_at = now;
        workflow.updated_at = now;

        for (index, node) in nodes.iter_mut().enumerate() {
            node.id = format!("node-{}", self.node_seq.fetch_add(1, Ordering::Relaxed));
            node.workflow_id = workflow.id.clone();
            node.position = index as i32;
            node.status = NodeStatus::Pending;
            node.started_at = None;
            node.completed_at = None;
            node.error = None;
            node.output = None;
        }

        self.records
            .lock()
            .await
            .insert(workflow.id.clone(), WorkflowRecord { workflow: workflow.clone(), nodes: nodes.clone() });
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Workflow> {
        self.records
            .lock()
            .await
            .get(id)
            .filter(|record| record.workflow.org_id == org_id)
            .map(|record| record.workflow.clone())
            .ok_or(WorkflowError::NotFound)
    }

    async fn get_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNode>> {
        self.records.lock().await.get(workflow_id).map(|record| record.nodes.clone()).ok_or(WorkflowError::NotFound)
    }

    async fn list(&self, org_id: &str, limit: usize, offset: usize) -> Result<Vec<Workflow>> {
        let mut workflows: Vec<Workflow> = self
            .records
            .lock()
            .await
            .values()
            .filter(|record| record.workflow.org_id == org_id)
            .map(|record| record.workflow.clone())
            .collect();
        workflows.sort_by_key(|workflow| std::cmp::Reverse(workflow.created_at));
        Ok(workflows.into_iter().skip(offset).take(limit.max(1)).collect())
    }

    async fn update_status(&self, id: &str, org_id: &str, status: WorkflowStatus) -> Result<()> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(id).filter(|record| record.workflow.org_id == org_id) else {
            return Err(WorkflowError::NotFound);
        };
        set_status(&mut record.workflow, status);
        Ok(())
    }

    async fn set_temporal_ids(
        &self,
        id: &str,
        org_id: &str,
        workflow_id: String,
        run_id: String,
        namespace_id: Option<String>,
    ) -> Result<()> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(id).filter(|record| record.workflow.org_id == org_id) else {
            return Err(WorkflowError::NotFound);
        };
        let workflow = &mut record.workflow;
        workflow.temporal_workflow_id = Some(workflow_id);
        workflow.temporal_run_id = Some(run_id);
        workflow.temporal_namespace_id = namespace_id;
        workflow.status_cause = None;
        workflow.execution_missing_since = None;
        workflow.execution_checked_at = None;
        workflow.execution_observation = None;
        workflow.reconciled_at = None;
        touch(workflow);
        Ok(())
    }

    async fn update_status_for_run(
        &self,
        id: &str,
        org_id: &str,
        run_id: &str,
        status: WorkflowStatus,
    ) -> Result<bool> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(id).filter(|record| record.workflow.org_id == org_id) else {
            return Err(WorkflowError::NotFound);
        };
        if record.workflow.temporal_run_id.as_deref() != Some(run_id) {
            return Ok(false);
        }
        set_status(&mut record.workflow, status);
        Ok(true)
    }

    async fn reconcile_candidates(&self, limit: usize) -> Result<Vec<Workflow>> {
        let mut candidates: Vec<Workflow> = self
            .records
            .lock()
            .await
            .values()
            .filter(|record| {
                record.workflow.status == WorkflowStatus::Running && record.workflow.temporal_workflow_id.is_some()
            })
            .map(|record| record.workflow.clone())
            .collect();
        // `None` sorts first, matching `NULLS FIRST` in the Postgres store.
        candidates.sort_by(|a, b| {
            (a.execution_checked_at, a.updated_at, &a.id).cmp(&(b.execution_checked_at, b.updated_at, &b.id))
        });
        candidates.truncate(limit);
        Ok(candidates)
    }

    async fn record_execution_check(&self, workflow: &Workflow, mark: MissingMark, observation: &str) -> Result<bool> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(&workflow.id).filter(|record| unchanged_since(&record.workflow, workflow))
        else {
            return Ok(false);
        };
        // Bookkeeping only: like the database trigger, no version bump.
        let now = Utc::now();
        record.workflow.execution_checked_at = Some(now);
        record.workflow.execution_observation = Some(observation.to_string());
        match mark {
            MissingMark::Keep => {}
            MissingMark::Clear => record.workflow.execution_missing_since = None,
            MissingMark::Set => {
                record.workflow.execution_missing_since.get_or_insert(now);
            }
        }
        Ok(true)
    }

    async fn mark_execution_missing(&self, workflow: &Workflow) -> Result<bool> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(&workflow.id).filter(|record| {
            unchanged_since(&record.workflow, workflow) && record.workflow.execution_missing_since.is_none()
        }) else {
            return Ok(false);
        };
        record.workflow.execution_missing_since = Some(Utc::now());
        Ok(true)
    }

    async fn set_status_if_unchanged(&self, workflow: &Workflow, status: WorkflowStatus) -> Result<bool> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(&workflow.id).filter(|record| same_execution(&record.workflow, workflow))
        else {
            return Ok(false);
        };
        set_status(&mut record.workflow, status);
        Ok(true)
    }

    async fn reconcile_status(
        &self,
        workflow: &Workflow,
        status: WorkflowStatus,
        cause: &str,
        missing_for_secs: Option<u64>,
    ) -> Result<bool> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(&workflow.id).filter(|record| unchanged_since(&record.workflow, workflow))
        else {
            return Ok(false);
        };
        let now = Utc::now();
        if let Some(secs) = missing_for_secs {
            let long_enough = record
                .workflow
                .execution_missing_since
                .is_some_and(|since| since <= now - chrono::Duration::seconds(secs as i64));
            if !long_enough {
                return Ok(false);
            }
        }
        record.workflow.status = status;
        record.workflow.status_cause = Some(cause.to_string());
        record.workflow.reconciled_at = Some(now);
        record.workflow.execution_checked_at = Some(now);
        record.workflow.execution_observation = Some(observation_for_cause(cause).to_string());
        touch(&mut record.workflow);
        Ok(true)
    }

    async fn update_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        err_msg: Option<String>,
        output: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut records = self.records.lock().await;
        let now = Utc::now();
        for record in records.values_mut() {
            if let Some(node) = record.nodes.iter_mut().find(|node| node.id == node_id) {
                node.status = status;
                if status == NodeStatus::Running && node.started_at.is_none() {
                    node.started_at = Some(now);
                }
                if matches!(status, NodeStatus::Completed | NodeStatus::Failed | NodeStatus::Skipped) {
                    if node.started_at.is_none() {
                        node.started_at = Some(now);
                    }
                    node.completed_at = Some(now);
                }
                if let Some(err_msg) = err_msg {
                    node.error = Some(err_msg);
                }
                if let Some(output) = output {
                    node.output = Some(output);
                }
                record.workflow.updated_at = now;
                return Ok(());
            }
        }
        Err(WorkflowError::NotFound)
    }

    async fn reset_nodes(&self, workflow_id: &str, org_id: &str) -> Result<()> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(workflow_id).filter(|r| r.workflow.org_id == org_id) else {
            return Ok(());
        };
        for node in record.nodes.iter_mut() {
            node.status = NodeStatus::Pending;
            node.started_at = None;
            node.completed_at = None;
            node.error = None;
            node.output = None;
        }
        record.workflow.updated_at = Utc::now();
        Ok(())
    }

    async fn history(&self, workflow_id: &str) -> Result<Vec<NodeHistory>> {
        let nodes = self.get_nodes(workflow_id).await?;
        Ok(nodes
            .into_iter()
            .map(|node| NodeHistory {
                node_id: node.id,
                node_name: node.name,
                node_type: node.node_type,
                status: node.status,
                started_at: node.started_at,
                completed_at: node.completed_at,
                error: node.error,
            })
            .collect())
    }
}

/// A normal status change: the new status is the workflow's own, so any
/// reconciliation cause or missing mark no longer describes it.
fn set_status(workflow: &mut Workflow, status: WorkflowStatus) {
    workflow.status = status;
    workflow.status_cause = None;
    workflow.execution_missing_since = None;
    touch(workflow);
}

/// What the `workflows_track_change` trigger does for a real change.
fn touch(workflow: &mut Workflow) {
    workflow.updated_at = Utc::now();
    workflow.state_version += 1;
}

/// Same organization, execution ids and `state_version` as when read.
fn same_execution(current: &Workflow, observed: &Workflow) -> bool {
    current.org_id == observed.org_id
        && current.temporal_workflow_id == observed.temporal_workflow_id
        && current.temporal_run_id == observed.temporal_run_id
        && current.state_version == observed.state_version
}

/// The compare-and-set every reconciliation write uses (#1352).
fn unchanged_since(current: &Workflow, observed: &Workflow) -> bool {
    current.status == WorkflowStatus::Running && same_execution(current, observed)
}

/// Observation stored with a reconciled status.
fn observation_for_cause(cause: &str) -> &'static str {
    if cause == super::model::CAUSE_RECONCILED_FROM_TEMPORAL { "closed" } else { "missing" }
}

/// Columns read by [`row_to_workflow`].
macro_rules! workflow_columns {
    () => {
        "id::text AS id, name, description, status, org_id, created_by::text AS created_by, \
         temporal_workflow_id, temporal_run_id, temporal_namespace_id, status_cause, execution_missing_since, \
         execution_checked_at, execution_observation, reconciled_at, state_version, created_at, updated_at"
    };
}

/// Same organization, execution ids and `state_version` as when read (#1352).
/// Binds `$1`..`$5`.
macro_rules! same_execution_sql {
    () => {
        "id = CAST($1 AS uuid) AND org_id = $2 AND temporal_workflow_id = $3 \
         AND temporal_run_id IS NOT DISTINCT FROM $4 AND state_version = $5"
    };
}

pub(crate) const RECONCILE_CANDIDATES_SQL: &str = concat!(
    "SELECT ",
    workflow_columns!(),
    " FROM workflows WHERE status = 'running' AND temporal_workflow_id IS NOT NULL \
     ORDER BY execution_checked_at NULLS FIRST, updated_at, id LIMIT $1"
);

/// Bookkeeping only: the trigger leaves `updated_at` and `state_version` alone.
pub(crate) const RECORD_EXECUTION_CHECK_SQL: &str = concat!(
    "UPDATE workflows SET execution_checked_at = NOW(), execution_observation = $7, \
     execution_missing_since = CASE $6 WHEN 'set' THEN COALESCE(execution_missing_since, NOW()) \
     WHEN 'clear' THEN NULL ELSE execution_missing_since END WHERE status = 'running' AND ",
    same_execution_sql!()
);

/// Bookkeeping only, and only when no mark is set yet.
pub(crate) const MARK_EXECUTION_MISSING_SQL: &str = concat!(
    "UPDATE workflows SET execution_missing_since = NOW() \
     WHERE status = 'running' AND execution_missing_since IS NULL AND ",
    same_execution_sql!()
);

pub(crate) const SET_STATUS_IF_UNCHANGED_SQL: &str = concat!(
    "UPDATE workflows SET status = $6, status_cause = NULL, execution_missing_since = NULL WHERE ",
    same_execution_sql!()
);

pub(crate) const RECONCILE_STATUS_SQL: &str = concat!(
    "UPDATE workflows SET status = $6, status_cause = $7, reconciled_at = NOW(), execution_checked_at = NOW(), \
     execution_observation = $9 WHERE status = 'running' AND ",
    same_execution_sql!(),
    " AND ($8::float8 IS NULL OR execution_missing_since <= NOW() - make_interval(secs => $8::float8))"
);

pub struct PgWorkflowStore {
    pool: PgPool,
}

impl PgWorkflowStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgWorkflowStore {
    async fn create(&self, workflow: &mut Workflow, nodes: &mut Vec<WorkflowNode>) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| WorkflowError::Internal(format!("begin workflow transaction: {err}")))?;

        let row = sqlx::query(
            "INSERT INTO workflows (name, description, status, org_id, created_by)              VALUES ($1, $2, $3, $4, CAST($5 AS uuid))              RETURNING id::text AS id, created_at, updated_at"
        )
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(workflow.status.as_str())
        .bind(&workflow.org_id)
        .bind(&workflow.created_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| WorkflowError::Internal(format!("insert workflow: {err}")))?;

        workflow.id = row.try_get("id").map_err(|err| WorkflowError::Internal(format!("read workflow id: {err}")))?;
        workflow.created_at = row
            .try_get("created_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow created_at: {err}")))?;
        workflow.updated_at = row
            .try_get("updated_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow updated_at: {err}")))?;

        let mut name_to_id = HashMap::new();
        for (index, node) in nodes.iter_mut().enumerate() {
            let config = node.config.clone().unwrap_or_else(|| serde_json::json!({}));
            let row = sqlx::query(
                "INSERT INTO workflow_nodes (workflow_id, name, type, config, position, status)                  VALUES (CAST($1 AS uuid), $2, $3, $4, $5, $6)                  RETURNING id::text AS id"
            )
            .bind(&workflow.id)
            .bind(&node.name)
            .bind(node.node_type.as_str())
            .bind(config)
            .bind(index as i32)
            .bind(NodeStatus::Pending.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(|err| WorkflowError::Internal(format!("insert workflow node: {err}")))?;
            node.id =
                row.try_get("id").map_err(|err| WorkflowError::Internal(format!("read workflow node id: {err}")))?;
            node.workflow_id = workflow.id.clone();
            node.position = index as i32;
            node.status = NodeStatus::Pending;
            node.started_at = None;
            node.completed_at = None;
            node.error = None;
            node.output = None;
            name_to_id.insert(node.name.clone(), node.id.clone());
        }

        for node in nodes.iter() {
            for dep_name in &node.depends_on {
                let dep_id = name_to_id.get(dep_name).ok_or_else(|| {
                    WorkflowError::InvalidInput(format!(
                        "dependency \"{dep_name}\" not found for node \"{}\"",
                        node.name
                    ))
                })?;
                sqlx::query(
                    "INSERT INTO workflow_node_dependencies (node_id, depends_on) VALUES (CAST($1 AS uuid), CAST($2 AS uuid))"
                )
                .bind(&node.id)
                .bind(dep_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| WorkflowError::Internal(format!("insert workflow dependency: {err}")))?;
            }
        }

        tx.commit().await.map_err(|err| WorkflowError::Internal(format!("commit workflow transaction: {err}")))?;
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Workflow> {
        let row = sqlx::query(concat!(
            "SELECT ",
            workflow_columns!(),
            " FROM workflows WHERE id = CAST($1 AS uuid) AND org_id = $2"
        ))
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("get workflow: {err}")))?
        .ok_or(WorkflowError::NotFound)?;
        row_to_workflow(&row)
    }

    async fn get_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNode>> {
        let rows = sqlx::query(
            "SELECT id::text AS id, workflow_id::text AS workflow_id, name, type, config, position, status, started_at, completed_at, error, output              FROM workflow_nodes WHERE workflow_id = CAST($1 AS uuid) ORDER BY position"
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("get workflow nodes: {err}")))?;

        let mut nodes = rows.iter().map(row_to_node).collect::<Result<Vec<_>>>()?;
        let dep_rows = sqlx::query(
            "SELECT d.node_id::text AS node_id, n.name AS depends_on_name              FROM workflow_node_dependencies d              JOIN workflow_nodes n ON n.id = d.depends_on              WHERE d.node_id = ANY(SELECT id FROM workflow_nodes WHERE workflow_id = CAST($1 AS uuid))"
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("get workflow dependencies: {err}")))?;
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        for row in dep_rows {
            let node_id: String =
                row.try_get("node_id").map_err(|err| WorkflowError::Internal(format!("read node_id: {err}")))?;
            let dep_name: String = row
                .try_get("depends_on_name")
                .map_err(|err| WorkflowError::Internal(format!("read depends_on_name: {err}")))?;
            deps.entry(node_id).or_default().push(dep_name);
        }
        for node in &mut nodes {
            node.depends_on = deps.remove(&node.id).unwrap_or_default();
        }
        Ok(nodes)
    }

    async fn list(&self, org_id: &str, limit: usize, offset: usize) -> Result<Vec<Workflow>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            workflow_columns!(),
            " FROM workflows WHERE org_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ))
        .bind(org_id)
        .bind(limit.max(1) as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("list workflows: {err}")))?;
        rows.iter().map(row_to_workflow).collect()
    }

    async fn update_status(&self, id: &str, org_id: &str, status: WorkflowStatus) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workflows SET status = $1, status_cause = NULL, execution_missing_since = NULL, updated_at = NOW() \
             WHERE id = CAST($2 AS uuid) AND org_id = $3",
        )
        .bind(status.as_str())
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("update workflow status: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(WorkflowError::NotFound);
        }
        Ok(())
    }

    async fn set_temporal_ids(
        &self,
        id: &str,
        org_id: &str,
        workflow_id: String,
        run_id: String,
        namespace_id: Option<String>,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workflows SET temporal_workflow_id = $1, temporal_run_id = $2, temporal_namespace_id = $3, \
             status_cause = NULL, execution_missing_since = NULL, execution_checked_at = NULL, \
             execution_observation = NULL, reconciled_at = NULL, updated_at = NOW() \
             WHERE id = CAST($4 AS uuid) AND org_id = $5",
        )
        .bind(workflow_id)
        .bind(run_id)
        .bind(namespace_id)
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("set temporal ids: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(WorkflowError::NotFound);
        }
        Ok(())
    }

    async fn update_status_for_run(
        &self,
        id: &str,
        org_id: &str,
        run_id: &str,
        status: WorkflowStatus,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE workflows SET status = $1, status_cause = NULL, execution_missing_since = NULL, updated_at = NOW() \
             WHERE id = CAST($2 AS uuid) AND org_id = $3 AND temporal_run_id = $4",
        )
        .bind(status.as_str())
        .bind(id)
        .bind(org_id)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("update workflow status for run: {err}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn reconcile_candidates(&self, limit: usize) -> Result<Vec<Workflow>> {
        let rows = sqlx::query(RECONCILE_CANDIDATES_SQL)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| WorkflowError::Internal(format!("list reconcile candidates: {err}")))?;
        rows.iter().map(row_to_workflow).collect()
    }

    async fn record_execution_check(&self, workflow: &Workflow, mark: MissingMark, observation: &str) -> Result<bool> {
        let mark = match mark {
            MissingMark::Keep => "keep",
            MissingMark::Clear => "clear",
            MissingMark::Set => "set",
        };
        let result = bind_execution(sqlx::query(RECORD_EXECUTION_CHECK_SQL), workflow)
            .bind(mark)
            .bind(observation)
            .execute(&self.pool)
            .await
            .map_err(|err| WorkflowError::Internal(format!("record execution check: {err}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn mark_execution_missing(&self, workflow: &Workflow) -> Result<bool> {
        let result = bind_execution(sqlx::query(MARK_EXECUTION_MISSING_SQL), workflow)
            .execute(&self.pool)
            .await
            .map_err(|err| WorkflowError::Internal(format!("mark execution missing: {err}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn set_status_if_unchanged(&self, workflow: &Workflow, status: WorkflowStatus) -> Result<bool> {
        let result = bind_execution(sqlx::query(SET_STATUS_IF_UNCHANGED_SQL), workflow)
            .bind(status.as_str())
            .execute(&self.pool)
            .await
            .map_err(|err| WorkflowError::Internal(format!("set workflow status if unchanged: {err}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn reconcile_status(
        &self,
        workflow: &Workflow,
        status: WorkflowStatus,
        cause: &str,
        missing_for_secs: Option<u64>,
    ) -> Result<bool> {
        let result = bind_execution(sqlx::query(RECONCILE_STATUS_SQL), workflow)
            .bind(status.as_str())
            .bind(cause)
            .bind(missing_for_secs.map(|secs| secs as f64))
            .bind(observation_for_cause(cause))
            .execute(&self.pool)
            .await
            .map_err(|err| WorkflowError::Internal(format!("reconcile workflow status: {err}")))?;
        Ok(result.rows_affected() == 1)
    }

    async fn update_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        err_msg: Option<String>,
        output: Option<serde_json::Value>,
    ) -> Result<()> {
        let now = Utc::now();
        let started_at = if status == NodeStatus::Running { Some(now) } else { None };
        let completed_at = if matches!(status, NodeStatus::Completed | NodeStatus::Failed | NodeStatus::Skipped) {
            Some(now)
        } else {
            None
        };
        let result = sqlx::query(
            "UPDATE workflow_nodes SET status = $1, started_at = COALESCE($2, started_at),                     completed_at = COALESCE($3, completed_at), error = COALESCE($4, error), output = COALESCE($5, output)              WHERE id = CAST($6 AS uuid)"
        )
        .bind(status.as_str())
        .bind(started_at)
        .bind(completed_at)
        .bind(err_msg)
        .bind(output)
        .bind(node_id)
        .execute(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("update workflow node status: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(WorkflowError::NotFound);
        }
        Ok(())
    }

    async fn reset_nodes(&self, workflow_id: &str, org_id: &str) -> Result<()> {
        // Org-scoped via the workflow membership subquery so a node reset cannot
        // cross tenants even though workflow_nodes has no direct org_id column.
        sqlx::query(
            "UPDATE workflow_nodes \
             SET status = 'pending', started_at = NULL, completed_at = NULL, error = NULL, output = NULL \
             WHERE workflow_id = CAST($1 AS uuid) \
               AND workflow_id IN (SELECT id FROM workflows WHERE org_id = $2)",
        )
        .bind(workflow_id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| WorkflowError::Internal(format!("reset workflow nodes: {err}")))?;
        Ok(())
    }

    async fn history(&self, workflow_id: &str) -> Result<Vec<NodeHistory>> {
        Ok(self
            .get_nodes(workflow_id)
            .await?
            .into_iter()
            .map(|node| NodeHistory {
                node_id: node.id,
                node_name: node.name,
                node_type: node.node_type,
                status: node.status,
                started_at: node.started_at,
                completed_at: node.completed_at,
                error: node.error,
            })
            .collect())
    }
}

/// Binds `$1`..`$5` of `same_execution_sql!`.
fn bind_execution<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    workflow: &'q Workflow,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query
        .bind(&workflow.id)
        .bind(&workflow.org_id)
        .bind(&workflow.temporal_workflow_id)
        .bind(&workflow.temporal_run_id)
        .bind(workflow.state_version)
}

fn row_to_workflow(row: &PgRow) -> Result<Workflow> {
    let status = row
        .try_get::<String, _>("status")
        .map_err(|err| WorkflowError::Internal(format!("read workflow status: {err}")))?;
    Ok(Workflow {
        id: row.try_get("id").map_err(|err| WorkflowError::Internal(format!("read workflow id: {err}")))?,
        name: row.try_get("name").map_err(|err| WorkflowError::Internal(format!("read workflow name: {err}")))?,
        description: row
            .try_get("description")
            .map_err(|err| WorkflowError::Internal(format!("read workflow description: {err}")))?,
        status: WorkflowStatus::from_str(&status).map_err(WorkflowError::Internal)?,
        org_id: row.try_get("org_id").map_err(|err| WorkflowError::Internal(format!("read workflow org_id: {err}")))?,
        created_by: row
            .try_get("created_by")
            .map_err(|err| WorkflowError::Internal(format!("read workflow created_by: {err}")))?,
        temporal_workflow_id: row
            .try_get("temporal_workflow_id")
            .map_err(|err| WorkflowError::Internal(format!("read temporal_workflow_id: {err}")))?,
        temporal_run_id: row
            .try_get("temporal_run_id")
            .map_err(|err| WorkflowError::Internal(format!("read temporal_run_id: {err}")))?,
        temporal_namespace_id: row
            .try_get("temporal_namespace_id")
            .map_err(|err| WorkflowError::Internal(format!("read temporal_namespace_id: {err}")))?,
        status_cause: row
            .try_get("status_cause")
            .map_err(|err| WorkflowError::Internal(format!("read workflow status_cause: {err}")))?,
        execution_missing_since: row
            .try_get("execution_missing_since")
            .map_err(|err| WorkflowError::Internal(format!("read workflow execution_missing_since: {err}")))?,
        execution_checked_at: row
            .try_get("execution_checked_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow execution_checked_at: {err}")))?,
        execution_observation: row
            .try_get("execution_observation")
            .map_err(|err| WorkflowError::Internal(format!("read workflow execution_observation: {err}")))?,
        reconciled_at: row
            .try_get("reconciled_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow reconciled_at: {err}")))?,
        state_version: row
            .try_get("state_version")
            .map_err(|err| WorkflowError::Internal(format!("read workflow state_version: {err}")))?,
        created_at: row
            .try_get("created_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow created_at: {err}")))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|err| WorkflowError::Internal(format!("read workflow updated_at: {err}")))?,
    })
}

fn row_to_node(row: &PgRow) -> Result<WorkflowNode> {
    let node_type =
        row.try_get::<String, _>("type").map_err(|err| WorkflowError::Internal(format!("read node type: {err}")))?;
    let status = row
        .try_get::<String, _>("status")
        .map_err(|err| WorkflowError::Internal(format!("read node status: {err}")))?;
    Ok(WorkflowNode {
        id: row.try_get("id").map_err(|err| WorkflowError::Internal(format!("read node id: {err}")))?,
        workflow_id: row
            .try_get("workflow_id")
            .map_err(|err| WorkflowError::Internal(format!("read node workflow_id: {err}")))?,
        name: row.try_get("name").map_err(|err| WorkflowError::Internal(format!("read node name: {err}")))?,
        node_type: NodeType::from_str(&node_type).map_err(WorkflowError::Internal)?,
        depends_on: Vec::new(),
        config: row.try_get("config").map_err(|err| WorkflowError::Internal(format!("read node config: {err}")))?,
        position: row
            .try_get("position")
            .map_err(|err| WorkflowError::Internal(format!("read node position: {err}")))?,
        status: NodeStatus::from_str(&status).map_err(WorkflowError::Internal)?,
        started_at: row
            .try_get("started_at")
            .map_err(|err| WorkflowError::Internal(format!("read node started_at: {err}")))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|err| WorkflowError::Internal(format!("read node completed_at: {err}")))?,
        error: row.try_get("error").map_err(|err| WorkflowError::Internal(format!("read node error: {err}")))?,
        output: row.try_get("output").map_err(|err| WorkflowError::Internal(format!("read node output: {err}")))?,
    })
}
