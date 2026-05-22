use std::collections::BTreeMap;
use std::sync::Arc;

use agentforge_core::{AppConfig, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::domain::billing::{
    BillingStripeGatewayPolicy, StripeEvent, StripeInvoiceSnapshot, StripeSubscriptionSnapshot,
};

type HmacSha256 = Hmac<Sha256>;

const STRIPE_API_BASE: &str = "https://api.stripe.com";
const WEBHOOK_TOLERANCE_SECONDS: i64 = 300;

#[derive(Debug, Clone)]
pub struct CheckoutSessionInput {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub user_email: String,
    pub plan_id: Uuid,
    pub price_id: String,
    pub billing_cycle: String,
    pub success_url: String,
    pub cancel_url: String,
}

#[derive(Debug, Clone)]
pub struct DirectSubscriptionInput {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub user_email: String,
    pub plan_id: Uuid,
    pub price_id: String,
    pub payment_method_id: String,
}

#[derive(Debug, Clone)]
pub struct CheckoutSession {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct PortalSession {
    pub url: String,
}

#[async_trait]
pub trait BillingGateway: Send + Sync {
    fn is_configured(&self) -> bool;

    async fn create_checkout_session(&self, input: CheckoutSessionInput) -> AppResult<CheckoutSession>;

    async fn create_direct_subscription(&self, input: DirectSubscriptionInput)
    -> AppResult<StripeSubscriptionSnapshot>;

    async fn create_portal_session(&self, customer_id: &str, return_url: &str) -> AppResult<PortalSession>;

    async fn cancel_subscription(
        &self,
        subscription_id: &str,
        immediately: bool,
    ) -> AppResult<StripeSubscriptionSnapshot>;

    async fn resume_subscription(&self, subscription_id: &str) -> AppResult<StripeSubscriptionSnapshot>;

    fn verify_webhook_payload(&self, payload: &str, signature: &str) -> AppResult<Value>;
}

#[derive(Debug, Default)]
pub struct DisabledBillingGateway;

#[async_trait]
impl BillingGateway for DisabledBillingGateway {
    fn is_configured(&self) -> bool {
        false
    }

    async fn create_checkout_session(&self, _input: CheckoutSessionInput) -> AppResult<CheckoutSession> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }

    async fn create_direct_subscription(
        &self,
        _input: DirectSubscriptionInput,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }

    async fn create_portal_session(&self, _customer_id: &str, _return_url: &str) -> AppResult<PortalSession> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }

    async fn cancel_subscription(
        &self,
        _subscription_id: &str,
        _immediately: bool,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }

    async fn resume_subscription(&self, _subscription_id: &str) -> AppResult<StripeSubscriptionSnapshot> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }

    fn verify_webhook_payload(&self, _payload: &str, _signature: &str) -> AppResult<Value> {
        Err(BillingStripeGatewayPolicy::not_configured().into())
    }
}

pub fn billing_gateway_from_config(config: &AppConfig) -> AppResult<Arc<dyn BillingGateway>> {
    if config.stripe.is_configured() {
        Ok(Arc::new(StripeBillingClient::from_config(config)?))
    } else {
        Ok(Arc::new(DisabledBillingGateway))
    }
}

#[derive(Debug, Clone)]
pub struct StripeBillingClient {
    http: Client,
    secret_key: String,
    webhook_secret: String,
    api_base: String,
}

