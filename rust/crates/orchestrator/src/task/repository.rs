use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, QueryBuilder, Row};
use tokio::sync::Mutex;

use super::errors::{Result, TaskError};
use super::model::{Task, TaskFilter, TaskPriority, TaskState, UpdateTaskRequest};
use super::store::Store;

pub struct MemoryStore {
    seq: AtomicU64,
    tasks: Mutex<HashMap<String, Task>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { seq: AtomicU64::new(1), tasks: Mutex::new(HashMap::new()) }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, task: &mut Task) -> Result<()> {
        let now = Utc::now();
        let id = format!("task-{}", self.seq.fetch_add(1, Ordering::Relaxed));
        task.id = id.clone();
        task.created_at = now;
        task.updated_at = now;

        self.tasks.lock().await.insert(id, task.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Task> {
        self.tasks.lock().await.get(id).filter(|task| task.org_id == org_id).cloned().ok_or(TaskError::NotFound)
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> = self
            .tasks
            .lock()
            .await
            .values()
            .filter(|task| task.org_id == filter.org_id)
            .filter(|task| filter.state.is_none_or(|state| task.state == state))
            .filter(|task| {
                filter.assigned_to.as_deref().is_none_or(|assigned| task.assigned_to.as_deref() == Some(assigned))
            })
            .cloned()
            .collect();
        tasks.sort_by_key(|task| std::cmp::Reverse(task.created_at));
        Ok(tasks.into_iter().skip(filter.offset).take(filter.limit).collect())
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateTaskRequest) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };

        if let Some(title) = req.title {
            task.title = title;
        }
        if let Some(description) = req.description {
            task.description = description;
        }
        if let Some(priority) = req.priority {
            task.priority = priority;
        }
        if let Some(assigned_to) = req.assigned_to {
            task.assigned_to = Some(assigned_to);
        }
        task.updated_at = Utc::now();
        Ok(())
    }

    async fn update_state(&self, id: &str, org_id: &str, state: TaskState) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };
        task.state = state;
        task.updated_at = Utc::now();
        Ok(())
    }

    async fn set_assignee(&self, id: &str, org_id: &str, participant_id: Option<String>) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };
        task.assigned_to = participant_id;
        task.updated_at = Utc::now();
        Ok(())
    }

    async fn set_session_id(&self, id: &str, org_id: &str, session_id: String) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };
        task.agentforge_session_id = Some(session_id);
        task.updated_at = Utc::now();
        Ok(())
    }

    async fn set_review_id(&self, id: &str, org_id: &str, review_id: String) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };
        task.review_id = Some(review_id);
        task.updated_at = Utc::now();
        Ok(())
    }

    async fn assign(&self, id: &str, org_id: &str, participant_id: String, state: TaskState) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(id).filter(|task| task.org_id == org_id) else {
            return Err(TaskError::NotFound);
        };
        task.assigned_to = Some(participant_id);
        task.state = state;
        task.updated_at = Utc::now();
        Ok(())
    }
}

pub struct PgTaskStore {
    pool: PgPool,
}

impl PgTaskStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgTaskStore {
    async fn create(&self, task: &mut Task) -> Result<()> {
        let depends_on = task.depends_on.clone();
        let mut tx =
            self.pool.begin().await.map_err(|err| TaskError::Internal(format!("begin task transaction: {err}")))?;
        let row = sqlx::query(
            "INSERT INTO tasks (workflow_id, title, description, state, priority, assigned_to, review_id, agentforge_session_id, created_by, org_id)              VALUES (CAST($1 AS uuid), $2, $3, $4, $5, CAST($6 AS uuid), CAST($7 AS uuid), $8, CAST($9 AS uuid), $10)              RETURNING id::text AS id, workflow_id::text AS workflow_id, title, description, state, priority,                        assigned_to::text AS assigned_to, review_id::text AS review_id, agentforge_session_id,                        created_by::text AS created_by, org_id, created_at, updated_at"
        )
        .bind(task.workflow_id.as_deref())
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.state.as_str())
        .bind(task.priority.as_str())
        .bind(task.assigned_to.as_deref())
        .bind(task.review_id.as_deref())
        .bind(task.agentforge_session_id.as_deref())
        .bind(&task.created_by)
        .bind(&task.org_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| TaskError::Internal(format!("insert task: {err}")))?;

