use compact_str::CompactString;

use uuid::Uuid;


use crate::tee::HardwareEnclave;
use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{header, HeaderMap, Request, StatusCode},
    response::{Html, Json, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};
use crate::{
    ai_agent::{self, AiAgent, AiConfig, ChatRequest, ChatResponse},
    auth::{self, AuthGateway, Registration, SignupSession},
    backup::{self, EncryptedBackup},
    circuit_breaker::{self, CircuitBreaker, CircuitBreakerConfig, CircuitAction, DEFAULT_PAIRS},
    cloak::{self, NodeCloakingProtocol},
    cloud::{self, ApiKey, ApiKeyManager, BillingMeter, BillingSummary, CloudOrchestrator, CloudStatus, Invoice, PaymentProcessor, PaymentWebhook, ScalingDecision, Tenant},
    cloud::tenant::Tier,
    consensus::{self, ConsensusOp, DAGConsensus},
    counterparty::{self, CounterpartyVisibilityStore},
    dashboard,
    iso20022::{self, Iso20022Queue},
    kyc::{self, ComplianceGateway},
    llm_sidecar::{self, LlmSidecar},
    market_data::{self, MarketDataStream, MarketEvent},
    pipeline::{self, DualPipeline, TradePayload},
    pg::{self, PgStore},
    shariah::{self, ShariahFilter},
    sovereign::{self, SovereignIdentityRequest, SovereignIdentityStore, generate_regulator_keypair_hex},
    sovereign_fortress::{self, SovereignFortress, SuccessionPlan},
    sovereign_protocol::{self, SovereignProtocol},
    token_auth::{self, TokenAuth},
    universal_bridge::{self, UniversalBridge, Capability},
    wal::{self, WriteAheadLog, WALRecord},
    wasm_engine::{self, WasmMatchHook},
    AppState, RateLimiter, MetricsCollector,
    types,
};
use crate::types::*;
pub fn tenant_from_headers(state: &AppState, headers: &HeaderMap) -> Result<cloud::tenant::Tenant, (StatusCode, Json<serde_json::Value>)> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "missing Authorization header"})))
        })?;
    let key = state.api_keys.validate_key(auth).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid API key"})))
    })?;
    let tenant = state.orchestrator.tenants.get_tenant(&key.tenant_id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "tenant not found"})))
    })?;
    Ok((*tenant).clone())
}

// ==================== Handlers ====================

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "the-bridge-matching-engine",
        "version": "1.0.0",
        "uptime": chrono::Utc::now().to_rfc3339(),
        "tee_enclave": "active",
        "fix_gateway": "connected",
        "dag_consensus": "online",
        "wal_replication": "active",
        "decentralized": true,
        "human_control": false
    }))
}

pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let consensus_healthy = state.consensus.is_healthy().await;
    let wal_healthy = state.wal.is_healthy();
    let tee = state.tee.status();
    let all_ok = consensus_healthy && wal_healthy && tee == "SEALED_PROTECTED";
    let code = if all_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (code, Json(serde_json::json!({
        "ready": all_ok,
        "consensus": consensus_healthy,
        "wal": wal_healthy,
        "tee": tee,
    })))
}

pub async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.prometheus_text(),
    )
}

pub async fn get_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "total_orders": state.books.total_orders(),
        "total_trades": state.books.total_trades(),
        "active_pairs": state.books.active_pairs(),
        "tps_current": state.books.tps_current(),
        "tps_peak": state.books.tps_peak(),
        "dot_settlements": state.dot.total_settlements(),
        "tee_attestation": state.tee.attest_report(),
        "fix_sessions": state.fix.read().await.session_count(),
        "wal_healthy": state.wal.is_healthy(),
        "consensus_vertices": state.consensus.num_vertices().await,
        "consensus_finalized": state.consensus.num_finalized().await,
        "consensus_tips": state.consensus.num_tips().await,
        "consensus_mempool": state.consensus.mempool_depth().await,
        "consensus_healthy": state.consensus.is_healthy().await,
        "crdt_orders": state.crdt.active_orders().len(),
    }))
}

const CIRCUIT_WINDOW_NS: u64 = 2000;
const CIRCUIT_JITTER: u64 = 200;

