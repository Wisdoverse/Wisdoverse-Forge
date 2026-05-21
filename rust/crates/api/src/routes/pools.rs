//! Container pool status endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/pools/status` — get pool status

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::pool::PoolService;

/// `GET /api/v1/pools/status` — get container pool status.
///
/// Returns warm pool counts. Requires Docker to be available.
async fn pool_status(State(state): State<AppState>, _auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = PoolService::new(state.docker.clone());
    Ok(Json(service.status_response()))
}

/// Build pool routes sub-router.
pub fn pool_routes() -> Router<AppState> {
    Router::new().route("/pools/status", get(pool_status))
}

#[cfg(test)]
mod tests {
    use crate::services::pool::pool_status_response;

    #[test]
    fn pool_status_response_format() {
        let response = pool_status_response(true);
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["docker_available"], true);
    }
}
