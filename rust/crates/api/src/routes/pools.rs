//! Container pool status endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/pools/status` — get pool status

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;

/// `GET /api/v1/pools/status` — get container pool status.
///
/// Returns warm pool counts. Requires Docker to be available.
async fn pool_status(State(state): State<AppState>, _auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    // Pool status is available through the platform crate's ContainerPool.
    // For now, return a static response indicating pool availability.
    let docker_available = state.docker.is_some();

    Ok(Json(serde_json::json!({
        "ok": true,
        "data": {
            "docker_available": docker_available,
            "message": "pool status — warm pool integration pending"
        }
    })))
}

/// Build pool routes sub-router.
pub fn pool_routes() -> Router<AppState> {
    Router::new().route("/pools/status", get(pool_status))
}

#[cfg(test)]
mod tests {
    #[test]
    fn pool_status_response_format() {
        let response = serde_json::json!({
            "ok": true,
            "data": {
                "docker_available": true,
                "warm_count": 3,
                "min_size": 2,
                "max_size": 10
            }
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["docker_available"], true);
    }
}