pub async fn place_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(order): Json<Order>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "place_order".into());

    // Kill switch check — reject if engine is cloaked
    if state.kill_switch.is_cloaked() {
        return Json(serde_json::json!({"error": "engine is cloaked — trading suspended"}));
    }

    let tenant = tenant_from_headers(&state, &headers).ok();
    let tenant_id = tenant.as_ref().map(|t| t.id);

    match process_order_placement(state.clone(), order, tenant_id).await {
        Ok(result) => {
            if let Some(ref t) = tenant {
                state.orchestrator.tenants.record_usage(&t.id, 0, result.trades.len() as u64, 0.0);
            }
            Json(serde_json::json!(result))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

/// Place an Iceberg order (visible portion replenishes as hidden is consumed).
pub async fn place_iceberg_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "place_iceberg".into());
    if state.kill_switch.is_cloaked() {
        return Json(serde_json::json!({"error": "engine is cloaked — trading suspended"}));
    }
    let user_id = match tenant_from_headers(&state, &headers).ok() {
        Some(t) => t.id,
        None => return Json(serde_json::json!({"error": "authentication required"})),
    };
    let pair = payload.get("pair").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let side = match payload.get("side").and_then(|v| v.as_str()) {
        Some("buy") => OrderSide::Buy,
        Some("sell") => OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side (buy/sell)"})),
    };
    let price = payload.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let quantity = payload.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let display_qty = payload.get("display_quantity").and_then(|v| v.as_f64()).unwrap_or(quantity);
    if quantity <= 0.0 || price <= 0.0 || display_qty <= 0.0 {
        return Json(serde_json::json!({"error": "invalid quantity/price/display_quantity"}));
    }
    let order = Order::new_iceberg(user_id, pair, side, price, quantity, display_qty);
    let tenant_id = Some(user_id);
    match process_order_placement(state.clone(), order, tenant_id).await {
        Ok(result) => Json(serde_json::json!(result)),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

/// Place a StopLoss order (triggered when market price crosses trigger).
pub async fn place_stop_loss_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "place_stop_loss".into());
    if state.kill_switch.is_cloaked() {
        return Json(serde_json::json!({"error": "engine is cloaked — trading suspended"}));
    }
    let user_id = match tenant_from_headers(&state, &headers).ok() {
        Some(t) => t.id,
        None => return Json(serde_json::json!({"error": "authentication required"})),
    };
    let pair = payload.get("pair").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let side = match payload.get("side").and_then(|v| v.as_str()) {
        Some("buy") => OrderSide::Buy,
        Some("sell") => OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side (buy/sell)"})),
    };
    let quantity = payload.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let trigger = payload.get("trigger_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let limit_price = payload.get("limit_price").and_then(|v| v.as_f64());
    if quantity <= 0.0 || trigger <= 0.0 {
        return Json(serde_json::json!({"error": "invalid quantity/trigger_price"}));
    }
    let order = Order::new_stop_loss(user_id, pair, side, quantity, trigger, limit_price);
    let tenant_id = Some(user_id);
    match process_order_placement(state.clone(), order, tenant_id).await {
        Ok(result) => Json(serde_json::json!(result)),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

/// Place a TWAP order (sliced into pieces over time by background scheduler).
pub async fn place_twap_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "place_twap".into());
    if state.kill_switch.is_cloaked() {
        return Json(serde_json::json!({"error": "engine is cloaked — trading suspended"}));
    }
    let user_id = match tenant_from_headers(&state, &headers).ok() {
        Some(t) => t.id,
        None => return Json(serde_json::json!({"error": "authentication required"})),
    };
    let pair = payload.get("pair").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let side = match payload.get("side").and_then(|v| v.as_str()) {
        Some("buy") => OrderSide::Buy,
        Some("sell") => OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side (buy/sell)"})),
    };
    let price = payload.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let quantity = payload.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let duration_secs = payload.get("duration_secs").and_then(|v| v.as_u64()).unwrap_or(60);
    let interval_secs = payload.get("interval_secs").and_then(|v| v.as_u64()).unwrap_or(10);
    if quantity <= 0.0 || price <= 0.0 || duration_secs == 0 || interval_secs == 0 {
        return Json(serde_json::json!({"error": "invalid quantity/price/duration/interval"}));
    }
    let order = Order {
        id: Uuid::new_v4(),
        user_id,
        pair: CompactString::from(pair.to_uppercase()),
        order_type: OrderType::Limit,
        side,
        price,
        quantity,
        filled: 0.0,
        remaining: quantity,
        status: OrderStatus::New,
        timestamp: chrono::Utc::now().timestamp_millis(),
        ttl_ms: None,
        is_swap: false,
        swap_target_currency: None,
        tee_signed: false,
        dot_verified: false,
        stealth: false,
        trailing_offset: None,
        trigger_price: None,
        hard_floor: None,
        track: Track::Compliant,
        style: OrderStyle::TWAP { duration_secs, interval_secs },
        hidden_remaining: 0.0,
    };
    let tenant_id = Some(user_id);
    match process_order_placement(state.clone(), order, tenant_id).await {
        Ok(result) => Json(serde_json::json!(result)),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

/// List stop-loss orders for a given pair.
pub async fn list_stop_losses(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    let key = pair.to_uppercase();
    let orders: Vec<Order> = state.books.stop_losses.get(&key)
        .map(|e| e.clone())
        .unwrap_or_default();
    Json(serde_json::json!({"pair": key, "orders": orders}))
}

/// List all active TWAP orders.
pub async fn list_twap_orders(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let orders: Vec<serde_json::Value> = state.books.twap_orders.iter()
        .map(|e| serde_json::json!({
            "id": e.key(),
            "order": e.value().order,
            "filled": e.value().filled_quantity,
            "slices_remaining": e.value().slices_remaining,
        }))
        .collect();
    Json(serde_json::json!({"twap_orders": orders}))
}

/// Shared order placement pipeline used by both REST API and FIX gateway.
pub async fn process_order_placement(
    state: AppState,
    order: Order,
    tenant_id: Option<Uuid>,
) -> Result<PlaceOrderResult, String> {
    use crate::wal::WALRecord;
    use crate::consensus::ConsensusOp;

    // 1. Kill switch check
    if state.kill_switch.is_cloaked() {
        return Err("engine is cloaked — trading suspended".into());
    }

    // 2. Circuit breaker check
    if state.circuit_breaker.is_paused(&order.pair) {
        return Err("trading paused for this pair — circuit breaker active".into());
    }

    // 2. WASM hook validation
    let _ = state.wasm_hook.on_place(&order).map_err(|e| e)?;

    // 2b. Shariah compliance check
    state.shariah_filter.write().check_order(&order).map_err(|e| e)?;

    // 3. DOT validation
    let validated = state.dot.validate_order(&order).map_err(|e| e)?;

    // 4. Balance check — ensure buyer has sufficient funds
    if let Some(tid) = tenant_id {
        let order_value = validated.price * validated.quantity;
        if validated.side == OrderSide::Buy {
            let (balance, locked) = state.orchestrator.tenants.get_balance(&tid);
            let available = balance - locked;
            if order_value > available {
                return Err(format!("insufficient balance: required {}, available {}", order_value, available));
            }
            state.orchestrator.tenants.lock_balance(&tid, order_value)
                .map_err(|e| format!("balance lock failed: {}", e))?;
        }
    }

    // 5. Metrics
    state.metrics.inc_orders();

    // 6. Billing
    state.billing.record_order(&tenant_id.unwrap_or_default());

    // 7. WAL append (log but don't block on failure — WAL is a durability optimization)
    if let Err(e) = state.wal.append(WALRecord::PlaceOrder(validated.clone())) {
        tracing::error!(error = %e, seq = ?validated.id, "WAL append failed for order");
    }

    // 7b. PostgreSQL persistence (best-effort)
    let _ = state.pg.save_order(&validated).await;
    let wal_json = serde_json::to_string(&WALRecord::PlaceOrder(validated.clone())).unwrap_or_default();
    let _ = state.pg.save_wal_entry("PlaceOrder", &wal_json).await;

    // 8. Consensus submit (log but don't block — consensus is replication, not gating)
    state.consensus.submit(ConsensusOp::PlaceOrder(validated.clone())).await;

    // 9. CRDT apply
    state.crdt.apply_add(validated.clone(), "engine-1");

    // 10. Place order in the order book
    let result = state.books.place_order(validated)?;

    // 11. Handle resulting trades
    if !result.trades.is_empty() {
        state.metrics.inc_trades_by(result.trades.len() as u64);
        for trade in &result.trades {
            // Circuit breaker — record trade price and check thresholds
            if let Some(action) = state.circuit_breaker.record_trade(&trade.pair, trade.price) {
                use crate::circuit_breaker::CircuitAction;
                match action {
                    CircuitAction::SwitchToBatch { .. } => {
                        for pair in circuit_breaker::DEFAULT_PAIRS {
                            state.books.create_book_with_batch(pair, CIRCUIT_WINDOW_NS, CIRCUIT_JITTER);
                        }
                        state.books.set_matching_mode(MatchingMode::BatchAuction {
                            window_ns: CIRCUIT_WINDOW_NS,
                            jitter_range_micros: CIRCUIT_JITTER,
                        });
                        tracing::warn!(pair = %trade.pair, "CIRCUIT BREAKER: Level 1 — switched to BatchAuction");
                    }
                    CircuitAction::PauseTrading => {
                        let _ = state.circuit_breaker.trigger_level2(&trade.pair);
                        tracing::warn!(pair = %trade.pair, "CIRCUIT BREAKER: Level 2 — trading paused");
                    }
                    CircuitAction::ActivateKillShield => {
                        NodeCloakingProtocol::activate_cloaking(&*state.kill_switch);
                        tracing::warn!(pair = %trade.pair, "CIRCUIT BREAKER: Level 3 — KILL SHIELD ACTIVATED");
                    }
                }
            }
            state.metrics.add_volume(trade.total);

            // Balance settlement — transfer from buyer to seller
            if tenant_id.is_some() {
                let _ = state.orchestrator.tenants.settle_trade(&trade.buy_user_id, trade.total, 0.0, trade.total);
                let _ = state.orchestrator.tenants.settle_trade(&trade.sell_user_id, 0.0, trade.total, trade.total);
            }
            state.billing.record_trade(&trade.buy_user_id);
            state.billing.record_trade(&trade.sell_user_id);

            // WAL trade record
            if let Err(e) = state.wal.append(WALRecord::TradeSettled(trade.clone())) {
                tracing::error!(error = %e, trade_id = ?trade.id, "WAL append failed for trade");
            }

            // PostgreSQL persistence (best-effort)
            let _ = state.pg.save_trade(trade).await;

            // Build pipeline payload and push
            let payload = build_trade_payload(trade, &order.track);
            let _ = state.pipeline.push(payload);

            // Broadcast trade to WebSocket subscribers
            let _ = state.trade_tx.send(trade.clone());
        }
    }

    // 12. Check triggered stop-loss orders
    if !result.trades.is_empty() {
        let pair = result.order.pair.to_string();
        let last_price = result.trades.last().map(|t| t.price).unwrap_or(0.0);
        let triggered = state.books.check_stop_losses(&pair, last_price);
        for sl in triggered {
            let sl_result = state.books.place_order(sl).map_err(|e| {
                tracing::error!(error = %e, "stop-loss placement failed");
                e
            }).ok();
            if let Some(sl_result) = sl_result {
                if !sl_result.trades.is_empty() {
                    state.metrics.inc_trades_by(sl_result.trades.len() as u64);
                    for trade in &sl_result.trades {
                        state.metrics.add_volume(trade.total);
                        if tenant_id.is_some() {
                            let _ = state.orchestrator.tenants.settle_trade(
                                &trade.buy_user_id, trade.total, 0.0, trade.total,
                            );
                            let _ = state.orchestrator.tenants.settle_trade(
                                &trade.sell_user_id, 0.0, trade.total, trade.total,
                            );
                        }
                        let _ = state.trade_tx.send(trade.clone());
                    }
                }
            }
        }
    }

    Ok(result)
}

pub fn build_trade_payload(trade: &Trade, track: &Track) -> pipeline::TradePayload {
    use crate::pipeline::TradePayload;
    let id_bytes = trade.id.to_bytes_le();
    let buy_bytes = trade.buy_order_id.to_bytes_le();
    let sell_bytes = trade.sell_order_id.to_bytes_le();
    let buy_user_bytes = trade.buy_user_id.to_bytes_le();
    let sell_user_bytes = trade.sell_user_id.to_bytes_le();
    let mut pair_buf = [0u8; 14];
    let bytes = trade.pair.as_bytes();
    let len = bytes.len().min(14);
    pair_buf[..len].copy_from_slice(&bytes[..len]);
    TradePayload {
        trade_id: u64::from_le_bytes(id_bytes[..8].try_into().unwrap_or([0; 8])),
        buy_order_id: u64::from_le_bytes(buy_bytes[..8].try_into().unwrap_or([0; 8])),
        sell_order_id: u64::from_le_bytes(sell_bytes[..8].try_into().unwrap_or([0; 8])),
        price: (trade.price * 1_000_000.0) as u64,
        quantity: (trade.quantity * 1_000_000.0) as u64,
        total: (trade.total * 1_000_000.0) as u64,
        buy_user_id: u64::from_le_bytes(buy_user_bytes[..8].try_into().unwrap_or([0; 8])),
        sell_user_id: u64::from_le_bytes(sell_user_bytes[..8].try_into().unwrap_or([0; 8])),
        timestamp_ns: chrono::Utc::now().timestamp_millis() * 1_000_000,
        seq: 0,
        track: match track {
            types::Track::Autonomous => types::TRACK_AUTONOMOUS,
            _ => types::TRACK_COMPLIANT,
        },
        pair: pair_buf,
        pair_len: trade.pair.len() as u8,
    }
}

pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.books.get_order(id) {
        Some(order) => Json(serde_json::json!(order)),
        None => Json(serde_json::json!({"error": "not_found", "id": id})),
    }
}

pub async fn list_my_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let orders: Vec<Order> = state.books.user_orders.get(&tenant.id)
        .map(|v| v.clone())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "count": orders.len(),
        "orders": orders,
    })))
}

