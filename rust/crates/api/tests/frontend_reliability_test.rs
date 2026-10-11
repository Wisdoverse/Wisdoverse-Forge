//! Browser observations use authenticated tenant scope and real PostgreSQL.

use agentforge_api::repositories::retention::RetentionRepository;
use agentforge_api::test_support::{mint_test_jwt, test_app_with_mock_provider};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn fixtures(pool: &PgPool) -> (Uuid, Uuid, String) {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email) VALUES ($1,'dev@example.com')").bind(user).execute(pool).await.unwrap();
    let (org, token) = organization(pool, user).await;
    (user, org, token)
}

async fn organization(pool: &PgPool, user: Uuid) -> (Uuid, String) {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id,name,slug) VALUES ($1,'Test',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_members (organization_id,user_id,role) VALUES ($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    (org, mint_test_jwt(org, user, "owner"))
}

async fn request(app: &Router, token: Option<&str>, event: Option<Value>, query: &str) -> (StatusCode, Value) {
    let mut request = Request::builder();
    let body = if let Some(event) = event {
        request = request.method("POST").uri("/api/v1/analytics/events").header("content-type", "application/json");
        Body::from(event.to_string())
    } else {
        request = request.uri(format!("/api/v1/analytics/frontend-reliability{query}"));
        Body::empty()
    };
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn event(app: &Router, token: &str, session: Uuid, kind: &str) {
    let (status, body) = request(
        app,
        Some(token),
        Some(json!({
            "event_name": format!("frontend_session_{kind}"), "properties": {"browserSessionId":session}
        })),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn observations_require_auth_and_strict_private_properties(pool: PgPool) {
    let (user, org, token) = fixtures(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    assert_eq!(request(&app, None, None, "").await.0, StatusCode::UNAUTHORIZED);
    let id = Uuid::new_v4();
    let valid = json!({"event_name":"frontend_session_started","properties":{"browserSessionId":id}});
    assert_eq!(request(&app, None, Some(valid.clone()), "").await.0, StatusCode::UNAUTHORIZED);
    for properties in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"browserSessionId":"bad"}),
        json!({"browserSessionId":Uuid::nil()}),
        json!({"browserSessionId":"00000000-0000-4000-0000-000000000000"}),
        json!({"browserSessionId":"AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"}),
        json!({"browserSessionId":id,"stack":"PRIVATE_ERROR","url":"https://staging.example.com/private"}),
        json!({"browserSessionId":id,"organizationId":Uuid::new_v4()}),
    ] {
        let (status, body) = request(
            &app,
            Some(&token),
            Some(json!({
                "event_name":"frontend_session_started","properties":properties
            })),
            "",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(!body.to_string().contains("PRIVATE_ERROR"));
        assert!(!body.to_string().contains("staging.example.com"));
    }
    let (status, _) = request(
        &app,
        Some(&token),
        Some(json!({
            "event_name":"frontend_session_unknown","properties":{"browserSessionId":id}
        })),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(&app, Some(&token), Some(valid), "").await;
    assert_eq!(status, StatusCode::OK);
    let owners: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT organization_id,user_id FROM analytics_events").fetch_all(&pool).await.unwrap();
    assert_eq!(owners, vec![(org, user)]);
    let (status, _) = request(
        &app,
        Some(&token),
        Some(json!({
            "event_name":"existing_product_event","properties":{"keepExistingShape":true}
        })),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let invalid_token = format!("{token}invalid");
    assert_eq!(request(&app, Some(&invalid_token), None, "").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        request(
            &app,
            Some(&invalid_token),
            Some(json!({
                "event_name":"frontend_session_ended","properties":{"browserSessionId":id}
            })),
            ""
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn duplicates_missing_ends_and_late_crashes_keep_an_honest_tenant_cohort(pool: PgPool) {
    let (user, org, token) = fixtures(&pool).await;
    let (other, other_token) = organization(&pool, user).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let good = Uuid::new_v4();
    let bad = Uuid::new_v4();
    let unfinished = Uuid::new_v4();
    for (session, kind) in [
        (good, "ended"),
        (good, "started"),
        (good, "started"),
        (good, "ended"),
        (bad, "started"),
        (bad, "ended"),
        (bad, "crashed"),
        (bad, "crashed"),
        (bad, "ended"),
        (unfinished, "started"),
    ] {
        event(&app, &token, session, kind).await;
    }
    event(&app, &other_token, good, "started").await;
    event(&app, &other_token, good, "crashed").await;
    let (status, body) = request(&app, Some(&token), None, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["startedSessions"], 3);
    assert_eq!(data["crashedSessions"], 1);
    assert_eq!(data["endedWithoutObservedCrash"], 1);
    assert_eq!(data["unfinishedSessions"], 1);
    assert_eq!(data["orphanSessions"], 0);
    assert_eq!(data["observedCrashFreeRate"], 1.0 / 3.0);
    assert_eq!(data["populationCoverageVerified"], false);
    for private in [user.to_string(), org.to_string(), other.to_string(), good.to_string(), "dev@example.com".into()] {
        assert!(!body.to_string().contains(&private));
    }
    let (_, foreign) = request(&app, Some(&other_token), None, "").await;
    assert_eq!(foreign["data"]["startedSessions"], 1);
    assert_eq!(foreign["data"]["crashedSessions"], 1);
    // A fresh API state has no in-memory measurement state to lose.
    let restarted = test_app_with_mock_provider(pool, "mock", "unused").await;
    assert_eq!(request(&restarted, Some(&token), None, "").await.1["data"]["startedSessions"], 3);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn first_start_is_fixed_before_window_filtering_and_late_failure_is_absorbing(pool: PgPool) {
    let (_, org, token) = fixtures(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let old = Uuid::new_v4();
    let selected = Uuid::new_v4();
    for (session, kind) in [(old, "started"), (selected, "started"), (selected, "ended")] {
        event(&app, &token, session, kind).await;
    }
    sqlx::query("UPDATE analytics_events SET created_at=NOW()-INTERVAL '25 hours' WHERE organization_id=$1 AND properties->>'browserSessionId'=$2")
        .bind(org).bind(old.to_string()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE analytics_events SET created_at=NOW()-INTERVAL '2 hours' WHERE organization_id=$1 AND properties->>'browserSessionId'=$2")
        .bind(org).bind(selected.to_string()).execute(&pool).await.unwrap();
    event(&app, &token, old, "started").await;
    event(&app, &token, old, "ended").await;
    event(&app, &token, selected, "crashed").await;
    let (_, body) = request(&app, Some(&token), None, "?hours=24").await;
    assert_eq!(body["data"]["startedSessions"], 1);
    assert_eq!(body["data"]["crashedSessions"], 1);
    assert_eq!(body["data"]["observedCrashFreeRate"], 0.0);
    let (_, narrow) = request(&app, Some(&token), None, "?hours=-5").await;
    assert_eq!(narrow["data"]["windowHours"], 1);
    assert_eq!(narrow["data"]["startedSessions"], 0);
    assert!(narrow["data"]["observedCrashFreeRate"].is_null());
    let (_, wide) = request(&app, Some(&token), None, "?hours=99999").await;
    assert_eq!(wide["data"]["windowHours"], 8760);
    assert_eq!(wide["data"]["startedSessions"], 2);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn empty_orphan_and_invalid_observations_cannot_qualify_a_rate(pool: PgPool) {
    let (user, org, token) = fixtures(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (_, empty) = request(&app, Some(&token), None, "").await;
    assert_eq!(empty["data"]["startedSessions"], 0);
    assert!(empty["data"]["observedCrashFreeRate"].is_null());
    let good = Uuid::new_v4();
    let orphan = Uuid::new_v4();
    event(&app, &token, good, "started").await;
    event(&app, &token, good, "ended").await;
    event(&app, &token, orphan, "crashed").await;
    let (_, incomplete) = request(&app, Some(&token), None, "").await;
    assert_eq!(incomplete["data"]["orphanSessions"], 1);
    assert!(incomplete["data"]["observedCrashFreeRate"].is_null());
    event(&app, &token, orphan, "started").await;
    let (_, placed) = request(&app, Some(&token), None, "").await;
    assert_eq!(placed["data"]["orphanSessions"], 0);
    assert_eq!(placed["data"]["observedCrashFreeRate"], 0.5);
    // Simulate malformed historical data outside the current API validator.
    sqlx::query("INSERT INTO analytics_events (organization_id,user_id,event_name,properties) VALUES ($1,$2,'frontend_session_ended','{}')")
        .bind(org).bind(user).execute(&pool).await.unwrap();
    let (_, invalid) = request(&app, Some(&token), None, "").await;
    assert_eq!(invalid["data"]["invalidObservations"], 1);
    assert!(invalid["data"]["observedCrashFreeRate"].is_null());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn retention_keeps_failure_history_until_the_started_cohort_expires(pool: PgPool) {
    let (user, org, token) = fixtures(&pool).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let id = Uuid::new_v4();
    event(&app, &token, id, "crashed").await;
    sqlx::query("UPDATE analytics_events SET created_at=NOW()-INTERVAL '30 hours' WHERE organization_id=$1")
        .bind(org)
        .execute(&pool)
        .await
        .unwrap();
    event(&app, &token, id, "started").await;
    event(&app, &token, id, "ended").await;
    sqlx::query("INSERT INTO analytics_events (organization_id,user_id,event_name,created_at) VALUES ($1,$2,'existing_product_event',NOW()-INTERVAL '30 hours')")
        .bind(org).bind(user).execute(&pool).await.unwrap();
    let retention = RetentionRepository::new(pool.clone());
    assert_eq!(retention.purge_telemetry(1).await.unwrap().1, 1);
    let (_, body) = request(&app, Some(&token), None, "?hours=24").await;
    assert_eq!(body["data"]["startedSessions"], 1);
    assert_eq!(body["data"]["crashedSessions"], 1);
    assert_eq!(body["data"]["observedCrashFreeRate"], 0.0);
    sqlx::query("UPDATE analytics_events SET created_at=NOW()-INTERVAL '30 hours' WHERE organization_id=$1")
        .bind(org)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(retention.purge_telemetry(1).await.unwrap().1, 3);
    let (_, empty) = request(&app, Some(&token), None, "?hours=24").await;
    assert_eq!(empty["data"]["startedSessions"], 0);
    assert!(empty["data"]["observedCrashFreeRate"].is_null());
}
