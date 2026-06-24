use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

use super::errors::{Result, WorkflowError};
use super::model::{NodeHistory, NodeStatus, NodeType, Workflow, WorkflowNode, WorkflowStatus};
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
        record.workflow.status = status;
        record.workflow.updated_at = Utc::now();
        Ok(())
    }

    async fn set_temporal_ids(&self, id: &str, org_id: &str, workflow_id: String, run_id: String) -> Result<()> {
        let mut records = self.records.lock().await;
        let Some(record) = records.get_mut(id).filter(|record| record.workflow.org_id == org_id) else {
            return Err(WorkflowError::NotFound);
        };
        record.workflow.temporal_workflow_id = Some(workflow_id);
        record.workflow.temporal_run_id = Some(run_id);
        record.workflow.updated_at = Utc::now();
        Ok(())
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
        let row = sqlx::query(
            "SELECT id::text AS id, name, description, status, org_id, created_by::text AS created_by,                     temporal_workflow_id, temporal_run_id, created_at, updated_at              FROM workflows WHERE id = CAST($1 AS uuid) AND org_id = $2"
        )
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
        let rows = sqlx::query(
            "SELECT id::text AS id, name, description, status, org_id, created_by::text AS created_by,                     temporal_workflow_id, temporal_run_id, created_at, updated_at              FROM workflows WHERE org_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
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
            "UPDATE workflows SET status = $1, updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3",
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

    async fn set_temporal_ids(&self, id: &str, org_id: &str, workflow_id: String, run_id: String) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workflows SET temporal_workflow_id = $1, temporal_run_id = $2, updated_at = NOW()              WHERE id = CAST($3 AS uuid) AND org_id = $4"
        )
        .bind(workflow_id)
        .bind(run_id)
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
