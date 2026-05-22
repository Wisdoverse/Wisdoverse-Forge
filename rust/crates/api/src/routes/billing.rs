//! Billing endpoints (nested under `/api/billing` — special prefix, no v1).
//!
//! - `GET    /api/billing/plans`        — list available plans
//! - `GET    /api/billing/subscription` — get current org subscription
//! - `POST   /api/billing/checkout`     — start hosted Stripe checkout
//! - `POST   /api/billing/portal`       — start Stripe customer portal
//! - `POST   /api/billing/subscribe`    — direct PaymentMethod subscribe route
//! - `POST   /api/billing/cancel`       — legacy cancel route
//! - `POST   /api/billing/subscription/cancel` — cancel subscription
//! - `POST   /api/billing/subscription/resume` — resume scheduled cancellation
//! - `GET    /api/billing/usage`        — list usage metrics
//! - `GET    /api/billing/invoices`     — list invoices (paginated)
//! - `POST   /api/billing/webhook`      — Stripe webhook (no auth)

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind};

use crate::domain::billing::{BillingPlanView, InvoiceView, SubscriptionView, UsageMetricView};
use crate::health::AppState;
use crate::repositories::billing::BillingRepository;
use crate::services::billing::BillingService;

/// Query parameters for paginated list endpoints.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for subscribing to a plan.
#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub plan_id: Uuid,
    pub payment_method_id: Option<String>,
}

/// Request body for creating a hosted checkout session.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutRequest {
    pub plan_id: Uuid,
    pub billing_cycle: String,
    pub success_url: String,
    pub cancel_url: String,
    pub coupon_code: Option<String>,
}

/// Request body for creating a Stripe customer-portal session.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalRequest {
    pub return_url: String,
}

/// Request body for canceling a subscription.
#[derive(Deserialize, Default)]
pub struct CancelSubscriptionRequest {
    #[serde(default)]
    pub immediately: bool,
}

/// Build a BillingService from shared state.
fn make_service(state: &AppState) -> BillingService {
    BillingService::with_gateway(BillingRepository::new(state.pool.clone()), state.billing_gateway.clone())
}

/// `GET /api/billing/plans` — list available plans.
async fn list_plans(State(state): State<AppState>, _auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let plans: Vec<_> = service.list_plans().await?.into_iter().map(BillingPlanView::from_plan).collect();
    Ok(Json(json!({ "ok": true, "data": &plans, "plans": plans })))
}

/// `GET /api/billing/subscription` — get current org subscription.
async fn get_subscription(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let sub = service.get_current_subscription(&auth.scope).await?;
    let plan = match &sub {
        Some(sub) => Some(BillingPlanView::from_plan(
            BillingRepository::new(state.pool.clone()).find_plan_by_id(sub.plan_id).await?,
        )),
        None => None,
    };
    let subscription = sub.map(SubscriptionView::from);
    Ok(Json(json!({ "ok": true, "data": &subscription, "subscription": subscription, "plan": plan })))
}

/// `POST /api/billing/checkout` — start hosted checkout.
async fn create_checkout(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CheckoutRequest>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let session = service
        .create_checkout_session(
            &auth.scope,
            req.plan_id,
            &req.billing_cycle,
            &req.success_url,
            &req.cancel_url,
            req.coupon_code.as_deref(),
        )
        .await?;
    let session_id = session.id;
    let url = session.url;
    Ok(Json(json!({ "ok": true, "data": &url, "agentId": session_id, "url": url })))
}

/// `POST /api/billing/portal` — start Stripe customer portal.
async fn create_portal(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PortalRequest>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let session = service.create_portal_session(&auth.scope, &req.return_url).await?;
    let url = session.url;
    Ok(Json(json!({ "ok": true, "data": &url, "url": url })))
}

/// `POST /api/billing/subscribe` — subscribe to a plan.
async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SubscribeRequest>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let sub = service.subscribe(&auth.scope, req.plan_id, req.payment_method_id.as_deref()).await?;
    Ok(Json(json!({ "ok": true, "data": sub })))
}

/// `POST /api/billing/cancel` — cancel subscription.
async fn cancel_subscription(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let sub = service.cancel(&auth.scope, false).await?;
    Ok(Json(json!({ "ok": true, "data": sub })))
}

/// `POST /api/billing/subscription/cancel` — cancel subscription.
async fn cancel_subscription_v2(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CancelSubscriptionRequest>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let sub = service.cancel(&auth.scope, req.immediately).await?;
    Ok(Json(json!({ "ok": true, "data": sub, "subscription": sub })))
}

/// `POST /api/billing/subscription/resume` — resume a scheduled cancellation.
async fn resume_subscription(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let sub = service.resume(&auth.scope).await?;
    Ok(Json(json!({ "ok": true, "data": sub, "subscription": sub })))
}

/// `GET /api/billing/usage` — usage metrics.
async fn get_usage(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let max_agents = match service.get_current_subscription(&auth.scope).await? {
        Some(sub) => BillingRepository::new(state.pool.clone()).find_plan_by_id(sub.plan_id).await?.max_agents as i64,
        None => 1,
    };
    let usage: Vec<UsageMetricView> =
        vec![UsageMetricView { metric: "agents".to_string(), current: 0, limit: max_agents, percent_used: 0 }];
    Ok(Json(json!({ "ok": true, "data": &usage, "usage": usage })))
}

/// `GET /api/billing/invoices` — list invoices (paginated).
async fn list_invoices(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let invoices: Vec<_> = service
        .list_invoices(&auth.scope, query.limit, query.offset)
        .await?
        .into_iter()
        .map(InvoiceView::from)
        .collect();
    Ok(Json(json!({ "ok": true, "data": &invoices, "invoices": invoices })))
}

/// `POST /api/billing/webhook` — Stripe webhook (no auth, uses Stripe signature verification).
async fn stripe_webhook(State(state): State<AppState>, headers: HeaderMap, body: String) -> AppResult<Json<Value>> {
    let signature =
        headers.get("stripe-signature").and_then(|value| value.to_str().ok()).ok_or(ErrorKind::Unauthorized)?;
    let service = make_service(&state);
    service.handle_webhook(&body, signature).await?;
    Ok(Json(json!({ "ok": true, "received": true })))
}

/// Build billing routes sub-router.
///
/// Auth-protected routes are grouped together, while the webhook route
/// is separate and does NOT require authentication.
pub fn billing_routes() -> Router<AppState> {
    Router::new()
        .route("/plans", get(list_plans))
        .route("/subscription", get(get_subscription))
        .route("/checkout", post(create_checkout))
        .route("/portal", post(create_portal))
        .route("/subscribe", post(subscribe))
        .route("/cancel", post(cancel_subscription))
        .route("/subscription/cancel", post(cancel_subscription_v2))
        .route("/subscription/resume", post(resume_subscription))
        .route("/usage", get(get_usage))
        .route("/invoices", get(list_invoices))
}

/// Webhook route — separate from auth-protected routes.
pub fn billing_webhook_routes() -> Router<AppState> {
    Router::new().route("/webhook", post(stripe_webhook))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn subscribe_request_deserialization() {
        let id = Uuid::now_v7();
        let json_str = format!(r#"{{"plan_id": "{}"}}"#, id);
        let req: SubscribeRequest = serde_json::from_str(&json_str).unwrap();
        assert_eq!(req.plan_id, id);
    }

    #[test]
    fn subscribe_request_missing_plan_id_fails() {
        let result = serde_json::from_str::<SubscribeRequest>("{}");
        assert!(result.is_err());
    }
}