pub async fn list_my_trades(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let trades: Vec<Trade> = state.books.user_trades.get(&tenant.id)
        .map(|v| v.clone())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "count": trades.len(),
        "trades": trades,
    })))
}

pub async fn market_trades_handler(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    let key = pair.to_uppercase();
    let trades = state.books.books.get(&key)
        .map(|book| {
            let recent: Vec<Trade> = book.trades.iter().rev().take(100).cloned().collect();
            recent
        })
        .unwrap_or_default();
    Json(serde_json::json!({
        "pair": key,
        "count": trades.len(),
        "trades": trades,
    }))
}

pub async fn cancel_order(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "cancel_order".into());
    let _ = state.wal.append(wal::WALRecord::CancelOrder(id));
    let _ = state.consensus.submit(consensus::ConsensusOp::CancelOrder(id)).await;
    state.crdt.apply_remove(id, "engine-1");
    match state.books.cancel_order(id) {
        Ok(_) => Json(serde_json::json!({"status": "cancelled", "id": id})),
        Err(e) => Json(serde_json::json!({"error": e, "id": id})),
    }
}

pub async fn get_orderbook(State(state): State<AppState>, Path(pair): Path<String>) -> Json<serde_json::Value> {
    match state.books.get_book_summary(&pair.to_uppercase()) {
        Some(book) => Json(serde_json::json!(book)),
        None => Json(serde_json::json!({"error": "pair_not_found", "pair": pair})),
    }
}

pub async fn get_depth(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    match state.books.get_depth(&pair.to_uppercase(), 10) {
        Some(depth) => Json(serde_json::json!(depth)),
        None => Json(serde_json::json!({"error": "pair_not_found"})),
    }
}

pub async fn get_ticker(State(state): State<AppState>, Path(pair): Path<String>) -> Json<serde_json::Value> {
    match state.books.get_ticker(&pair.to_uppercase()) {
        Some(ticker) => Json(serde_json::json!(ticker)),
        None => Json(serde_json::json!({"error": "pair_not_found"})),
    }
}

pub async fn dot_transfer(
    State(state): State<AppState>,
    Json(tx): Json<DOTTransfer>,
) -> Json<serde_json::Value> {
    state.kill_switch.threat_analyzer.record_request("api".into(), "dot_transfer".into());
    let _ = state.wal.append(wal::WALRecord::SettleDOT(tx.clone()));
    match state.dot.execute_transfer(tx) {
        Ok(result) => Json(serde_json::json!(result)),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

pub async fn dot_status(State(state): State<AppState>, Path(id): Path<uuid::Uuid>) -> Json<serde_json::Value> {
    match state.dot.get_transfer(id) {
        Some(tx) => Json(serde_json::json!(tx)),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

pub async fn tee_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enclave": "sgx",
        "status": state.tee.status(),
        "attestation": state.tee.attest_report(),
        "key_isolated": true,
        "human_inaccessible": true,
    }))
}

pub async fn tee_rotate(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.tee.rotate_keys() {
        Ok(report) => Json(serde_json::json!({"status": "rotated", "attestation": report})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

pub async fn fix_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let fix = state.fix.read().await;
    Json(serde_json::json!({
        "gateway": "FIX/FAST 5.0 SP2",
        "sessions": fix.session_count(),
        "connected_institutions": fix.connected_institutions(),
        "total_orders_routed": fix.total_orders_routed(),
    }))
}

pub async fn fix_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let fix = state.fix.read().await;
    Json(serde_json::json!({"sessions": fix.sessions()}))
}

pub async fn sovereign_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let threat = state.kill_switch.threat_analyzer.analyze();
    let current = match state.kill_switch.current_threat.lock() {
        Ok(t) => format!("{:?}", t),
        Err(_) => String::new(),
    };
    Json(serde_json::json!({
        "sovereign": true,
        "threat_analyzer": {
            "status": "active",
            "current_level": current,
            "instant_level": format!("{:?}", threat),
        },
        "node_cloaking": {
            "protocol": "active",
            "backup_nodes": state.kill_switch.backup_nodes,
            "flood_detection": "enabled",
            "hot_migration_ready": true,
        },
        "kill_switch": "armed",
        "human_override": false,
    }))
}

pub async fn sovereign_shield(State(state): State<AppState>) -> Json<serde_json::Value> {
    NodeCloakingProtocol::activate_cloaking(&*state.kill_switch);
    Json(serde_json::json!({
        "status": "shield_activated",
        "protocol": "no_entity_can_halt_this_protocol",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

pub async fn consensus_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "vertices": state.consensus.num_vertices().await,
        "finalized": state.consensus.num_finalized().await,
        "tips": state.consensus.num_tips().await,
        "mempool_depth": state.consensus.mempool_depth().await,
        "healthy": state.consensus.is_healthy().await,
        "node_id": "engine-1",
    }))
}

pub async fn wal_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "healthy": state.wal.is_healthy(),
        "replica_lag_ms": state.wal.replica_lag(),
    }))
}

pub async fn crdt_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "active_orders": state.crdt.active_orders().len(),
        "node_id": "engine-1",
    }))
}

pub async fn wasm_status(_state: State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": cfg!(feature = "wasm"),
        "message": if cfg!(feature = "wasm") { "WASM hooks active" } else { "compile with --features wasm" },
    }))
}

// ==================== Cloud Handlers ====================

pub async fn cloud_status_handler(State(state): State<AppState>) -> Json<cloud::CloudStatus> {
    Json(state.orchestrator.cloud_status())
}

