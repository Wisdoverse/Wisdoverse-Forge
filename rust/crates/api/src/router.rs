//! Main router construction.
//!
//! Assembles all route groups and applies the Tower middleware stack.
//!
//! Route layout (enterprise API versioning):
//! - `/health`         — liveness probe (infrastructure, no version)
//! - `/api/health`     — readiness probe (deep check, no version)
//! - `/ws`             — WebSocket gateway (no version)
//! - `/mcp`            — internal MCP bridge for orchestrator agent tools
//! - `/api/v1/*`       — all versioned API routes
//! - `/api/billing/*`  — billing routes (special prefix per CLAUDE.md, no v1)

use std::path::Path;

use axum::routing::{get, post};
use axum::{Extension, Router};
use tower_http::services::{ServeDir, ServeFile};

use crate::gateway;
use crate::health::{self, AppState};
use crate::middleware;
use crate::routes;

/// Build the top-level Axum router with all routes and middleware.
///
/// Middleware nests like this (request enters outer→inner; see
/// [`apply_outer_layers`] for why the outer order is load-bearing):
/// 1. Request-id correlation (outermost — span wraps everything; echoes `x-request-id`)
/// 2. HTTP metrics (counts every request, including panic-synthesized 500s)
/// 3. CORS (outside CatchPanic so panic-500s stay CORS-readable cross-origin)
/// 4. CatchPanic (converts handler panics into a synthesized 500 Response)
/// 5. Tracing
/// 6. Handler (innermost)
pub fn create_router(state: AppState) -> Router {
    let attachment_upload_body_limit = usize::try_from(state.config.storage_max_file_size)
        .unwrap_or(usize::MAX.saturating_sub(1024 * 1024))
        .saturating_add(1024 * 1024);

    let mcp_route = state
        .mcp_internal_token
        .clone()
        .zip(state.mcp_tools.clone())
        .map(|(token, tools)| crate::mcp::mcp_router(token, tools));

    // Issue #15 P4 dropped the `record_compat_hit` + `apply_deprecation_headers`
    // middleware. The nav surface is no longer "compat" — it's just the nav
    // surface. Routes merge directly into `/api/v1` below.

    // Versioned API routes — all nested under /api/v1
    let api_v1 = Router::new()
        // Public auth routes (no auth middleware yet — JWT check is per-handler via AuthUser extractor)
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/register", post(routes::auth::register))
        .route("/auth/forgot-password", post(routes::auth::forgot_password))
        .route("/auth/reset-password", post(routes::auth::reset_password))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/deprovision", post(routes::auth::deprovision))
        .route("/auth/sso/provision", post(routes::auth::sso_provision))
        .route("/auth/refresh", post(routes::auth::refresh_token))
        .route("/auth/providers", get(routes::auth::providers))
        .route("/auth/sso/oidc", get(routes::auth::sso_authorize))
        .route("/auth/sso/oidc/callback", get(routes::auth::sso_callback))
        .route("/auth/sso/exchange", post(routes::auth::sso_exchange))
        .merge(routes::scim::scim_routes())
        // Public local-join bootstrap + pairing-code claim (the code is the credential)
        .merge(routes::agent_join::agent_join_routes())
        // Protected routes
        .route("/me", get(routes::auth::me))
        .merge(routes::legacy_navigation::legacy_navigation_routes())
        .merge(routes::resource_members::resource_member_routes())
        .route("/auth/switch-context", post(routes::auth::switch_context))
        .merge(routes::agents::agent_routes())
        .merge(routes::turns::turn_routes())
        .merge(routes::events::event_routes())
        .merge(routes::organizations::organization_routes())
        .merge(routes::workspaces::workspace_routes())
        .merge(routes::projects::project_routes())
        .merge(routes::teams::team_routes())
        .merge(routes::task_templates::task_template_routes())
        .merge(routes::users::user_routes())
        .merge(routes::api_keys::api_key_routes())
        .merge(routes::api_keys::legacy_auth_api_key_routes())
        .merge(routes::ssh_keys::ssh_key_routes())
        .merge(routes::ssh_keys::legacy_user_ssh_key_routes())
        .merge(routes::git_credentials::git_credential_routes())
        .merge(routes::cli_credentials::cli_credential_routes())
        .merge(routes::cli_auth_proxy::cli_auth_proxy_routes())
        .merge(routes::orchestration::orchestration_routes())
        .merge(routes::self_fix::self_fix_routes())
        .merge(routes::llm_providers::llm_provider_routes())
        .merge(routes::settings::setting_routes())
        .merge(routes::feature_flags::feature_flag_routes())
        .merge(routes::favorites::favorite_routes())
        .merge(routes::audit::audit_routes())
        .merge(routes::groups::group_routes())
        .merge(routes::inbox::inbox_routes())
        .merge(routes::admin::admin_routes())
        .merge(routes::plugins::plugin_routes())
        .merge(routes::context::context_routes())
        .merge(routes::skills::skill_routes())
        .merge(routes::memory::memory_routes())
        .merge(routes::analytics::analytics_routes())
        .merge(routes::governance_audit::governance_audit_routes())
        .merge(routes::licenses::license_routes())
        .merge(routes::voice::voice_routes())
        .merge(routes::tiles::tile_routes())
        .merge(routes::prompts::prompt_routes())
        .merge(routes::attachments::attachment_routes(attachment_upload_body_limit))
        .merge(routes::resource_profiles::resource_profile_routes())
        .merge(routes::recurring_tasks::recurring_task_routes())
        .merge(routes::quota::quota_routes())
        .merge(routes::dev_environments::dev_environment_routes())
        .merge(routes::pools::pool_routes())
        // Container control routes (require Docker to be available)
        .route("/agents/{id}/start", post(routes::containers::start_agent))
        .route("/agents/{id}/stop", post(routes::containers::stop_agent));

    // Billing routes (special prefix /api/billing per CLAUDE.md — no v1)
    let billing_public = routes::billing::billing_webhook_routes();
    let billing_protected = routes::billing::billing_routes();

    let mut router = Router::new()
        // Infrastructure probes (no version, no auth)
        .route("/health", get(health::health))
        .route("/api/health", get(health::health_ready))
        // Prometheus scrape endpoint (admin-gated, top-level per CLAUDE.md Routing)
        .merge(routes::metrics::metrics_routes())
        // WebSocket gateway (JWT auth via query param, NATS broadcast)
        .route("/ws", get(gateway::ws::ws_handler))
        // Versioned API
        .nest("/api/v1", api_v1)
        // Billing (special prefix, no version)
        .nest("/api/billing", billing_public.merge(billing_protected));

    if let Some(mcp_route) = mcp_route {
        router = router.merge(mcp_route);
    }

    // SPA static + fallback. When `static_dir` is set and exists, serve assets
    // from disk and fall back to index.html for any unmatched GET so client-side
    // routes (e.g. /tasks, /agents) load the SPA instead of returning 404.
    if let Some(dir) = state.config.static_dir.as_deref() {
        let path = Path::new(dir);
        if path.is_dir() {
            let index = path.join("index.html");
            let serve_dir = ServeDir::new(path).fallback(ServeFile::new(&index));
            router = router.fallback_service(serve_dir);
            tracing::info!(static_dir = %path.display(), "SPA static fallback enabled");
        } else {
            tracing::warn!(static_dir = %path.display(), "static_dir not found, SPA fallback disabled");
        }
    }

    let router = router
        // State
        .with_state(state.clone())
        // Make JwtManager available to the AuthUser extractor via request extensions
        .layer(Extension(state.jwt.clone()))
        // Tracing is innermost of the outer stack; CORS moved OUT to wrap
        // CatchPanic (see apply_outer_layers) so panic-500s stay CORS-readable.
        .layer(middleware::trace_layer());

    // Apply the load-bearing OUTER middleware (CatchPanic → CORS → metrics →
    // request-id) in one shared, test-pinned helper.
    apply_outer_layers(router, state.config.is_production(), state.config.cors_origin.as_deref())
}