impl StripeBillingClient {
    pub fn from_config(config: &AppConfig) -> AppResult<Self> {
        let secret_key = config
            .stripe
            .stripe_secret_key
            .as_ref()
            .map(|value| value.expose_secret().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(BillingStripeGatewayPolicy::not_configured)?;
        let webhook_secret = config
            .stripe
            .stripe_webhook_secret
            .as_ref()
            .map(|value| value.expose_secret().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(BillingStripeGatewayPolicy::not_configured)?;

        Ok(Self { http: Client::new(), secret_key, webhook_secret, api_base: STRIPE_API_BASE.to_string() })
    }

    async fn request_form<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<T> {
        let url = format!("{}{}", self.api_base, path);
        let response = self
            .http
            .request(method, url)
            .bearer_auth(&self.secret_key)
            .form(&params)
            .send()
            .await
            .map_err(BillingStripeGatewayPolicy::api_request_failed)?;

        parse_stripe_response(response).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let url = format!("{}{}", self.api_base, path);
        let response = self
            .http
            .delete(url)
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(BillingStripeGatewayPolicy::api_request_failed)?;

        parse_stripe_response(response).await
    }
}

#[async_trait]
impl BillingGateway for StripeBillingClient {
    fn is_configured(&self) -> bool {
        true
    }

    async fn create_checkout_session(&self, input: CheckoutSessionInput) -> AppResult<CheckoutSession> {
        let mut params = vec![
            ("mode".to_string(), "subscription".to_string()),
            ("success_url".to_string(), input.success_url),
            ("cancel_url".to_string(), input.cancel_url),
            ("customer_email".to_string(), input.user_email),
            ("client_reference_id".to_string(), input.org_id.to_string()),
            ("line_items[0][price]".to_string(), input.price_id),
            ("line_items[0][quantity]".to_string(), "1".to_string()),
            ("allow_promotion_codes".to_string(), "true".to_string()),
        ];
        push_billing_metadata(&mut params, "metadata", &input.org_id, &input.user_id, &input.plan_id);
        push_billing_metadata(
            &mut params,
            "subscription_data[metadata]",
            &input.org_id,
            &input.user_id,
            &input.plan_id,
        );
        params.push(("subscription_data[metadata][billing_cycle]".to_string(), input.billing_cycle));

        let session: StripeCheckoutSession = self.request_form(Method::POST, "/v1/checkout/sessions", params).await?;
        let url = session
            .url
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(BillingStripeGatewayPolicy::missing_checkout_redirect_url)?;

        Ok(CheckoutSession { id: session.id, url })
    }

    async fn create_direct_subscription(
        &self,
        input: DirectSubscriptionInput,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        let mut customer_params = vec![("email".to_string(), input.user_email)];
        push_billing_metadata(&mut customer_params, "metadata", &input.org_id, &input.user_id, &input.plan_id);
        let customer: StripeCustomer = self.request_form(Method::POST, "/v1/customers", customer_params).await?;

        let payment_method_path = format!("/v1/payment_methods/{}/attach", input.payment_method_id);
        self.request_form::<StripePaymentMethod>(
            Method::POST,
            &payment_method_path,
            vec![("customer".to_string(), customer.id.clone())],
        )
        .await?;

        let customer_path = format!("/v1/customers/{}", customer.id);
        self.request_form::<StripeCustomer>(
            Method::POST,
            &customer_path,
            vec![("invoice_settings[default_payment_method]".to_string(), input.payment_method_id)],
        )
        .await?;

        let mut subscription_params = vec![
            ("customer".to_string(), customer.id),
            ("items[0][price]".to_string(), input.price_id),
            ("payment_behavior".to_string(), "default_incomplete".to_string()),
            ("payment_settings[save_default_payment_method]".to_string(), "on_subscription".to_string()),
        ];
        push_billing_metadata(&mut subscription_params, "metadata", &input.org_id, &input.user_id, &input.plan_id);
        let subscription: StripeSubscriptionApi =
            self.request_form(Method::POST, "/v1/subscriptions", subscription_params).await?;
        Ok(subscription.into_snapshot())
    }

    async fn create_portal_session(&self, customer_id: &str, return_url: &str) -> AppResult<PortalSession> {
        let session: StripePortalSession = self
            .request_form(
                Method::POST,
                "/v1/billing_portal/sessions",
                vec![
                    ("customer".to_string(), customer_id.to_string()),
                    ("return_url".to_string(), return_url.to_string()),
                ],
            )
            .await?;
        Ok(PortalSession { url: session.url })
    }

    async fn cancel_subscription(
        &self,
        subscription_id: &str,
        immediately: bool,
    ) -> AppResult<StripeSubscriptionSnapshot> {
        let path = format!("/v1/subscriptions/{subscription_id}");
        let subscription: StripeSubscriptionApi = if immediately {
            self.delete(&path).await?
        } else {
            self.request_form(Method::POST, &path, vec![("cancel_at_period_end".to_string(), "true".to_string())])
                .await?
        };
        Ok(subscription.into_snapshot())
    }

    async fn resume_subscription(&self, subscription_id: &str) -> AppResult<StripeSubscriptionSnapshot> {
        let path = format!("/v1/subscriptions/{subscription_id}");
        let subscription: StripeSubscriptionApi = self
            .request_form(Method::POST, &path, vec![("cancel_at_period_end".to_string(), "false".to_string())])
            .await?;
        Ok(subscription.into_snapshot())
    }

    fn verify_webhook_payload(&self, payload: &str, signature: &str) -> AppResult<Value> {
        verify_stripe_signature(payload, signature, &self.webhook_secret)?;
        serde_json::from_str(payload).map_err(|err| BillingStripeGatewayPolicy::invalid_webhook_json(err).into())
    }
}

async fn parse_stripe_response<T: DeserializeOwned>(response: reqwest::Response) -> AppResult<T> {
    let status = response.status();
    let body = response.text().await.map_err(BillingStripeGatewayPolicy::api_response_read_failed)?;

    if !status.is_success() {
        let message = stripe_error_message(&body).unwrap_or_else(|| format!("Stripe API returned {status}"));
        return Err(BillingStripeGatewayPolicy::api_error_from_status(status.as_u16(), message).into());
    }

    serde_json::from_str(&body).map_err(|err| BillingStripeGatewayPolicy::api_response_decode_failed(err).into())
}

fn stripe_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<StripeErrorEnvelope>(body).ok().and_then(|envelope| envelope.error.message)
}

fn push_billing_metadata(
    params: &mut Vec<(String, String)>,
    prefix: &str,
    org_id: &Uuid,
    user_id: &Uuid,
    plan_id: &Uuid,
) {
    params.push((format!("{prefix}[org_id]"), org_id.to_string()));
    params.push((format!("{prefix}[user_id]"), user_id.to_string()));
    params.push((format!("{prefix}[plan_id]"), plan_id.to_string()));
}

fn verify_stripe_signature(payload: &str, signature: &str, webhook_secret: &str) -> AppResult<()> {
    let mut timestamp: Option<i64> = None;
    let mut signatures = Vec::new();

    for part in signature.split(',') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "t" => timestamp = value.parse::<i64>().ok(),
            "v1" => signatures.push(value),
            _ => {}
        }
    }

    let timestamp = timestamp.ok_or_else(BillingStripeGatewayPolicy::invalid_webhook_signature)?;
    if (Utc::now().timestamp() - timestamp).abs() > WEBHOOK_TOLERANCE_SECONDS {
        return Err(BillingStripeGatewayPolicy::invalid_webhook_signature().into());
    }
    if signatures.is_empty() {
        return Err(BillingStripeGatewayPolicy::invalid_webhook_signature().into());
    }

    let signed_payload = format!("{timestamp}.{payload}");
    for candidate in signatures {
        let Ok(expected) = hex::decode(candidate) else {
            continue;
        };
        let mut mac = HmacSha256::new_from_slice(webhook_secret.as_bytes())
            .map_err(BillingStripeGatewayPolicy::hmac_init_failed)?;
        mac.update(signed_payload.as_bytes());
        if mac.verify_slice(&expected).is_ok() {
            return Ok(());
        }
    }

    Err(BillingStripeGatewayPolicy::invalid_webhook_signature().into())
}