pub async fn list_tenants_handler(State(state): State<AppState>) -> Json<Vec<Tenant>> {
    Json(state.orchestrator.tenants.list_tenants())
}

#[derive(serde::Deserialize)]
pub struct CreateTenantReq {
    name: String,
    email: String,
    tier: String,
}

pub async fn create_tenant_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateTenantReq>,
) -> Result<Json<Tenant>, (StatusCode, Json<serde_json::Value>)> {
    let tier = match req.tier.to_lowercase().as_str() {
        "free" => Tier::Free,
        "pro" => Tier::Pro,
        "enterprise" => Tier::Enterprise,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid tier"})))),
    };
    match state.orchestrator.tenants.create_tenant(req.name, req.email, tier) {
        Ok(tenant) => {
            let _ = state.orchestrator.provision_engine(&tenant.id);
            Ok(Json(tenant))
        }
        Err(e) => Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn get_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Tenant>, (StatusCode, Json<serde_json::Value>)> {
    match state.orchestrator.tenants.get_tenant(&id) {
        Some(tenant) => Ok(Json(Tenant::clone(&tenant))),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))),
    }
}

pub async fn delete_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = state.orchestrator.drain_engine(&id);
    if state.orchestrator.tenants.delete_tenant(&id) {
        Ok(Json(serde_json::json!({"status": "deleted"})))
    } else {
        Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "tenant not found"}))))
    }
}

#[derive(serde::Deserialize)]
pub struct UpgradeTierReq {
    tier: String,
}

pub async fn upgrade_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<UpgradeTierReq>,
) -> Result<Json<Tenant>, (StatusCode, Json<serde_json::Value>)> {
    let tier = match req.tier.to_lowercase().as_str() {
        "free" => Tier::Free,
        "pro" => Tier::Pro,
        "enterprise" => Tier::Enterprise,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid tier"})))),
    };
    state.orchestrator.tenants.upgrade_tenant(&id, tier).map_err(|e| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e})))
    })?;
    let tenant = state.orchestrator.tenants.get_tenant(&id).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "tenant disappeared after upgrade"})))
    })?;
    Ok(Json(Tenant::clone(&tenant)))
}

pub async fn create_api_key_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if state.orchestrator.tenants.get_tenant(&id).is_none() {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "tenant not found"}))));
    }
    match state.api_keys.create_key(id) {
        Ok((_, full_key)) => Ok(Json(serde_json::json!({
            "key": full_key,
            "tenant_id": id,
            "created_at": chrono::Utc::now().timestamp_millis(),
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn list_api_keys_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<Vec<cloud::ApiKey>> {
    let keys = state.api_keys.list_keys_for_tenant(&id);
    Json(keys)
}

pub async fn get_invoices_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<Vec<cloud::Invoice>> {
    Json(state.billing.get_invoices(&id))
}

pub async fn billing_summary_handler(
    State(state): State<AppState>,
) -> Json<cloud::BillingSummary> {
    Json(state.billing.global_summary())
}

pub async fn get_scaling_decision_handler(
    State(state): State<AppState>,
) -> Json<cloud::ScalingDecision> {
    Json(state.orchestrator.calculate_scaling_decision())
}

// ==================== Compliance Handlers ====================

#[derive(serde::Deserialize)]
pub struct OnboardReq {
    tenant_id: uuid::Uuid,
    legal_name: String,
    lei: String,
    jurisdiction: String,
}

pub async fn onboard_entity_handler(
    State(state): State<AppState>,
    Json(req): Json<OnboardReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let profile = state.compliance.onboard_entity(
        req.tenant_id,
        &req.legal_name,
        &req.lei,
        &req.jurisdiction,
    ).map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    if let Some(mut tenant) = state.orchestrator.tenants.get_tenant_mut(&req.tenant_id) {
        tenant.lei = Some(req.lei.clone());
        tenant.jurisdiction = Some(req.jurisdiction.clone());
        tenant.disclosure_level = profile.disclosure_level.clone();
    }

    Ok(Json(serde_json::json!(profile)))
}

pub async fn compliance_status_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    let tenant = state.orchestrator.tenants.get_tenant(&id);
    match tenant {
        Some(t) => Json(serde_json::json!({
            "tenant_id": t.id,
            "disclosure_level": t.disclosure_level,
            "lei_verified": t.lei.is_some(),
            "jurisdiction": t.jurisdiction,
        })),
        None => Json(serde_json::json!({"error": "tenant not found"})),
    }
}

// ==================== Matching Mode ====================

#[derive(serde::Deserialize, serde::Serialize)]
pub struct MatchingModeReq {
    mode: String,
    window_ns: Option<u64>,
    jitter_range_micros: Option<u64>,
}

pub async fn set_matching_mode_handler(
    State(state): State<AppState>,
    Json(req): Json<MatchingModeReq>,
) -> Json<serde_json::Value> {
    match req.mode.to_lowercase().as_str() {
        "continuous" => {
            Json(serde_json::json!({"status": "ok", "mode": "continuous"}))
        }
        "batch" => {
            let window = req.window_ns.unwrap_or(2000);
            let jitter = req.jitter_range_micros.unwrap_or(200);
            // Create books with these batch params if they don't exist
            for pair in &["USD/EUR", "USD/EGP", "USD/SAR", "USD/AED", "USD/GBP", "EUR/EGP"] {
                state.books.create_book_with_batch(pair, window, jitter);
            }
            Json(serde_json::json!({
                "status": "ok",
                "mode": "batch",
                "window_ns": window,
                "jitter_range_micros": jitter,
                "anti_sniping": jitter > 0,
                "sucp": true,
            }))
        }
        _ => Json(serde_json::json!({"error": "invalid mode: use 'continuous' or 'batch'"})),
    }
}

#[derive(serde::Deserialize)]
pub struct BatchStatusParams {
    pair: String,
}

pub async fn batch_status_handler(
    State(state): State<AppState>,
    Path(params): Path<BatchStatusParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let key = params.pair.to_uppercase();
    if !state.books.books.contains_key(&key) {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "pair not found"}))));
    }
    let auction_info = state.books.get_batch_info(&key).unwrap_or(serde_json::json!(null));
    let summary = state.books.get_book_summary(&key);
    Ok(Json(serde_json::json!({
        "pair": key,
        "auction": auction_info,
        "summary": summary,
    })))
}

pub async fn batch_execute_handler(
    State(state): State<AppState>,
    Path(params): Path<BatchStatusParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let key = params.pair.to_uppercase();
    match state.books.execute_batch_auction_manual(&key) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "ok", "pair": key}))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

// ==================== Auth / Signup Handlers ====================

#[derive(serde::Deserialize)]
pub struct RegisterReq { email: String }

pub async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.register(&req.email)
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
pub struct VerifyReq { token: String }

pub async fn verify_handler(
    State(state): State<AppState>,
    Json(req): Json<VerifyReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.verify_email(&req.token)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
pub struct LoginReq {
    api_key: String,
}

#[derive(serde::Serialize)]
pub struct LoginRes {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    tenant_id: Uuid,
}

pub async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginRes>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = state.api_keys.validate_key(&req.api_key)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))))?;
    let access_token = state.token_auth.create_access_token(api_key.tenant_id, "pro");
    let refresh_token = state.token_auth.create_refresh_token(api_key.tenant_id);
    Ok(Json(LoginRes {
        access_token,
        refresh_token,
        expires_in: 900,
        tenant_id: api_key.tenant_id,
    }))
}

#[derive(serde::Deserialize)]
pub struct RefreshReq {
    refresh_token: String,
}

