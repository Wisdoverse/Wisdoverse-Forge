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
/// Middleware is applied bottom-up (last `.layer()` call = outermost):
/// 1. CORS (innermost)
/// 2. Tracing
/// 3. CatchPanic (converts handler panics into a synthesized 500 Response)
/// 4. HTTP metrics (outermost — so the synthesized 500 is counted; see below)
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
        .route("/auth/refresh", post(routes::auth::refresh_token))
        .route("/auth/providers", get(routes::auth::providers))
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
        .merge(routes::users::user_routes())
        .merge(routes::api_keys::api_key_routes())
        .merge(routes::api_keys::legacy_auth_api_key_routes())
        .merge(routes::ssh_keys::ssh_key_routes())
        .merge(routes::ssh_keys::legacy_user_ssh_key_routes())
        .merge(routes::git_credentials::git_credential_routes())
        .merge(routes::cli_credentials::cli_credential_routes())
        .merge(routes::cli_auth_proxy::cli_auth_proxy_routes())
        .merge(routes::orchestration::orchestration_routes())
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
        .merge(routes::metrics::metrics_routes::<AppState>())
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

    router
        // State
        .with_state(state.clone())
        // Make JwtManager available to the AuthUser extractor via request extensions
        .layer(Extension(state.jwt.clone()))
        // Inner middleware (applied bottom-up): CORS, then tracing, then
        // CatchPanic wraps both.
        .layer(middleware::cors_layer(state.config.is_production(), state.config.cors_origin.as_deref()))
        .layer(middleware::trace_layer())
        .layer(middleware::catch_panic_layer())
        // HTTP request metrics (http_requests_total / http_request_duration_seconds).
        //
        // Applied LAST so it is the OUTERMOST layer — it wraps `catch_panic_layer`.
        // Ordering is load-bearing two ways:
        //
        //  1. Panic accounting. Tower/Axum layers run bottom-up, so on the
        //     request path this layer runs first and `catch_panic_layer` runs
        //     just inside it. A panicking handler therefore unwinds *up to*
        //     CatchPanic, which converts the panic into a synthesized 500
        //     `Response`, and that 500 then flows back out through this layer's
        //     `next.run(req).await` as a normal value — so it is counted as
        //     `http_requests_total{status="500"}`. If this layer were inside
        //     CatchPanic, the unwind would tear through its own
        //     `next.run().await` and the recording code would never execute,
        //     making panic-induced 500s invisible to the metric (the bug this
        //     fixes). See `observability::http_metrics` for the ordering test.
        //  2. MatchedPath. This still wraps the routed `Router`, so
        //     `MatchedPath` is populated and the `path` label is the matched
        //     route template (e.g. `/api/v1/agents/{id}/restart`) — what the
        //     agents-runtime SLO alert rules and Grafana panels query. Unmatched
        //     URIs (404s, SPA fallback) collapse into a single `<unmatched>`
        //     series.
        .layer(axum::middleware::from_fn(crate::observability::track_http_metrics))
}
