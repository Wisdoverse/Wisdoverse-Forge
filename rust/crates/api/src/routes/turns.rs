//! Chat turn read endpoints.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};

use crate::health::AppState;
use crate::repositories::event::EventRepository;
use crate::services::turn::{TurnService, default_turn_limit};

#[derive(Debug, Deserialize)]
pub struct TurnPageQuery {
    pub cursor: Option<String>,
    #[serde(default = "default_turn_limit")]
    pub limit: i64,
}

async fn list_agent_turns(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<TurnPageQuery>,
) -> AppResult<Json<Value>> {
    let service = TurnService::new(EventRepository::new(state.pool.clone()));
    let page = service.list_page(&auth.scope, AgentId::from(id), query.cursor.as_deref(), query.limit).await?;

    Ok(Json(json!({
        "ok": true,
        "turns": page.turns,
        "cursor": page.cursor,
        "hasMore": page.has_more,
        "totalTurnCount": page.total_turn_count,
        "lastEvent": page.last_event,
    })))
}

pub fn turn_routes() -> Router<AppState> {
    Router::new().route("/agents/{id}/turns", get(list_agent_turns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_defaults_to_chat_page_size() {
        let query: TurnPageQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 50);
        assert!(query.cursor.is_none());
    }
}