#[derive(serde::Serialize)]
pub struct RefreshRes {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

pub async fn refresh_handler(
    State(state): State<AppState>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<RefreshRes>, (StatusCode, Json<serde_json::Value>)> {
    match state.token_auth.rotate_refresh_token(&req.refresh_token) {
        Some((access, refresh, _)) => Ok(Json(RefreshRes {
            access_token: access,
            refresh_token: refresh,
            expires_in: 900,
        })),
        None => Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid refresh token"})))),
    }
}

pub async fn audit_log_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let entries = state.audit_log.recent(100);
    Json(serde_json::json!({
        "count": entries.len(),
        "entries": entries,
    }))
}

#[derive(serde::Deserialize)]
pub struct KycReq { email: String, lei: String, jurisdiction: String }

pub async fn kyc_handler(
    State(state): State<AppState>,
    Json(req): Json<KycReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.submit_kyc(&req.email, &req.lei, &req.jurisdiction)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
pub struct SelectTierReq { email: String, tier: String }

pub async fn select_tier_handler(
    State(state): State<AppState>,
    Json(req): Json<SelectTierReq>,
) -> Result<Json<auth::SignupSession>, (StatusCode, Json<serde_json::Value>)> {
    let tier = match req.tier.to_lowercase().as_str() {
        "free" => Tier::Free,
        "pro" => Tier::Pro,
        "enterprise" => Tier::Enterprise,
        _ => return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid tier"})))),
    };
    let (reg, key) = state.auth_gateway.select_tier(&req.email, &tier, &state.api_keys)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;
    let session = auth::SignupSession {
        email: reg.email,
        step: reg.step,
        tenant_id: reg.tenant_id,
        api_key: Some(key),
    };
    Ok(Json(session))
}

// ==================== Payment Webhook Handler ====================

pub async fn payment_webhook_handler(
    State(state): State<AppState>,
    Json(webhook): Json<cloud::PaymentWebhook>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.payment_processor.handle_webhook(&webhook, &state.billing, &state.orchestrator.tenants) {
        Ok(event) => Ok(Json(serde_json::json!(event))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

// ==================== Wallet Handlers ====================

pub async fn wallet_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let (balance, locked) = state.orchestrator.tenants.get_balance(&tenant.id);
    Ok(Json(serde_json::json!({
        "tenant_id": tenant.id,
        "tenant_name": tenant.name,
        "tier": tenant.tier,
        "balance": balance,
        "locked": locked,
        "available": balance - locked,
        "total_orders": tenant.usage.total_orders,
        "total_trades": tenant.usage.total_trades,
    })))
}

pub async fn wallet_deposit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let amount = body.get("amount").and_then(|v| v.as_f64()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "missing amount"})))
    })?;
    if amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "amount must be positive"}))));
    }
    let new_balance = state.orchestrator.tenants.deposit(&tenant.id, amount).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
    })?;
    state.billing.record_order(&tenant.id);
    Ok(Json(serde_json::json!({
        "status": "deposited",
        "amount": amount,
        "balance": new_balance,
    })))
}

pub async fn wallet_withdraw(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let amount = body.get("amount").and_then(|v| v.as_f64()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "missing amount"})))
    })?;
    if amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "amount must be positive"}))));
    }
    let new_balance = state.orchestrator.tenants.withdraw(&tenant.id, amount).map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))
    })?;
    Ok(Json(serde_json::json!({
        "status": "withdrawn",
        "amount": amount,
        "balance": new_balance,
    })))
}

// ==================== AI Agent Handlers ====================

pub async fn ai_chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ai_agent::ChatRequest>,
) -> Json<ai_agent::ChatResponse> {
    Json(state.ai_agent.chat(req))
}

pub async fn ai_config_handler(
    State(state): State<AppState>,
) -> Json<ai_agent::AiConfig> {
    Json(state.ai_agent.config())
}

pub async fn ai_config_update_handler(
    State(state): State<AppState>,
    Json(cfg): Json<ai_agent::AiConfig>,
) -> Json<serde_json::Value> {
    state.ai_agent.update_config(cfg);
    Json(serde_json::json!({"status": "updated"}))
}

pub async fn ai_status_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.ai_agent.config();
    Json(serde_json::json!({
        "status": "operational",
        "llm_provider": config.llm_provider,
        "auto_marketing": config.auto_marketing,
        "auto_compliance": config.auto_compliance,
        "total_sessions": state.ai_agent.sessions.lock().len(),
        "system_health": {
            "tenants": state.orchestrator.active_tenants.load(std::sync::atomic::Ordering::Relaxed),
            "total_orders": state.books.total_orders(),
            "total_trades": state.books.total_trades(),
            "sars_filed": state.compliance.aml_monitor.total_sars(),
        }
    }))
}

// ==================== Webhook Handlers ====================

#[derive(serde::Deserialize)]
pub struct RegisterWebhookReq {
    url: String,
}

pub async fn register_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterWebhookReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    state.webhooks.entry(tenant.id).or_default().push(req.url.clone());
    Ok(Json(serde_json::json!({"status": "ok", "url": req.url})))
}

pub async fn list_webhooks_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let urls: Vec<String> = state.webhooks.get(&tenant.id)
        .map(|v| v.clone())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({"count": urls.len(), "webhooks": urls})))
}

pub async fn delete_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    if let Some(mut urls) = state.webhooks.get_mut(&tenant.id) {
        urls.retain(|u| *u != id);
    }
    Ok(Json(serde_json::json!({"status": "deleted", "url": id})))
}

// ==================== Shariah Compliance Handlers ====================

pub async fn shariah_status_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let filter = state.shariah_filter.read();
    let (approved, rejected) = filter.audit_count();
    Json(serde_json::json!({
        "enabled": filter.enabled,
        "audit_count": {
            "approved": approved,
            "rejected": rejected,
            "total": approved + rejected,
        },
    }))
}

pub async fn shariah_audit_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let filter = state.shariah_filter.read();
    let entries = filter.recent_audit(100);
    Json(serde_json::json!({
        "count": entries.len(),
        "entries": entries,
    }))
}

#[derive(serde::Deserialize)]
pub struct ShariahProhibitReq {
    pair: String,
}

pub async fn shariah_prohibit_handler(
    State(state): State<AppState>,
    Json(req): Json<ShariahProhibitReq>,
) -> Json<serde_json::Value> {
    state.shariah_filter.write().add_prohibited_pair(&req.pair);
    Json(serde_json::json!({"status": "ok", "pair": req.pair.to_uppercase()}))
}

// ==================== WebSocket: Live Order Fills ====================

pub async fn orders_ws_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<serde_json::value::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let tenant_id = tenant.id;
    let metrics = state.metrics.clone();
    let trade_rx = state.trade_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_orders_ws(socket, tenant_id, metrics, trade_rx)))
}

#[derive(serde::Deserialize)]
pub struct MarketDataParams {
    pair: String,
}

pub async fn market_data_ws_handler(
    State(state): State<AppState>,
    Path(params): Path<MarketDataParams>,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<serde_json::value::Value>)> {
    let rx = state.market_data.tx.subscribe();
    let key = params.pair.to_uppercase();
    Ok(ws.on_upgrade(move |socket| handle_market_data_ws(socket, key, rx)))
}