fn unix_to_datetime(seconds: Option<i64>) -> Option<DateTime<Utc>> {
    seconds.and_then(|value| Utc.timestamp_opt(value, 0).single())
}

pub fn stripe_event(payload: Value) -> AppResult<StripeEvent> {
    serde_json::from_value(payload).map_err(|err| BillingStripeGatewayPolicy::invalid_webhook_event_shape(err).into())
}

pub fn parse_subscription_object(value: Value) -> AppResult<StripeSubscriptionSnapshot> {
    let subscription: StripeSubscriptionApi =
        serde_json::from_value(value).map_err(BillingStripeGatewayPolicy::invalid_subscription_object)?;
    Ok(subscription.into_snapshot())
}

pub fn parse_invoice_object(value: Value) -> AppResult<StripeInvoiceSnapshot> {
    let invoice: StripeInvoiceApi =
        serde_json::from_value(value).map_err(BillingStripeGatewayPolicy::invalid_invoice_object)?;
    Ok(invoice.into_snapshot())
}

#[derive(Debug, Deserialize)]
struct StripeCheckoutSession {
    id: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StripePortalSession {
    url: String,
}

#[derive(Debug, Deserialize)]
struct StripeCustomer {
    id: String,
}

#[derive(Debug, Deserialize)]
struct StripePaymentMethod {
    #[allow(dead_code)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct StripeSubscriptionApi {
    id: String,
    customer: Option<ExpandableId<StripeCustomer>>,
    status: String,
    current_period_start: Option<i64>,
    current_period_end: Option<i64>,
    #[serde(default)]
    cancel_at_period_end: bool,
    canceled_at: Option<i64>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    items: Option<StripeSubscriptionItems>,
}

impl StripeSubscriptionApi {
    fn into_snapshot(self) -> StripeSubscriptionSnapshot {
        let price_id =
            self.items.and_then(|items| items.data.into_iter().find_map(|item| item.price.map(|price| price.id)));

        StripeSubscriptionSnapshot {
            id: self.id,
            customer_id: self.customer.and_then(|customer| customer.id()),
            status: self.status,
            current_period_start: unix_to_datetime(self.current_period_start),
            current_period_end: unix_to_datetime(self.current_period_end),
            cancel_at_period_end: self.cancel_at_period_end,
            canceled_at: unix_to_datetime(self.canceled_at),
            metadata: self.metadata,
            price_id,
        }
    }
}

#[derive(Debug, Deserialize)]
struct StripeSubscriptionItems {
    #[serde(default)]
    data: Vec<StripeSubscriptionItem>,
}

#[derive(Debug, Deserialize)]
struct StripeSubscriptionItem {
    price: Option<StripePrice>,
}

#[derive(Debug, Deserialize)]
struct StripePrice {
    id: String,
}

#[derive(Debug, Deserialize)]
struct StripeInvoiceApi {
    id: String,
    customer: Option<ExpandableId<StripeCustomer>>,
    subscription: Option<ExpandableId<StripeSubscriptionApi>>,
    amount_due: Option<i64>,
    amount_paid: Option<i64>,
    total: Option<i64>,
    currency: Option<String>,
    status: Option<String>,
    status_transitions: Option<StripeInvoiceStatusTransitions>,
    created: i64,
}

impl StripeInvoiceApi {
    fn into_snapshot(self) -> StripeInvoiceSnapshot {
        let amount = self.total.or(self.amount_paid).or(self.amount_due).unwrap_or(0);
        StripeInvoiceSnapshot {
            id: self.id,
            customer_id: self.customer.and_then(|customer| customer.id()),
            subscription_id: self.subscription.and_then(|subscription| subscription.id()),
            amount_cents: i32::try_from(amount).unwrap_or(i32::MAX),
            currency: self.currency.unwrap_or_else(|| "usd".to_string()),
            status: self.status.unwrap_or_else(|| "draft".to_string()),
            paid_at: self.status_transitions.and_then(|transitions| unix_to_datetime(transitions.paid_at)),
            created_at: unix_to_datetime(Some(self.created)).unwrap_or_else(Utc::now),
        }
    }
}

#[derive(Debug, Deserialize)]
struct StripeInvoiceStatusTransitions {
    paid_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExpandableId<T> {
    Id(String),
    Object(T),
}

impl ExpandableId<StripeCustomer> {
    fn id(self) -> Option<String> {
        match self {
            Self::Id(id) => Some(id),
            Self::Object(customer) => Some(customer.id),
        }
    }
}

impl ExpandableId<StripeSubscriptionApi> {
    fn id(self) -> Option<String> {
        match self {
            Self::Id(id) => Some(id),
            Self::Object(subscription) => Some(subscription.id),
        }
    }
}

#[derive(Debug, Deserialize)]
struct StripeErrorEnvelope {
    error: StripeErrorBody,
}

#[derive(Debug, Deserialize)]
struct StripeErrorBody {
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use agentforge_core::ErrorKind;

