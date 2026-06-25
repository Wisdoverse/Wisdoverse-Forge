//! CN-3 contract: liveness vs readiness probes on the orchestrator.
//!
//! `/health` is a shallow LIVENESS probe (the process is up) and must stay green
//! even with no database. `/health/ready` is a READINESS probe that reports 503
//! unless the Postgres pool is configured and answering, so a load balancer /
//! Kubernetes readiness gate stops routing to an orchestrator that lost its DB.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).expect("build request")
}

#[tokio::test]
async fn liveness_is_shallow_and_ok_without_a_database() {
    let app = AppState::test().router();
    let res = app.oneshot(get("/health")).await.expect("request");
    assert_eq!(res.status(), StatusCode::OK, "liveness must not depend on the database");
}

#[tokio::test]
async fn readiness_is_503_without_a_database_pool() {
    let app = AppState::test().router();
    let res = app.oneshot(get("/health/ready")).await.expect("request");
    assert_eq!(
        res.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "readiness must fail when the orchestrator has no usable database"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn readiness_is_200_with_a_healthy_pool(pool: sqlx::PgPool) {
    let app = AppState::test_mcp_pg(pool, "secret-token", "org-health").router();
    let res = app.oneshot(get("/health/ready")).await.expect("request");
    assert_eq!(res.status(), StatusCode::OK, "readiness must pass when the DB pool answers");
}