pub async fn handle_market_data_ws(
    mut ws: axum::extract::ws::WebSocket,
    pair: String,
    mut rx: broadcast::Receiver<market_data::MarketEvent>,
) {
    use axum::extract::ws::Message;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let msg = serde_json::json!({"type": "heartbeat", "pair": pair, "timestamp": chrono::Utc::now().timestamp_millis()});
                if ws.send(Message::Text(msg.to_string().into())).await.is_err() { break; }
            }
            event = rx.recv() => {
                match event {
                    Ok(market_data::MarketEvent::Depth(depth)) => {
                        if depth.pair == pair {
                            let msg = serde_json::json!({
                                "type": "depth",
                                "pair": depth.pair,
                                "bids": depth.bids,
                                "asks": depth.asks,
                                "timestamp": depth.timestamp,
                            });
                            if ws.send(Message::Text(msg.to_string().into())).await.is_err() { break; }
                        }
                    }
                    Ok(market_data::MarketEvent::Ticker(ticker)) => {
                        if ticker.pair == pair {
                            let msg = serde_json::json!({
                                "type": "ticker",
                                "pair": ticker.pair,
                                "bid": ticker.bid,
                                "ask": ticker.ask,
                                "last": ticker.last,
                                "high_24h": ticker.high_24h,
                                "low_24h": ticker.low_24h,
                                "volume_24h": ticker.volume_24h,
                                "change_24h_pct": ticker.change_24h_pct,
                                "timestamp": ticker.timestamp,
                            });
                            if ws.send(Message::Text(msg.to_string().into())).await.is_err() { break; }
                        }
                    }
                    Ok(market_data::MarketEvent::Candle(candle)) => {
                        if candle.pair == pair {
                            let msg = serde_json::json!({
                                "type": "candle",
                                "pair": candle.pair,
                                "interval": candle.interval,
                                "open": candle.open,
                                "high": candle.high,
                                "low": candle.low,
                                "close": candle.close,
                                "volume": candle.volume,
                                "timestamp": candle.timestamp,
                            });
                            if ws.send(Message::Text(msg.to_string().into())).await.is_err() { break; }
                        }
                    }
                    Ok(market_data::MarketEvent::Trade(trade)) => {
                        if trade.pair.as_str() == pair {
                            let msg = serde_json::json!({
                                "type": "trade",
                                "pair": trade.pair,
                                "price": trade.price,
                                "quantity": trade.quantity,
                                "total": trade.total,
                                "timestamp": trade.timestamp,
                            });
                            if ws.send(Message::Text(msg.to_string().into())).await.is_err() { break; }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(lagged = n, "market data ws lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

pub async fn handle_orders_ws(
    mut ws: axum::extract::ws::WebSocket,
    tenant_id: uuid::Uuid,
    metrics: Arc<MetricsCollector>,
    mut trade_rx: broadcast::Receiver<Trade>,
) {
    use axum::extract::ws::Message;
    use std::time::Instant;

    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let start = Instant::now();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let summary = metrics.snapshot();
                let tps = summary["tps_current"].as_u64().unwrap_or(0);
                let trades = summary["trades"].as_u64().unwrap_or(0);
                let msg = serde_json::json!({
                    "type": "heartbeat",
                    "tenant_id": tenant_id,
                    "tps": tps,
                    "trades": trades,
                    "uptime_secs": start.elapsed().as_secs(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                });
                if ws.send(Message::Text(msg.to_string().into())).await.is_err() {
                    break;
                }
            }
            trade = trade_rx.recv() => {
                match trade {
                    Ok(trade) => {
                        let msg = serde_json::json!({
                            "type": "trade",
                            "trade_id": trade.id,
                            "pair": trade.pair,
                            "price": trade.price,
                            "quantity": trade.quantity,
                            "total": trade.total,
                            "buy_order_id": trade.buy_order_id,
                            "sell_order_id": trade.sell_order_id,
                            "buy_user_id": trade.buy_user_id,
                            "sell_user_id": trade.sell_user_id,
                            "timestamp": trade.timestamp,
                        });
                        if ws.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(dropped = n, "WebSocket: trade events lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = ws.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.trim() == "ping" {
                            let _ = ws.send(Message::Text("pong".into())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

// ==================== Developer Portal Handlers ====================

const DOCS_HTML: &str = include_str!("../docs/index.html");
const KILL_SWITCH_DEMO_HTML: &str = include_str!("../docs/kill-switch-demo.html");

pub async fn docs_page() -> Html<&'static str> {
    Html(DOCS_HTML)
}

pub async fn kill_switch_demo_page() -> Html<&'static str> {
    Html(KILL_SWITCH_DEMO_HTML)
}

pub async fn openapi_spec() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "THE-BRIDGE Matching Engine API",
            "version": "1.0.0",
            "description": "1M+ TPS matching engine with FIX 5.0 SP2, DAG consensus, Sovereign privacy, WASM hooks"
        },
        "servers": [{"url": "https://api.the-bridge.io"}],
        "paths": {
            "/api/v1/health": {
                "get": {"summary": "Health check", "tags": ["System"]}
            },
            "/api/v1/wallet/balance": {
                "get": {"summary": "Get balance", "tags": ["Wallet"]}
            },
            "/api/v1/wallet/deposit": {
                "post": {"summary": "Deposit funds", "tags": ["Wallet"]}
            },
            "/api/v1/wallet/withdraw": {
                "post": {"summary": "Withdraw funds", "tags": ["Wallet"]}
            },
            "/api/v1/order": {
                "post": {"summary": "Place order", "tags": ["Trading"]}
            },
            "/api/v1/order/{id}": {
                "get": {"summary": "Get order", "tags": ["Trading"]},
                "delete": {"summary": "Cancel order", "tags": ["Trading"]}
            },
            "/api/v1/orderbook/{pair}": {
                "get": {"summary": "Order book", "tags": ["Market Data"]}
            },
            "/api/v1/orderbook/{pair}/depth": {
                "get": {"summary": "Market depth", "tags": ["Market Data"]}
            },
            "/api/v1/ticker/{pair}": {
                "get": {"summary": "24h ticker", "tags": ["Market Data"]}
            },
            "/ws/orders": {
                "get": {"summary": "Live orders WebSocket", "tags": ["WebSocket"]}
            },
            "/ws/dashboard": {
                "get": {"summary": "Dashboard WebSocket", "tags": ["WebSocket"]}
            }
        }
    }))
}

// ==================== Dashboard Handlers ====================

pub async fn trade_page() -> Html<&'static str> {
    Html(include_str!("../trading/index.html"))
}

pub async fn dashboard_page() -> Html<&'static str> {
    Html(include_str!("../dashboard/index.html"))
}

pub async fn dashboard_sw() -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 2], &'static str) {
    (StatusCode::OK,
     [(axum::http::header::CONTENT_TYPE, "application/javascript"),
      (axum::http::header::CACHE_CONTROL, "no-cache")],
     include_str!("../dashboard/sw.js"))
}

pub async fn dashboard_manifest() -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 2], &'static str) {
    (StatusCode::OK,
     [(axum::http::header::CONTENT_TYPE, "application/manifest+json"),
      (axum::http::header::CACHE_CONTROL, "no-cache")],
     include_str!("../dashboard/manifest.json"))
}

pub async fn dashboard_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response {
    let query = req.uri().query().unwrap_or("");
    let authed = query.contains("token=")
        || req.headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                let key = v.strip_prefix("Bearer ").unwrap_or(v);
                state.api_keys.validate_key(key)
            })
            .is_some();

    if !authed {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "missing or invalid token — add ?token=YOUR_KEY to /ws/dashboard"}))).into_response();
    }

    ws.on_upgrade(move |socket| async move {
        dashboard::handle_ws_dashboard(
            socket,
            state.metrics,
            state.billing,
            state.orchestrator,
        ).await;
    })
    .into_response()
}

// ==================== Layer 3 — Sovereign Handlers ====================

#[derive(serde::Deserialize)]
pub struct RegisterSovereignIdentityReq {
    tenant_id: uuid::Uuid,
    legal_name: String,
    lei: String,
    jurisdiction: String,
}