/// Apply the load-bearing OUTER middleware in production order. Tower runs the
/// last-applied layer outermost, so the request path enters in this order:
/// request-id → metrics → CORS → CatchPanic → (inner: tracing, handler).
///
/// Why this exact nesting:
///  - **CORS outside CatchPanic.** A panicking handler unwinds up to
///    `catch_panic_layer`, which synthesises a 500 *inside* CORS. If CORS were
///    inside CatchPanic (the unwind would tear past it) that 500 would carry no
///    `Access-Control-Allow-Origin` / `Access-Control-Expose-Headers`, so a
///    cross-origin browser could not read it — including the `x-request-id` the
///    outer layer adds. Outside CatchPanic, the synthesised 500 flows back out
///    through CORS and is decorated. (Trade-off: CORS now also short-circuits
///    preflight `OPTIONS` before the metrics layer, so preflights are not
///    counted in `http_requests_total` — intentional; they are CORS noise.)
///  - **Metrics outside CatchPanic** so panic-induced 500s are counted (see
///    [`apply_panic_and_metrics_layers`]).
///  - **Request-id outermost** so its correlation span wraps everything and the
///    `x-request-id` echo wraps the whole response, including the panic 500.
///
/// Shared by `create_router` and the ordering tests so the order can't drift.
pub(crate) fn apply_outer_layers(router: axum::Router, is_production: bool, cors_origin: Option<&str>) -> axum::Router {
    apply_panic_and_metrics_layers(router)
        .layer(middleware::cors_layer(is_production, cors_origin))
        .layer(axum::middleware::from_fn(crate::observability::track_request_id))
}

