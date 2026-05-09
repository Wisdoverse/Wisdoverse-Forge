use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, QueryBuilder, Row};
use tokio::sync::Mutex;

use super::errors::{Result, ReviewError};
use super::model::{CodeReview, ReviewComment, ReviewFilter, ReviewState, ReviewWithComments};
use super::store::Store;

pub struct MemoryStore {
    review_seq: AtomicU64,
    comment_seq: AtomicU64,
    reviews: Mutex<HashMap<String, ReviewWithComments>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { review_seq: AtomicU64::new(1), comment_seq: AtomicU64::new(1), reviews: Mutex::new(HashMap::new()) }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, review: &mut CodeReview) -> Result<()> {
        let now = Utc::now();
        let id = format!("review-{}", self.review_seq.fetch_add(1, Ordering::Relaxed));
        review.id = id.clone();
        review.created_at = now;
        review.updated_at = now;

        self.reviews.lock().await.insert(id, ReviewWithComments { review: review.clone(), comments: Vec::new() });
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<ReviewWithComments> {
        self.reviews
            .lock()
            .await
            .get(id)
            .filter(|review| review.review.org_id == org_id)
            .cloned()
            .ok_or(ReviewError::NotFound)
    }

    async fn list(&self, filter: ReviewFilter) -> Result<Vec<CodeReview>> {
        let mut reviews: Vec<CodeReview> = self
            .reviews
            .lock()
            .await
            .values()
            .filter(|review| review.review.org_id == filter.org_id)
            .filter(|review| filter.task_id.as_deref().is_none_or(|task_id| review.review.task_id == task_id))
            .filter(|review| filter.state.is_none_or(|state| review.review.state == state))
            .map(|review| review.review.clone())
            .collect();
        reviews.sort_by_key(|review| std::cmp::Reverse(review.created_at));
        Ok(reviews.into_iter().skip(filter.offset).take(filter.limit).collect())
    }

    async fn update_state(&self, id: &str, org_id: &str, state: ReviewState) -> Result<()> {
        let mut reviews = self.reviews.lock().await;
        let Some(review) = reviews.get_mut(id).filter(|review| review.review.org_id == org_id) else {
            return Err(ReviewError::NotFound);
        };
        review.review.state = state;
        review.review.updated_at = Utc::now();
        Ok(())
    }

    async fn add_comment(&self, review_id: &str, org_id: &str, comment: &mut ReviewComment) -> Result<()> {
        let mut reviews = self.reviews.lock().await;
        let Some(review) = reviews.get_mut(review_id).filter(|review| review.review.org_id == org_id) else {
            return Err(ReviewError::NotFound);
        };
        comment.id = format!("comment-{}", self.comment_seq.fetch_add(1, Ordering::Relaxed));
        comment.review_id = review_id.to_string();
        comment.created_at = Utc::now();
        review.comments.push(comment.clone());
        Ok(())
    }
}

pub struct PgReviewStore {
    pool: PgPool,
}

impl PgReviewStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgReviewStore {
    async fn create(&self, review: &mut CodeReview) -> Result<()> {
        let row = sqlx::query(
            "INSERT INTO code_reviews (task_id, session_id, diff_ref, diff_snapshot, state, assigned_to, org_id, created_by)              VALUES (CAST($1 AS uuid), $2, $3, $4, $5, CAST($6 AS uuid), $7, CAST($8 AS uuid))              RETURNING id::text AS id, task_id::text AS task_id, session_id, diff_ref, diff_snapshot, state,                        assigned_to::text AS assigned_to, org_id, created_by::text AS created_by, created_at, updated_at"
        )
        .bind(&review.task_id)
        .bind(&review.session_id)
        .bind(&review.diff_ref)
        .bind(review.diff_snapshot.clone())
        .bind(review.state.as_str())
        .bind(review.assigned_to.as_deref())
        .bind(&review.org_id)
        .bind(&review.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| ReviewError::Internal(format!("insert review: {err}")))?;
        *review = row_to_review(&row)?;
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<ReviewWithComments> {
        let row = sqlx::query(
            "SELECT id::text AS id, task_id::text AS task_id, session_id, diff_ref, diff_snapshot, state,                     assigned_to::text AS assigned_to, org_id, created_by::text AS created_by, created_at, updated_at              FROM code_reviews WHERE id = CAST($1 AS uuid) AND org_id = $2"
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| ReviewError::Internal(format!("get review: {err}")))?
        .ok_or(ReviewError::NotFound)?;

        let comments = sqlx::query(
            "SELECT id::text AS id, review_id::text AS review_id, author_id::text AS author_id, body, file_path, line, created_at              FROM review_comments WHERE review_id = CAST($1 AS uuid) ORDER BY created_at ASC"
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| ReviewError::Internal(format!("get review comments: {err}")))?
        .iter()
        .map(row_to_comment)
        .collect::<Result<Vec<_>>>()?;

        Ok(ReviewWithComments { review: row_to_review(&row)?, comments })
    }

    async fn list(&self, filter: ReviewFilter) -> Result<Vec<CodeReview>> {
        let limit = if filter.limit == 0 { 50 } else { filter.limit };
        let mut qb = QueryBuilder::new(
            "SELECT id::text AS id, task_id::text AS task_id, session_id, diff_ref, diff_snapshot, state,                     assigned_to::text AS assigned_to, org_id, created_by::text AS created_by, created_at, updated_at              FROM code_reviews WHERE org_id = ",
        );
        qb.push_bind(&filter.org_id);
        if let Some(task_id) = filter.task_id.as_deref() {
            qb.push(" AND task_id = CAST(").push_bind(task_id).push(" AS uuid)");
        }
        if let Some(state) = filter.state {
            qb.push(" AND state = ").push_bind(state.as_str());
        }
        qb.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(filter.offset as i64);

        qb.build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| ReviewError::Internal(format!("list reviews: {err}")))?
            .iter()
            .map(row_to_review)
            .collect()
    }