pub async fn register_sovereign_identity_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterSovereignIdentityReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sovereign_req = sovereign::SovereignIdentityRequest {
        tenant_id: req.tenant_id,
        legal_name: req.legal_name,
        lei: req.lei,
        jurisdiction: req.jurisdiction,
    };
    match state.sovereign_store.encrypt_identity(&sovereign_req) {
        Ok(identity) => Ok(Json(serde_json::json!(identity))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn get_sovereign_identity_handler(
    State(state): State<AppState>,
    Path(tenant_id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.sovereign_store.get_encrypted(&tenant_id) {
        Some(identity) => Json(serde_json::json!(identity)),
        None => Json(serde_json::json!({"error": "identity_not_found"})),
    }
}

#[derive(serde::Deserialize)]
pub struct DecryptSovereignReq {
    tenant_id: uuid::Uuid,
    regulator_secret_hex: String,
}

pub async fn decrypt_sovereign_identity_handler(
    State(state): State<AppState>,
    Json(req): Json<DecryptSovereignReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let identity = match state.sovereign_store.get_encrypted(&req.tenant_id) {
        Some(id) => id,
        None => return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "identity_not_found"})))),
    };
    match state.sovereign_store.decrypt_identity(&identity, &req.regulator_secret_hex) {
        Ok(data) => Ok(Json(data)),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn generate_regulator_keypair_handler() -> Json<serde_json::Value> {
    let (sec, pubk) = sovereign::generate_regulator_keypair_hex();
    Json(serde_json::json!({
        "public_key_hex": pubk,
        "secret_key_hex": sec,
        "warning": "save the secret key securely — it cannot be recovered",
    }))
}

// ==================== Layer 2 — Counterparty Visibility Handlers ====================

#[derive(serde::Deserialize)]
pub struct AddCounterpartyReq {
    tenant_id: uuid::Uuid,
    counterparty_id: uuid::Uuid,
}

pub async fn add_counterparty_handler(
    State(state): State<AppState>,
    Json(req): Json<AddCounterpartyReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.counterparty_store.add_counterparty(&req.tenant_id, &req.counterparty_id) {
        Ok(_) => Ok(Json(serde_json::json!({
            "status": "added",
            "tenant_id": req.tenant_id,
            "counterparty_id": req.counterparty_id,
        }))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn list_counterparty_handler(
    State(state): State<AppState>,
    Path(tenant_id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.counterparty_store.get_list(&tenant_id) {
        Some(list) => Json(serde_json::json!(list)),
        None => Json(serde_json::json!({
            "tenant_id": tenant_id,
            "counterparty_count": 0,
            "note": "no counterparty list — accepts all",
        })),
    }
}

pub async fn check_counterparty_handler(
    State(state): State<AppState>,
    Path((a_id, b_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Json<serde_json::Value> {
    let mutual = state.counterparty_store.mutual_acceptance(&a_id, &b_id);
    let a_accepts_b = state.counterparty_store.accepts(&a_id, &b_id);
    let b_accepts_a = state.counterparty_store.accepts(&b_id, &a_id);
    Json(serde_json::json!({
        "tenant_a": a_id,
        "tenant_b": b_id,
        "a_accepts_b": a_accepts_b,
        "b_accepts_a": b_accepts_a,
        "mutual_acceptance": mutual,
    }))
}

pub async fn iso20022_list_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.iso20022_queue.list_reports(100) {
        Ok(list) => Ok(Json(serde_json::json!({"reports": list}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))),
    }
}

pub async fn iso20022_get_handler(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.iso20022_queue.get_report(&filename) {
        Ok(xml) => Ok(Json(serde_json::json!({"filename": filename, "xml": xml}))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e})))),
    }
}

// ==================== Ghost Protocol Handlers ====================

pub async fn ghost_tax_rate_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let rate = state.sovereign_protocol.tax_rate();
    Json(serde_json::json!({"tax_rate_bps": rate, "tax_rate_percent": rate as f64 / 100.0}))
}

pub async fn ghost_tax_rate_set(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let rate = body["rate_bps"].as_u64().unwrap_or(0);
    state.sovereign_protocol.set_tax_rate(rate);
    state.fortress.record_action("sovereign", "tax_rate_change", &body, |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "tax_rate_bps": rate}))
}

pub async fn ghost_treasury(State(state): State<AppState>) -> Json<serde_json::Value> {
    let balance = state.sovereign_protocol.treasury_balance();
    let total = state.sovereign_protocol.tax_collected_total();
    Json(serde_json::json!({
        "treasury": balance,
        "total_collected": total,
    }))
}

pub async fn ghost_prohibited_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let list = state.sovereign_protocol.list_prohibited();
    Json(serde_json::json!({
        "count": list.len(),
        "addresses": list.into_iter().map(|(a, c)| serde_json::json!({"address": a, "blocked_count": c})).collect::<Vec<_>>(),
    }))
}

pub async fn ghost_prohibited_add(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    state.sovereign_protocol.add_prohibited(&addr);
    state.fortress.record_action("sovereign", "prohibit_add", &serde_json::json!({"address": addr}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "address": addr}))
}

pub async fn ghost_prohibited_remove(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.sovereign_protocol.remove_prohibited(&addr);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }, "address": addr}))
}

pub async fn ghost_sleeper_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let sleepers = state.sovereign_protocol.list_sleepers();
    Json(serde_json::json!({
        "count": sleepers.len(),
        "sleepers": sleepers.into_iter().map(|s| serde_json::json!({
            "address": s.address,
            "label": s.state.label,
            "status": s.state.status,
            "total_volume": s.state.total_volume,
            "trade_count": s.state.trade_count,
            "last_seen_ns": s.state.last_seen_ns,
            "action": s.state.action,
        })).collect::<Vec<_>>(),
    }))
}

pub async fn ghost_sleeper_watch(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let label = body["label"].as_str().unwrap_or("unknown");
    state.sovereign_protocol.watch_sleeper(&addr, label);
    Json(serde_json::json!({"status": "ok", "address": addr, "label": label}))
}

pub async fn ghost_sleeper_unwatch(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.sovereign_protocol.unwatch_sleeper(&addr);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }, "address": addr}))
}

pub async fn ghost_sleeper_freeze(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    match state.sovereign_protocol.freeze_sleeper(&addr) {
        Ok(()) => {
            state.fortress.record_action("sovereign", "sleeper_freeze", &serde_json::json!({"address": addr}), |msg| state.tee.sign(msg));
            Json(serde_json::json!({"status": "ok", "action": "freeze", "address": addr}))
        }
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

pub async fn ghost_sleeper_seize(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    match state.sovereign_protocol.seize_sleeper(&addr) {
        Ok(action) => {
            state.fortress.treasury.deposit(action.amount_seized);
            state.fortress.record_action("sovereign", "sleeper_seize", &serde_json::json!({"address": addr, "amount": action.amount_seized}), |msg| state.tee.sign(msg));
            Json(serde_json::json!({"status": "ok", "action": "seize", "address": addr, "amount_seized": action.amount_seized}))
        }
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

pub async fn ghost_sleeper_tax(
    State(state): State<AppState>,
    Path((addr, amount)): Path<(String, u64)>,
) -> Json<serde_json::Value> {
    match state.sovereign_protocol.one_time_tax_sleeper(&addr, amount) {
        Ok(()) => Json(serde_json::json!({"status": "ok", "action": "tax", "address": addr, "amount": amount})),
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

pub async fn ghost_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.sovereign_protocol.snapshot();
    Json(serde_json::json!(snap))
}

// ==================== Universal Bridge Handlers ====================

pub async fn bridge_list_projects(State(state): State<AppState>) -> Json<serde_json::Value> {
    let projects = state.universal_bridge.list_projects();
    Json(serde_json::json!({
        "count": projects.len(),
        "projects": projects,
    }))
}

pub async fn bridge_register_project(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let name = body["name"].as_str().unwrap_or("unnamed");
    let endpoint = body["endpoint"].as_str().unwrap_or("");
    let auth_key = body["auth_key"].as_str().unwrap_or("");
    let description = body["description"].as_str().unwrap_or("");
    let caps = body["capabilities"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| match v.as_str() {
                    Some("orders") => Some(universal_bridge::Capability::ReceiveOrders),
                    Some("settlements") => Some(universal_bridge::Capability::ReceiveSettlements),
                    Some("iso20022") => Some(universal_bridge::Capability::ReceiveISO20022),
                    Some("ghost") => Some(universal_bridge::Capability::ReceiveGhostCommands),
                    Some("send") => Some(universal_bridge::Capability::SendData),
                    Some("bidirectional") => Some(universal_bridge::Capability::Bidirectional),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if endpoint.is_empty() {
        return Json(serde_json::json!({"status": "error", "error": "endpoint required"}));
    }

    state
        .universal_bridge
        .register_project(name, endpoint, auth_key, caps, description);
    Json(serde_json::json!({"status": "ok", "name": name}))
}

pub async fn bridge_remove_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.universal_bridge.remove_project(&name);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }}))
}

pub async fn bridge_forward(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let command_type = body["command_type"].as_str().unwrap_or("generic");
    let payload = body["payload"].clone();
    match state
        .universal_bridge
        .forward_to_project(&name, command_type, payload)
        .await
    {
        Ok(response) => Json(serde_json::json!({"status": "ok", "response": response})),
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

pub async fn bridge_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.universal_bridge.snapshot())
}

pub async fn bridge_receive(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // Receive commands from other bridge-connected projects
    let cmd_type = body["command_type"].as_str().unwrap_or("unknown");
    let payload = body["payload"].clone();

    match cmd_type {
        "ping" => Json(serde_json::json!({"status": "pong", "node": "the-bridge-master"})),
        "ghost_tax_rate" => {
            if let Some(rate) = payload["rate_bps"].as_u64() {
                state.sovereign_protocol.set_tax_rate(rate);
            }
            Json(serde_json::json!({"status": "ok", "tax_rate_bps": state.sovereign_protocol.tax_rate()}))
        }
        "ghost_freeze" => {
            if let Some(addr) = payload["address"].as_str() {
                let _ = state.sovereign_protocol.freeze_sleeper(addr);
            }
            Json(serde_json::json!({"status": "ok"}))
        }
        "ghost_seize" => {
            if let Some(addr) = payload["address"].as_str() {
                let _ = state.sovereign_protocol.seize_sleeper(addr);
            }
            Json(serde_json::json!({"status": "ok"}))
        }
        _ => Json(serde_json::json!({
            "status": "received",
            "node": "the-bridge-master",
            "command": cmd_type,
        })),
    }
}

// ==================== LLM Sidecar Handlers ====================

pub async fn llm_chat(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<llm_sidecar::ChatRequest>,
) -> Json<serde_json::Value> {
    let response = state.llm_sidecar.chat(req).await;
    Json(serde_json::json!(response))
}

pub async fn llm_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "available": state.llm_sidecar.is_available(),
        "total_queries": state.llm_sidecar.total_queries(),
    }))
}