        let mut created = row_to_task(&row)?;
        for dependency in &depends_on {
            sqlx::query(
                "INSERT INTO task_dependencies (task_id, depends_on) VALUES (CAST($1 AS uuid), CAST($2 AS uuid))",
            )
            .bind(&created.id)
            .bind(dependency)
            .execute(&mut *tx)
            .await
            .map_err(|err| TaskError::Internal(format!("insert task dependency: {err}")))?;
        }
        tx.commit().await.map_err(|err| TaskError::Internal(format!("commit task transaction: {err}")))?;

        created.depends_on = depends_on;
        *task = created;
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Task> {
        let row = sqlx::query(
            "SELECT id::text AS id, workflow_id::text AS workflow_id, title, description, state, priority,                     assigned_to::text AS assigned_to, review_id::text AS review_id, agentforge_session_id,                     created_by::text AS created_by, org_id, created_at, updated_at              FROM tasks WHERE id = CAST($1 AS uuid) AND org_id = $2"
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("get task: {err}")))?
        .ok_or(TaskError::NotFound)?;

        let mut task = row_to_task(&row)?;
        let dependency_rows = sqlx::query(
            "SELECT depends_on::text AS depends_on FROM task_dependencies WHERE task_id = CAST($1 AS uuid)",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("get task dependencies: {err}")))?;
        task.depends_on = dependency_rows
            .iter()
            .map(|row| row.try_get::<String, _>("depends_on"))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|err| TaskError::Internal(format!("scan task dependency: {err}")))?;
        Ok(task)
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let limit = if filter.limit == 0 { 50 } else { filter.limit };
        let mut qb = QueryBuilder::new(
            "SELECT id::text AS id, workflow_id::text AS workflow_id, title, description, state, priority,                     assigned_to::text AS assigned_to, review_id::text AS review_id, agentforge_session_id,                     created_by::text AS created_by, org_id, created_at, updated_at              FROM tasks WHERE org_id = ",
        );
        qb.push_bind(&filter.org_id);
        if let Some(state) = filter.state {
            qb.push(" AND state = ").push_bind(state.as_str());
        }
        if let Some(assigned_to) = filter.assigned_to.as_deref() {
            qb.push(" AND assigned_to = CAST(").push_bind(assigned_to).push(" AS uuid)");
        }
        qb.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(filter.offset as i64);

        let rows =
            qb.build().fetch_all(&self.pool).await.map_err(|err| TaskError::Internal(format!("list tasks: {err}")))?;

        rows.iter().map(row_to_task).collect()
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateTaskRequest) -> Result<()> {
        if req.title.is_none() && req.description.is_none() && req.priority.is_none() && req.assigned_to.is_none() {
            return Ok(());
        }

        let mut qb = QueryBuilder::new("UPDATE tasks SET updated_at = NOW()");
        if let Some(title) = req.title {
            qb.push(", title = ").push_bind(title);
        }
        if let Some(description) = req.description {
            qb.push(", description = ").push_bind(description);
        }
        if let Some(priority) = req.priority {
            qb.push(", priority = ").push_bind(priority.as_str());
        }
        if let Some(assigned_to) = req.assigned_to {
            qb.push(", assigned_to = CAST(").push_bind(assigned_to).push(" AS uuid)");
        }
        qb.push(" WHERE id = CAST(").push_bind(id).push(" AS uuid) AND org_id = ").push_bind(org_id);

        let result =
            qb.build().execute(&self.pool).await.map_err(|err| TaskError::Internal(format!("update task: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn update_state(&self, id: &str, org_id: &str, state: TaskState) -> Result<()> {
        let result =
            sqlx::query("UPDATE tasks SET state = $1, updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3")
                .bind(state.as_str())
                .bind(id)
                .bind(org_id)
                .execute(&self.pool)
                .await
                .map_err(|err| TaskError::Internal(format!("update task state: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn set_assignee(&self, id: &str, org_id: &str, participant_id: Option<String>) -> Result<()> {
        let mut qb = QueryBuilder::new("UPDATE tasks SET assigned_to = ");
        match participant_id.as_deref() {
            Some(participant_id) => {
                qb.push("CAST(").push_bind(participant_id).push(" AS uuid)");
            }
            None => {
                qb.push("NULL");
            }
        }
        qb.push(", updated_at = NOW() WHERE id = CAST(")
            .push_bind(id)
            .push(" AS uuid) AND org_id = ")
            .push_bind(org_id);

        let result =
            qb.build().execute(&self.pool).await.map_err(|err| TaskError::Internal(format!("set assignee: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn set_session_id(&self, id: &str, org_id: &str, session_id: String) -> Result<()> {
        let result = sqlx::query(
            "UPDATE tasks SET agentforge_session_id = $1, updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3"
        )
        .bind(session_id)
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("set session id: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn set_review_id(&self, id: &str, org_id: &str, review_id: String) -> Result<()> {
        let result = sqlx::query(
            "UPDATE tasks SET review_id = CAST($1 AS uuid), updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3"
        )
        .bind(review_id)
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("set review id: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn assign(&self, id: &str, org_id: &str, participant_id: String, state: TaskState) -> Result<()> {
        let result = sqlx::query(
            "UPDATE tasks SET assigned_to = CAST($1 AS uuid), state = $2, updated_at = NOW()              WHERE id = CAST($3 AS uuid) AND org_id = $4"
        )
        .bind(participant_id)
        .bind(state.as_str())
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("assign task: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }
}

fn row_to_task(row: &PgRow) -> Result<Task> {
    let state =
        row.try_get::<String, _>("state").map_err(|err| TaskError::Internal(format!("read task state: {err}")))?;
    let priority = row
        .try_get::<String, _>("priority")
        .map_err(|err| TaskError::Internal(format!("read task priority: {err}")))?;

    Ok(Task {
        id: row.try_get("id").map_err(|err| TaskError::Internal(format!("read task id: {err}")))?,
        workflow_id: row
            .try_get("workflow_id")
            .map_err(|err| TaskError::Internal(format!("read workflow id: {err}")))?,
        title: row.try_get("title").map_err(|err| TaskError::Internal(format!("read task title: {err}")))?,
        description: row
            .try_get("description")
            .map_err(|err| TaskError::Internal(format!("read task description: {err}")))?,
        state: TaskState::from_str(&state).map_err(TaskError::Internal)?,
        priority: TaskPriority::from_str(&priority).map_err(TaskError::Internal)?,
        assigned_to: row.try_get("assigned_to").map_err(|err| TaskError::Internal(format!("read assignee: {err}")))?,
        review_id: row.try_get("review_id").map_err(|err| TaskError::Internal(format!("read review id: {err}")))?,
        agentforge_session_id: row
            .try_get("agentforge_session_id")
            .map_err(|err| TaskError::Internal(format!("read agentforge session id: {err}")))?,
        depends_on: Vec::new(),
        created_by: row.try_get("created_by").map_err(|err| TaskError::Internal(format!("read created_by: {err}")))?,
        org_id: row.try_get("org_id").map_err(|err| TaskError::Internal(format!("read org_id: {err}")))?,
        created_at: row.try_get("created_at").map_err(|err| TaskError::Internal(format!("read created_at: {err}")))?,
        updated_at: row.try_get("updated_at").map_err(|err| TaskError::Internal(format!("read updated_at: {err}")))?,
    })
}
