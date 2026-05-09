//! Attachment object storage integration tests.

use std::path::PathBuf;
use std::sync::Arc;

use agentforge_api::repositories::attachment::AttachmentRepository;
use agentforge_api::services::attachment::AttachmentService;
use agentforge_api::test_support::{mint_test_jwt, tenant_scope_for_ids, test_app_with_mock_provider};
use agentforge_core::TenantScope;
use agentforge_infra::ObjectStorageClient;
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode, header};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn seed_scope(pool: &PgPool) -> TenantScope {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind("Attachment Test Org")
        .bind(format!("attachment-test-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("attachment-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");

    tenant_scope_for_ids(org_id, user_id)
}

fn storage_root() -> PathBuf {
    std::env::temp_dir().join(format!("agentforge-attachments-test-{}", Uuid::new_v4()))
}

fn service(pool: &PgPool, root: PathBuf, max_file_size: i64) -> AttachmentService {
    AttachmentService::new(
        AttachmentRepository::new(pool.clone()),
        Arc::new(ObjectStorageClient::Local { root }),
        max_file_size,
        20,
    )
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upload_download_and_delete_roundtrip_uses_local_object_storage(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let root = storage_root();
    let service = service(&pool, root.clone(), 10 * 1024 * 1024);

    let attachment = service
        .create(&scope, None, "report.txt", "text/plain", b"hello attachment".to_vec())
        .await
        .expect("create attachment");

    assert_eq!(attachment.filename, "report.txt");
    assert_eq!(attachment.content_type, "text/plain");
    assert_eq!(attachment.size_bytes, 16);
    assert_eq!(attachment.storage_backend, "local");
    assert!(root.join(&attachment.storage_path).exists(), "object should be persisted before metadata is returned");

    let (metadata, bytes) = service.download(&scope, attachment.id.as_uuid()).await.expect("download attachment");
    assert_eq!(metadata.id, attachment.id);
    assert_eq!(bytes, b"hello attachment");

    service.delete(&scope, attachment.id.as_uuid()).await.expect("delete attachment");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE id = $1")
        .bind(attachment.id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count attachments");
    assert_eq!(count, 0, "delete should remove metadata after object deletion");
    assert!(!root.join(&attachment.storage_path).exists(), "delete should remove object bytes");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn oversized_upload_does_not_insert_metadata_or_write_object(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let root = storage_root();
    let service = service(&pool, root.clone(), 3);

    let err = service
        .create(&scope, None, "report.txt", "text/plain", b"four".to_vec())
        .await
        .expect_err("oversized upload must fail");

    assert!(format!("{}", err.kind).contains("size limit"), "unexpected error: {err:?}");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE organization_id = $1")
        .bind(scope.org_id().as_uuid())
        .fetch_one(&pool)
        .await
        .expect("count attachments");
    assert_eq!(count, 0, "rejected upload must not insert metadata");
    assert!(!root.exists(), "rejected upload must not write object bytes");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn multipart_endpoint_uploads_and_downloads_bytes(pool: PgPool) {
    let scope = seed_scope(&pool).await;
    let jwt = mint_test_jwt(scope.org_id().as_uuid(), scope.user_id().as_uuid(), "member");
    let app = test_app_with_mock_provider(pool, "mock", "ignored").await;

    let boundary = "agentforge-test-boundary";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"api.txt\"\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         hello from api\r\n\
         --{boundary}--\r\n"
    );

    let upload_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/attachments")
                .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
                .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
                .body(Body::from(body))
                .expect("upload request"),
        )
        .await
        .expect("upload response");

    assert_eq!(upload_response.status(), StatusCode::CREATED);
    let upload_json: serde_json::Value =
        serde_json::from_slice(&to_bytes(upload_response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let attachment_id = upload_json["data"]["id"].as_str().expect("attachment id");

    let download_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/attachments/{attachment_id}/download"))
                .header(header::AUTHORIZATION, format!("Bearer {jwt}"))
                .body(Body::empty())
                .expect("download request"),
        )
        .await
        .expect("download response");

    assert_eq!(download_response.status(), StatusCode::OK);
    assert_eq!(download_response.headers()[header::CONTENT_TYPE], "text/plain");
    let bytes = to_bytes(download_response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&bytes[..], b"hello from api");
}