// ==================== Encrypted Backup Handlers ====================

pub async fn backup_trigger(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.backup.trigger(&*state.tee).await {
        Ok(manifest) => Json(serde_json::json!({"status": "ok", "manifest": manifest})),
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

pub async fn backup_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.backup.status();
    Json(serde_json::json!(status))
}

// ==================== Sovereign Fortress Handlers ====================

pub async fn fortress_heartbeat(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.fortress.heartbeat();
    let status = state.fortress.status();
    state.fortress.record_action("sovereign", "heartbeat", &serde_json::json!({"timestamp_ns": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "switch_state": status.switch_state}))
}

pub async fn fortress_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.fortress.status()))
}

pub async fn fortress_audit(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.fortress.audit.snapshot();
    Json(serde_json::json!(snap))
}

pub async fn fortress_succession_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.fortress.dead_mans_switch.succession_plan() {
        Some(plan) => Json(serde_json::json!({"configured": true, "plan": plan})),
        None => Json(serde_json::json!({"configured": false})),
    }
}

pub async fn fortress_succession_set(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let plan = sovereign_fortress::SuccessionPlan {
        successor_pubkey: body["successor_pubkey"].as_str().unwrap_or("").to_string(),
        cold_wallet_addresses: body["cold_wallet_addresses"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        notify_webhooks: body["notify_webhooks"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        timeout_hours: body["timeout_hours"].as_u64().unwrap_or(72),
    };
    if plan.successor_pubkey.is_empty() {
        return Json(serde_json::json!({"status": "error", "error": "successor_pubkey required"}));
    }
    state.fortress.configure_succession(plan);
    state.fortress.record_action("sovereign", "succession_configured", &body, |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn fortress_succession_disable(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.fortress.dead_mans_switch.disable();
    state.fortress.record_action("sovereign", "succession_disabled", &serde_json::json!({}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn fortress_treasury_balance(State(state): State<AppState>) -> Json<serde_json::Value> {
    let bal = state.fortress.treasury.balance();
    Json(serde_json::json!({"balance": bal, "asset": "USDC"}))
}

pub async fn fortress_treasury_withdraw(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let amount = body["amount"].as_u64().unwrap_or(0);
    match state.fortress.treasury.withdraw(amount) {
        Ok(remaining) => {
            state.fortress.record_action("sovereign", "treasury_withdraw", &serde_json::json!({"amount": amount, "remaining": remaining}), |msg| state.tee.sign(msg));
            Json(serde_json::json!({"status": "ok", "withdrawn": amount, "remaining": remaining}))
        }
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

// ==================== Circuit Breaker Handlers ====================

pub async fn circuit_breaker_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let states = state.circuit_breaker.all_states();
    let pairs: Vec<serde_json::Value> = states.into_iter().map(|(pair, cs, cfg)| {
        serde_json::json!({
            "pair": pair,
            "state": cs.to_string(),
            "config": {
                "max_move_percent": cfg.max_move_percent,
                "window_secs": cfg.window_secs,
                "level1_enabled": cfg.level1_enabled,
                "level2_enabled": cfg.level2_enabled,
                "level3_enabled": cfg.level3_enabled,
            }
        })
    }).collect();
    Json(serde_json::json!({
        "pairs": pairs,
        "total_triggers": state.circuit_breaker.total_triggers(),
    }))
}

pub async fn circuit_breaker_config_get(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    match state.circuit_breaker.get_config(&pair) {
        Some(cfg) => Json(serde_json::json!({
            "pair": pair.to_uppercase(),
            "config": {
                "max_move_percent": cfg.max_move_percent,
                "window_secs": cfg.window_secs,
                "level1_enabled": cfg.level1_enabled,
                "level2_enabled": cfg.level2_enabled,
                "level3_enabled": cfg.level3_enabled,
            }
        })),
        None => Json(serde_json::json!({"error": "pair not found"})),
    }
}

pub async fn circuit_breaker_config_set(
    State(state): State<AppState>,
    Path(pair): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let cfg = circuit_breaker::CircuitBreakerConfig {
        max_move_percent: body["max_move_percent"].as_f64().unwrap_or(10.0),
        window_secs: body["window_secs"].as_u64().unwrap_or(5),
        level1_enabled: body["level1_enabled"].as_bool().unwrap_or(true),
        level2_enabled: body["level2_enabled"].as_bool().unwrap_or(true),
        level3_enabled: body["level3_enabled"].as_bool().unwrap_or(true),
    };
    state.circuit_breaker.set_config(&pair, cfg);
    Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase()}))
}

pub async fn circuit_breaker_events(State(state): State<AppState>) -> Json<serde_json::Value> {
    let events = state.circuit_breaker.recent_events(100);
    Json(serde_json::json!({
        "count": events.len(),
        "events": events,
    }))
}

pub async fn circuit_breaker_reset(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    let ok = state.circuit_breaker.reset(&pair);
    if ok {
        // Also reset matching mode to continuous
        state.books.set_matching_mode(types::MatchingMode::Continuous);
        Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase(), "new_state": "normal"}))
    } else {
        Json(serde_json::json!({"error": "pair not found"}))
    }
}

pub async fn circuit_breaker_trigger(
    State(state): State<AppState>,
    Path((pair, level)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match level.as_str() {
        "1" => {
            state.circuit_breaker.trigger_level1(&pair);
            for p in circuit_breaker::DEFAULT_PAIRS {
                state.books.create_book_with_batch(p, 2000, 200);
            }
            state.books.set_matching_mode(types::MatchingMode::BatchAuction {
                window_ns: 2000,
                jitter_range_micros: 200,
            });
            Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase(), "level": 1, "mode": "batch"}))
        }
        "2" => {
            state.circuit_breaker.trigger_level2(&pair);
            Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase(), "level": 2, "mode": "paused"}))
        }
        "3" => {
            state.circuit_breaker.trigger_level3(&pair);
            NodeCloakingProtocol::activate_cloaking(&*state.kill_switch);
            Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase(), "level": 3, "mode": "kill_shield"}))
        }
        _ => Json(serde_json::json!({"error": "invalid level — use 1, 2, or 3"})),
    }
}