    use super::*;

    fn signed_header(payload: &str, secret: &str, timestamp: i64) -> String {
        let signed_payload = format!("{timestamp}.{payload}");
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed_payload.as_bytes());
        format!("t={timestamp},v1={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_valid_stripe_signature() {
        let payload = r#"{"id":"evt_test","type":"customer.subscription.updated","data":{"object":{}}}"#;
        let header = signed_header(payload, "whsec_test", Utc::now().timestamp());
        verify_stripe_signature(payload, &header, "whsec_test").expect("signature should verify");
    }

    #[test]
    fn rejects_invalid_stripe_signature() {
        let payload = r#"{"id":"evt_test","type":"customer.subscription.updated","data":{"object":{}}}"#;
        let header = signed_header(payload, "wrong", Utc::now().timestamp());
        let err = verify_stripe_signature(payload, &header, "whsec_test").expect_err("signature should fail");
        assert!(matches!(err.kind, ErrorKind::Unauthorized));
    }

    #[test]
    fn subscription_snapshot_extracts_price_and_metadata() {
        let payload = serde_json::json!({
            "id": "sub_123",
            "customer": "cus_123",
            "status": "active",
            "current_period_start": 1_700_000_000,
            "current_period_end": 1_700_086_400,
            "cancel_at_period_end": true,
            "metadata": {"org_id": Uuid::nil().to_string(), "plan_id": Uuid::nil().to_string()},
            "items": {"data": [{"price": {"id": "price_123"}}]}
        });

        let snapshot = parse_subscription_object(payload).expect("valid subscription object");
        assert_eq!(snapshot.id, "sub_123");
        assert_eq!(snapshot.customer_id.as_deref(), Some("cus_123"));
        assert_eq!(snapshot.price_id.as_deref(), Some("price_123"));
        assert!(snapshot.cancel_at_period_end);
    }
}