/// Apply the panic-accounting + HTTP-metrics layers in the load-bearing order:
/// the metrics layer is OUTERMOST and wraps `catch_panic_layer`.
///
/// Ordering is load-bearing two ways:
///  1. Panic accounting. Tower/Axum layers run bottom-up, so the metrics layer
///     runs first on the request path and `catch_panic_layer` runs just inside
///     it. A panicking handler unwinds up to CatchPanic, which converts the
///     panic into a synthesized 500 `Response`; that 500 flows back out through
///     the metrics layer's `next.run().await` as a normal value and is counted
///     as `http_requests_total{status="500"}`. If the metrics layer were inside
///     CatchPanic, the unwind would tear through its own `next.run().await` and
///     panic-induced 500s would be invisible to the metric.
///  2. MatchedPath. The metrics layer still wraps the routed `Router`, so the
///     `path` label is the matched route template.
///
/// Shared by `create_router` and `observability::http_metrics`'s ordering test
/// so production layer order is pinned by a single source of truth (#891/F080).
pub(crate) fn apply_panic_and_metrics_layers(router: axum::Router) -> axum::Router {
    router
        .layer(middleware::catch_panic_layer())
        .layer(axum::middleware::from_fn(crate::observability::track_http_metrics))
}

#[cfg(test)]
mod outer_layer_tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use tower::ServiceExt;

    const ORIGIN: &str = "https://app.example.com";

    async fn boom() -> &'static str {
        panic!("handler panic for the ordering test");
    }

    /// A cross-origin request to a PANICKING handler must come back as a 500 that
    /// still carries BOTH the CORS decoration (`Access-Control-Allow-Origin`) and
    /// the `x-request-id` correlation header. This pins the load-bearing nesting
    /// in [`super::apply_outer_layers`]: CORS + request-id sit OUTSIDE CatchPanic,
    /// so the synthesised 500 flows back out through them. If CORS were inside
    /// CatchPanic (the original bug), a cross-origin browser could not read the
    /// correlation id for exactly the 500s it most needs.
    #[tokio::test]
    async fn panic_500_keeps_cors_and_request_id_headers() {
        let app = super::apply_outer_layers(Router::new().route("/panic", get(boom)), true, Some(ORIGIN));

        let response = app
            .oneshot(Request::builder().uri("/panic").header(header::ORIGIN, ORIGIN).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR, "CatchPanic must synthesise a 500");
        let headers = response.headers();
        assert_eq!(
            headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).and_then(|v| v.to_str().ok()),
            Some(ORIGIN),
            "panic 500 must carry CORS allow-origin so a cross-origin browser can read it"
        );
        assert!(headers.get("x-request-id").is_some(), "panic 500 must carry x-request-id for correlation");
    }
}
