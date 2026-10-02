//! Corrective migration preserves records on upgrade and removes them only
//! after explicit task deletion, retaining tenant ownership.

use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
async fn maintenance_source_upgrade_and_explicit_deletion(pool: PgPool) {
    // Reconstruct migration 101's original FK actions to test an upgrade with
    // existing records, rather than only an empty fresh schema.
    sqlx::raw_sql("ALTER TABLE maintenance_requests DROP CONSTRAINT maintenance_requests_organization_id_fkey, DROP CONSTRAINT maintenance_requests_organization_id_task_id_fkey, ADD CONSTRAINT maintenance_requests_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id), ADD CONSTRAINT maintenance_requests_organization_id_task_id_fkey FOREIGN KEY (organization_id, task_id) REFERENCES orchestration_tasks(organization_id,id)")
        .execute(&pool).await.unwrap();
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations(id,name,slug) VALUES ($1,'Test',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email) VALUES ($1,'dev@example.com')").bind(user).execute(&pool).await.unwrap();
    let task: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks(organization_id,title,created_by) VALUES ($1,'Maintenance deletion contract',$2) RETURNING id")
        .bind(org).bind(user).fetch_one(&pool).await.unwrap();
    let report = Uuid::new_v4();
    sqlx::query("INSERT INTO maintenance_requests(organization_id,task_id,repository,source_kind,source_reference,default_branch,starting_sha) VALUES ($1,$2,'example-org/example-repo','request','delete-contract','main',$3)")
        .bind(org).bind(task).bind("a".repeat(40)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO maintenance_verification_reports(id,organization_id,task_id,request_key,author_id,task_version,revision,starting_revision,criteria,input,snapshot) VALUES ($1,$2,$3,$4,$5,0,$6,$6,'Recorded evidence','{}','{}')")
        .bind(report).bind(org).bind(task).bind(Uuid::new_v4()).bind(user).bind("a".repeat(40)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO maintenance_review_decisions(id,organization_id,task_id,report_id,request_key,reviewer_id,verdict,reason,input) VALUES ($1,$2,$3,$4,$5,$6,'accepted','Reviewed evidence','{}')")
        .bind(Uuid::new_v4()).bind(org).bind(task).bind(report).bind(Uuid::new_v4()).bind(user).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO maintenance_handoffs(id,organization_id,task_id,request_key,author_id,reason,next_step,input,snapshot) VALUES ($1,$2,$3,$4,$5,'Blocked','Continue review','{}','{}')")
        .bind(Uuid::new_v4()).bind(org).bind(task).bind(Uuid::new_v4()).bind(user).execute(&pool).await.unwrap();
    let blocked =
        sqlx::query("DELETE FROM orchestration_tasks WHERE id=$1").bind(task).execute(&pool).await.unwrap_err();
    assert_eq!(blocked.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));

    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../migrations/104_maintenance_source_deletion.sql")).execute(&pool).await.unwrap();
    }
    let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM maintenance_requests), (SELECT count(*) FROM maintenance_verification_reports), (SELECT count(*) FROM maintenance_review_decisions), (SELECT count(*) FROM maintenance_handoffs)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, 1), "upgrade must preserve every record");
    let foreign_org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations(id,name,slug) VALUES ($1,'Foreign',$2)")
        .bind(foreign_org)
        .bind(foreign_org.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let forged = sqlx::query("UPDATE maintenance_requests SET organization_id=$1 WHERE task_id=$2")
        .bind(foreign_org)
        .bind(task)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(forged.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));
    sqlx::query("DELETE FROM orchestration_tasks WHERE id=$1").bind(task).execute(&pool).await.unwrap();
    let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM maintenance_requests), (SELECT count(*) FROM maintenance_verification_reports), (SELECT count(*) FROM maintenance_review_decisions), (SELECT count(*) FROM maintenance_handoffs)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0, 0), "explicit task deletion removes its records");
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT confdeltype::text FROM pg_constraint WHERE conrelid='maintenance_requests'::regclass AND contype='f'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, vec!["c".to_string(), "c".to_string()]);
}
