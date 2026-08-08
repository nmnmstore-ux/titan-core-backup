use std::sync::Arc;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use rust_decimal::prelude::ToPrimitive;
use tracing::instrument;

use crate::config::FiatConfig;
use crate::stripe::StripeClient;
use crate::banxa::BanxaClient;
use crate::cards::CardManager;
use crate::compliance::ComplianceChecker;
use crate::FiatError;

#[derive(Clone)]
pub struct AppState {
    pub config: FiatConfig,
    pub stripe: Arc<StripeClient>,
    pub banxa: Arc<BanxaClient>,
    pub cards: Arc<CardManager>,
    pub compliance: Arc<ComplianceChecker>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/deposit", post(deposit_handler))
        .route("/withdraw", post(withdraw_handler))
        .route("/card/provision", post(provision_card_handler))
        .route("/card/status/:card_id", get(card_status_handler))
        .route("/card/activate", post(activate_card_handler))
        .route("/history", get(history_handler))
        .route("/webhook/stripe", post(stripe_webhook_handler))
        .route("/webhook/banxa", post(banxa_webhook_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DepositRequestPayload {
    user_id: String,
    amount: String,
    currency: String,
    method: String,
    destination_wallet: String,
    chain_id: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct DepositResponsePayload {
    deposit_id: String,
    external_reference: String,
    status: String,
    net_amount: String,
    fees: String,
    payment_url: Option<String>,
}

#[instrument(skip(state))]
async fn deposit_handler(
    State(state): State<AppState>,
    Json(req): Json<DepositRequestPayload>,
) -> impl axum::response::IntoResponse {
    let amount = match req.amount.parse::<rust_decimal::Decimal>() {
        Ok(v) => v,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "invalid amount"),
    };

    if state.config.kyc_required {
        if let Err(e) = state.compliance.check_kyc(&req.user_id).await {
            return error_response(StatusCode::FORBIDDEN, &e.to_string());
        }
    }

    let amount_str = amount.to_string();
    let banxa_response = state.banxa.create_order(
        &req.user_id,
        &amount_str,
        &req.currency,
        "USDT",
        "buy",
    ).await;

    match banxa_response {
        Ok(order) => {
            Json(serde_json::json!({
                "deposit_id": order.order_id,
                "external_reference": order.id,
                "status": order.status,
                "amount": order.amount,
                "currency": order.currency,
            }))
            .into_response()
        }
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WithdrawRequestPayload {
    user_id: String,
    amount: String,
    currency: String,
    method: String,
    source_wallet: String,
    chain_id: String,
    bank_account_id: Option<String>,
    idempotency_key: String,
}

async fn withdraw_handler(
    State(state): State<AppState>,
    Json(req): Json<WithdrawRequestPayload>,
) -> impl axum::response::IntoResponse {
    let amount = match req.amount.parse::<rust_decimal::Decimal>() {
        Ok(v) => v,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "invalid amount"),
    };

    if state.config.kyc_required {
        match state.compliance.full_check(&req.user_id, &req.source_wallet, amount, &req.currency).await {
            Ok(_) => {},
            Err(FiatError::KycNotVerified) => return error_response(StatusCode::FORBIDDEN, "kyc not verified"),
            Err(FiatError::LimitExceeded) => return error_response(StatusCode::TOO_MANY_REQUESTS, "limit exceeded"),
            Err(FiatError::Sanctioned) => return error_response(StatusCode::FORBIDDEN, "sanctioned address"),
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }
    }

    let payout = state.stripe.create_payout(
        (amount.to_f64().unwrap_or_default() * 100.0) as i64,
        &req.currency.to_lowercase(),
        &req.source_wallet,
        None,
    ).await;

    match payout {
        Ok(p) => Json(serde_json::json!({
            "withdrawal_id": p.id,
            "status": p.status,
            "amount": p.amount,
            "currency": p.currency,
        })).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CardProvisionRequest {
    user_id: String,
    wallet_address: String,
    chain_id: String,
    initial_load: String,
    currency: String,
    contactless: bool,
    idempotency_key: String,
}

async fn provision_card_handler(
    State(state): State<AppState>,
    Json(req): Json<CardProvisionRequest>,
) -> impl axum::response::IntoResponse {
    if state.config.kyc_required {
        match state.compliance.check_kyc(&req.user_id).await {
            Ok(_) => {},
            Err(_) => return error_response(StatusCode::FORBIDDEN, "kyc not verified"),
        }
    }

    match state.cards.provision_card(
        &req.user_id,
        &req.wallet_address,
        &req.chain_id,
        &req.currency,
        &req.idempotency_key,
    ).await {
        Ok(card) => Json(serde_json::json!({
            "card_id": card.id,
            "masked_pan": card.masked_pan,
            "token": card.token,
            "status": card.status.as_str(),
            "expires_at": card.expires_at,
        })).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

async fn card_status_handler(
    State(state): State<AppState>,
    axum::extract::Path(card_id): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    match state.cards.get_card(&card_id).await {
        Some(card) => Json(serde_json::json!({
            "card_id": card.id,
            "masked_pan": card.masked_pan,
            "brand": card.brand,
            "currency": card.currency,
            "status": card.status.as_str(),
            "expires_at": card.expires_at,
            "is_active": card.is_active(),
        })).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "card not found"),
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ActivateCardRequest {
    card_id: String,
    user_id: String,
}

async fn activate_card_handler(
    State(state): State<AppState>,
    Json(req): Json<ActivateCardRequest>,
) -> impl axum::response::IntoResponse {
    match state.cards.activate_card(&req.card_id, &req.user_id).await {
        Ok(card) => Json(serde_json::json!({
            "success": true,
            "card_id": card.id,
            "status": card.status.as_str(),
        })).into_response(),
        Err(FiatError::NotFound(_)) => error_response(StatusCode::NOT_FOUND, "card not found"),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HistoryRequest {
    user_id: String,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn history_handler(
    State(_state): State<AppState>,
    Json(req): Json<HistoryRequest>,
) -> impl axum::response::IntoResponse {
    Json(serde_json::json!({
        "transactions": [],
        "total": 0,
        "page": req.page.unwrap_or(1),
        "page_size": req.page_size.unwrap_or(20),
    })).into_response()
}

async fn stripe_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl axum::response::IntoResponse {
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match crate::webhooks::verify_stripe_signature(
        body.as_bytes(),
        signature,
        &state.config.stripe_webhook_secret,
        None,
    ) {
        Ok(true) => {
            tracing::info!("stripe webhook verified");
            (StatusCode::ACCEPTED, "OK").into_response()
        }
        Ok(false) => {
            tracing::warn!("stripe webhook signature mismatch");
            error_response(StatusCode::UNAUTHORIZED, "signature verification failed")
        }
        Err(e) => {
            error_response(StatusCode::BAD_REQUEST, &e.to_string())
        }
    }
}

async fn banxa_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl axum::response::IntoResponse {
    let signature = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let timestamp_str = headers
        .get("x-timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("0");

    let timestamp = timestamp_str.parse::<u64>().unwrap_or(0);

    match crate::webhooks::verify_banxa_signature(
        body.as_bytes(),
        signature,
        &state.config.banxa_webhook_secret,
        timestamp,
    ) {
        Ok(true) => {
            tracing::info!("banxa webhook verified");
            (StatusCode::ACCEPTED, "OK").into_response()
        }
        Ok(false) => {
            error_response(StatusCode::UNAUTHORIZED, "signature verification failed")
        }
        Err(e) => {
            error_response(StatusCode::BAD_REQUEST, &e.to_string())
        }
    }
}

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    let body = serde_json::json!({
        "success": false,
        "error": message,
    });
    (status, Json(body)).into_response()
}

#[allow(dead_code)]
pub fn build_state(config: FiatConfig) -> AppState {
    let stripe = Arc::new(StripeClient::new(config.clone()));
    let banxa = Arc::new(BanxaClient::new(config.clone()));
    let cards = Arc::new(CardManager::new(config.clone()));
    let compliance = Arc::new(ComplianceChecker::new(config.clone()));
    AppState {
        config,
        stripe,
        banxa,
        cards,
        compliance,
    }
}
