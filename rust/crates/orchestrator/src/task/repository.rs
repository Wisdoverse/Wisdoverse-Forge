use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, QueryBuilder, Row};
use tokio::sync::Mutex;

use super::errors::{Result, TaskError};
use super::model::{Task, TaskDispatch, TaskFilter, TaskPriority, TaskState, UpdateTaskRequest};
use super::store::Store;

pub struct MemoryStore {
    seq: AtomicU64,
    tasks: Mutex<HashMap<String, Task>>,
    dispatches: Mutex<Vec<TaskDispatch>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { seq: AtomicU64::new(1), tasks: Mutex::new(HashMap::new()), dispatches: Mutex::new(Vec::new()) }
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

    async fn create_dispatch(&self, task_id: &str, org_id: &str) -> Result<String> {
        let mut dispatches = self.dispatches.lock().await;
        let n = dispatches.len() + 1;
        let id = format!("dispatch-{n}");
        let now = Utc::now();
        dispatches.push(TaskDispatch {
            id: id.clone(),
            task_id: task_id.to_string(),
            org_id: org_id.to_string(),
            status: "queued".to_string(),
            attempt: 1,
            last_error: None,
            session_id: None,
            created_at: now,
            updated_at: now,
        });
        Ok(id)
    }

    async fn update_dispatch(
        &self,
        dispatch_id: &str,
        org_id: &str,
        status: &str,
        last_error: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let mut dispatches = self.dispatches.lock().await;
        let dispatch =
            dispatches.iter_mut().find(|d| d.id == dispatch_id && d.org_id == org_id).ok_or(TaskError::NotFound)?;
        // Mirror the PgTaskStore guard: do not overwrite a reaper-set 'failed'
        // status. A late-completing spawn calling update_dispatch after the
        // reaper has already flipped to 'failed' must be a no-op.
        if dispatch.status == "failed" {
            return Err(TaskError::NotFound);
        }
        dispatch.status = status.to_string();
        if let Some(err) = last_error {
            dispatch.last_error = Some(err.to_string());
        }
        if let Some(sid) = session_id {
            dispatch.session_id = Some(sid.to_string());
        }
        dispatch.updated_at = Utc::now();
        Ok(())
    }

    async fn get_dispatch(&self, task_id: &str, org_id: &str) -> Result<TaskDispatch> {
        let dispatches = self.dispatches.lock().await;
        dispatches
            .iter()
            .filter(|d| d.task_id == task_id && d.org_id == org_id)
            .max_by_key(|d| d.created_at)
            .cloned()
            .ok_or(TaskError::NotFound)
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

    async fn create_dispatch(&self, task_id: &str, org_id: &str) -> Result<String> {
        let row = sqlx::query(
            "INSERT INTO task_dispatches (task_id, org_id, status, attempt) \
             VALUES ($1, $2, 'queued', 1) \
             RETURNING id::text AS id",
        )
        .bind(task_id)
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("create dispatch: {err}")))?;

        row.try_get("id").map_err(|err| TaskError::Internal(format!("read dispatch id: {err}")))
    }

    async fn update_dispatch(
        &self,
        dispatch_id: &str,
        org_id: &str,
        status: &str,
        last_error: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let mut qb = QueryBuilder::new("UPDATE task_dispatches SET updated_at = NOW(), status = ");
        qb.push_bind(status);
        if let Some(err) = last_error {
            qb.push(", last_error = ").push_bind(err);
        }
        if let Some(sid) = session_id {
            qb.push(", session_id = ").push_bind(sid);
        }
        qb.push(" WHERE id = CAST(")
            .push_bind(dispatch_id)
            .push(" AS uuid) AND org_id = ")
            .push_bind(org_id)
            .push(" AND status NOT IN ('failed')");

        let result = qb
            .build()
            .execute(&self.pool)
            .await
            .map_err(|err| TaskError::Internal(format!("update dispatch: {err}")))?;
        // Mirror the other update_* methods: a 0-row update means the dispatch
        // (or its org scope) was not found, or the dispatch is already in a
        // terminal 'failed' state (reaper-set). Surfacing NotFound lets the
        // spawn's best-effort wrapper log it instead of silently overwriting a
        // reaper-assigned failure verdict.
        if result.rows_affected() == 0 {
            return Err(TaskError::NotFound);
        }
        Ok(())
    }

    async fn get_dispatch(&self, task_id: &str, org_id: &str) -> Result<TaskDispatch> {
        let row = sqlx::query(
            "SELECT id::text AS id, task_id, org_id, status, attempt, last_error, session_id, created_at, updated_at \
             FROM task_dispatches \
             WHERE task_id = $1 AND org_id = $2 \
             ORDER BY created_at DESC \
             LIMIT 1",
        )
        .bind(task_id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| TaskError::Internal(format!("get dispatch: {err}")))?
        .ok_or(TaskError::NotFound)?;

        row_to_dispatch(&row)
    }
}

fn row_to_dispatch(row: &PgRow) -> Result<TaskDispatch> {
    Ok(TaskDispatch {
        id: row.try_get("id").map_err(|err| TaskError::Internal(format!("read dispatch id: {err}")))?,
        task_id: row.try_get("task_id").map_err(|err| TaskError::Internal(format!("read dispatch task_id: {err}")))?,
        org_id: row.try_get("org_id").map_err(|err| TaskError::Internal(format!("read dispatch org_id: {err}")))?,
        status: row.try_get("status").map_err(|err| TaskError::Internal(format!("read dispatch status: {err}")))?,
        attempt: row.try_get("attempt").map_err(|err| TaskError::Internal(format!("read dispatch attempt: {err}")))?,
        last_error: row
            .try_get("last_error")
            .map_err(|err| TaskError::Internal(format!("read dispatch last_error: {err}")))?,
        session_id: row
            .try_get("session_id")
            .map_err(|err| TaskError::Internal(format!("read dispatch session_id: {err}")))?,
        created_at: row
            .try_get("created_at")
            .map_err(|err| TaskError::Internal(format!("read dispatch created_at: {err}")))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|err| TaskError::Internal(format!("read dispatch updated_at: {err}")))?,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Fix #3: A dispatch already in `failed` status must not be moved to
    /// `started` (or any other status) by `update_dispatch`. This guards the
    /// race where a late-completing spawn calls `update_dispatch` after the
    /// reaper has already flipped the row to `failed`.
    #[tokio::test]
    async fn update_dispatch_does_not_overwrite_failed() {
        let store = MemoryStore::new();

        // Create a dispatch and manually drive it to 'failed' (simulating the
        // reaper having already acted).
        let dispatch_id = store.create_dispatch("task-x", "org-x").await.expect("create dispatch");
        store
            .update_dispatch(&dispatch_id, "org-x", "failed", Some("dispatch_timeout"), None)
            .await
            .expect("reaper sets failed");

        // A late spawn now tries to move it to 'started'.
        let result = store.update_dispatch(&dispatch_id, "org-x", "started", None, None).await;
        assert!(
            matches!(result, Err(TaskError::NotFound)),
            "update_dispatch must return NotFound when the dispatch is already failed, got: {result:?}"
        );

        // The status must still be 'failed' with the original last_error.
        let dispatch = store.get_dispatch("task-x", "org-x").await.expect("get dispatch");
        assert_eq!(dispatch.status, "failed", "status must remain failed");
        assert_eq!(dispatch.last_error.as_deref(), Some("dispatch_timeout"), "last_error must not be overwritten");
    }
}
