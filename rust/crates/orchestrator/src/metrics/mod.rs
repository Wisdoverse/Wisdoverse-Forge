mod repository;
mod store;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;

use crate::auth;
use crate::state::AppState;

pub use repository::{MemoryMetricsStore, PgMetricsStore};
pub use store::Store;

const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardMetrics {
    pub active_tasks: usize,
    pub completed_today: usize,
    pub active_agents: usize,
    pub pending_reviews: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetric {
    pub participant_id: String,
    pub display_name: String,
    pub provider: String,
    pub tasks_completed: usize,
    pub avg_duration_ms: i64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLatency {
    pub avg_hours: f64,
    pub p50_hours: f64,
    pub p95_hours: f64,
}

#[derive(Clone)]
pub struct MetricsCache {
    ttl: Duration,
    agents: Arc<Mutex<HashMap<String, TimedValue<Vec<AgentMetric>>>>>,
    latency: Arc<Mutex<HashMap<String, TimedValue<ReviewLatency>>>>,
}

#[derive(Clone)]
struct TimedValue<T> {
    value: T,
    expires_at: Instant,
}

impl MetricsCache {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, agents: Arc::new(Mutex::new(HashMap::new())), latency: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn with_default_ttl() -> Self {
        Self::new(CACHE_TTL)
    }

    pub async fn get_agents(&self, key: &str) -> Option<Vec<AgentMetric>> {
        self.get_typed(&self.agents, key).await
    }

    pub async fn set_agents(&self, key: String, value: Vec<AgentMetric>) {
        self.set_typed(&self.agents, key, value).await;
    }

    pub async fn get_latency(&self, key: &str) -> Option<ReviewLatency> {
        self.get_typed(&self.latency, key).await
    }

    pub async fn set_latency(&self, key: String, value: ReviewLatency) {
        self.set_typed(&self.latency, key, value).await;
    }

    async fn get_typed<T: Clone>(&self, storage: &Arc<Mutex<HashMap<String, TimedValue<T>>>>, key: &str) -> Option<T> {
        let mut storage = storage.lock().await;
        let entry = storage.get(key)?.clone();
        if Instant::now() >= entry.expires_at {
            storage.remove(key);
            return None;
        }
        Some(entry.value)
    }

    async fn set_typed<T>(&self, storage: &Arc<Mutex<HashMap<String, TimedValue<T>>>>, key: String, value: T) {
        let mut storage = storage.lock().await;
        storage.insert(key, TimedValue { value, expires_at: Instant::now() + self.ttl });
    }
}

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/dashboard", axum::routing::get(dashboard))
        .route("/agents", axum::routing::get(agents))
        .route("/reviews/latency", axum::routing::get(review_latency))
}

fn service_unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"ok": false, "error": "database not configured"}))).into_response()
}

async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let Some(metrics_store) = state.metrics_store.clone() else {
        return service_unavailable();
    };

    match metrics_store.dashboard(&identity.org_id).await {
        Ok(metrics) => (StatusCode::OK, Json(json!({"ok": true, "metrics": metrics}))).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "dashboard metrics query failed");
            internal_error("metrics unavailable")
        }
    }
}

async fn agents(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let Some(metrics_store) = state.metrics_store.clone() else {
        return service_unavailable();
    };

    let cache_key = format!("agents:{}", identity.org_id);
    if let Some(cached) = state.metrics_cache.get_agents(&cache_key).await {
        return (StatusCode::OK, Json(json!({"ok": true, "agents": cached, "cached": true}))).into_response();
    }

    match metrics_store.agent_leaderboard(&identity.org_id).await {
        Ok(agents) => {
            state.metrics_cache.set_agents(cache_key, agents.clone()).await;
            (StatusCode::OK, Json(json!({"ok": true, "agents": agents}))).into_response()
        }
        Err(err) => {
            tracing::error!(error = %err, "agent leaderboard query failed");
            internal_error("metrics unavailable")
        }
    }
}

async fn review_latency(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let Some(metrics_store) = state.metrics_store.clone() else {
        return service_unavailable();
    };

    let cache_key = format!("latency:{}", identity.org_id);
    if let Some(cached) = state.metrics_cache.get_latency(&cache_key).await {
        return (StatusCode::OK, Json(json!({"ok": true, "latency": cached, "cached": true}))).into_response();
    }

    match metrics_store.review_latency(&identity.org_id).await {
        Ok(latency) => {
            state.metrics_cache.set_latency(cache_key, latency.clone()).await;
            (StatusCode::OK, Json(json!({"ok": true, "latency": latency}))).into_response()
        }
        Err(err) => {
            tracing::error!(error = %err, "review latency query failed");
            internal_error("metrics unavailable")
        }
    }
}

fn internal_error(message: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": message}))).into_response()
}