    async fn update_state(&self, id: &str, org_id: &str, state: ReviewState) -> Result<()> {
        let result = sqlx::query(
            "UPDATE code_reviews SET state = $1, updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3",
        )
        .bind(state.as_str())
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await
        .map_err(|err| ReviewError::Internal(format!("update review state: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(ReviewError::NotFound);
        }
        Ok(())
    }

    async fn add_comment(&self, review_id: &str, org_id: &str, comment: &mut ReviewComment) -> Result<()> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM code_reviews WHERE id = CAST($1 AS uuid) AND org_id = $2)")
                .bind(review_id)
                .bind(org_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|err| ReviewError::Internal(format!("check review exists: {err}")))?;
        if !exists {
            return Err(ReviewError::NotFound);
        }

        let row = sqlx::query(
            "INSERT INTO review_comments (review_id, author_id, body, file_path, line)              VALUES (CAST($1 AS uuid), CAST($2 AS uuid), $3, $4, $5)              RETURNING id::text AS id, review_id::text AS review_id, author_id::text AS author_id, body, file_path, line, created_at"
        )
        .bind(review_id)
        .bind(&comment.author_id)
        .bind(&comment.body)
        .bind(comment.file_path.as_deref())
        .bind(comment.line)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| ReviewError::Internal(format!("insert review comment: {err}")))?;
        *comment = row_to_comment(&row)?;
        Ok(())
    }
}

fn row_to_review(row: &PgRow) -> Result<CodeReview> {
    let state =
        row.try_get::<String, _>("state").map_err(|err| ReviewError::Internal(format!("read review state: {err}")))?;
    Ok(CodeReview {
        id: row.try_get("id").map_err(|err| ReviewError::Internal(format!("read review id: {err}")))?,
        task_id: row.try_get("task_id").map_err(|err| ReviewError::Internal(format!("read task id: {err}")))?,
        session_id: row
            .try_get("session_id")
            .map_err(|err| ReviewError::Internal(format!("read session id: {err}")))?,
        diff_ref: row.try_get("diff_ref").map_err(|err| ReviewError::Internal(format!("read diff ref: {err}")))?,
        diff_snapshot: row
            .try_get("diff_snapshot")
            .map_err(|err| ReviewError::Internal(format!("read diff snapshot: {err}")))?,
        state: ReviewState::from_str(&state).map_err(ReviewError::Internal)?,
        assigned_to: row
            .try_get("assigned_to")
            .map_err(|err| ReviewError::Internal(format!("read assigned_to: {err}")))?,
        org_id: row.try_get("org_id").map_err(|err| ReviewError::Internal(format!("read org_id: {err}")))?,
        created_by: row
            .try_get("created_by")
            .map_err(|err| ReviewError::Internal(format!("read created_by: {err}")))?,
        created_at: row
            .try_get("created_at")
            .map_err(|err| ReviewError::Internal(format!("read created_at: {err}")))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|err| ReviewError::Internal(format!("read updated_at: {err}")))?,
    })
}

fn row_to_comment(row: &PgRow) -> Result<ReviewComment> {
    Ok(ReviewComment {
        id: row.try_get("id").map_err(|err| ReviewError::Internal(format!("read comment id: {err}")))?,
        review_id: row.try_get("review_id").map_err(|err| ReviewError::Internal(format!("read review id: {err}")))?,
        author_id: row.try_get("author_id").map_err(|err| ReviewError::Internal(format!("read author id: {err}")))?,
        body: row.try_get("body").map_err(|err| ReviewError::Internal(format!("read comment body: {err}")))?,
        file_path: row.try_get("file_path").map_err(|err| ReviewError::Internal(format!("read file path: {err}")))?,
        line: row.try_get("line").map_err(|err| ReviewError::Internal(format!("read line: {err}")))?,
        created_at: row
            .try_get("created_at")
            .map_err(|err| ReviewError::Internal(format!("read created_at: {err}")))?,
    })
}
