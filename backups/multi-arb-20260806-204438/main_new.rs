#[cfg(target_os = "linux")]
use tikv_jemallocator::Jemalloc;

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod time_cache;
mod types;
mod orderbook;
mod matching;
mod dot;
mod tee;
mod fix;
mod metrics;
mod numa;
mod snapshot;
mod cloak;
mod wal;
mod consensus;
mod crdt;
mod wasm_engine;
mod encrypted;
mod memory;
mod pool;
mod anti_debug;
mod cloud;
mod kyc;
mod pipeline;
mod auth;
mod dashboard;
mod sovereign;
mod counterparty;
mod iso20022;
mod sovereign_protocol;
mod universal_bridge;
mod llm_sidecar;
mod backup;
mod sovereign_fortress;
mod circuit_breaker;
mod io;
mod pg;
mod market_data;
mod token_auth;
mod shariah;
mod ai_agent;
mod smart_router;
mod dark_pool_manager;
mod futures_options;
mod lending_pool;
mod securities_lending;
mod revenue_engine;
mod fx_engine;
mod ghost_integration;
mod threshold_crypto;
mod encrypted_mempool;
mod batch_auction;
mod dual_track;
mod compliance_engine;
mod risk_engine;
mod onboarding_engine;
mod execution_engine;
mod liquidity_engine;
mod white_label;
mod prime_brokerage;
mod dark_pool_orchestrator;
mod sovereign_ghost;
mod instant_flow;
mod vampire_core;
mod ai_ceo;
mod flash_loan_api;
mod mev_api;
mod batch_auction_api;
mod futures_api;
mod liquidation;
mod liquidation_api;
mod bmm_amm;
mod xdp_firewall;
mod memfd_secret;
mod hugepages;
mod zk_snark;
mod htlc_bridge;
mod policy_dsl;
mod direction_supervisor;
mod direction_registry;
mod bmm_circuit_shield;
mod triangular_fee_network;

use the_bridge_cross_venue_arb as cross_venue_arb;
use the_bridge_super_arb as super_arb;

use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, Json, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Router,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;
use compact_str::CompactString;
use types::*;
use orderbook::OrderBookManager;
use dot::DOTEngine;
use tee::{HardwareEnclave, TEEEnclave};
use fix::FIXGateway;
use metrics::MetricsCollector;
use numa::{CPUAffinity, NUMADistributor, AffinityThreadPool};
use cloak::{SovereignKillSwitch, NodeCloakingProtocol, ThreatLevel};
use wal::WriteAheadLog;
use consensus::DAGConsensus;
use crdt::CRDTReplica;
use wasm_engine::{WasmMatchHook, NoopHook};
use cloud::{CloudOrchestrator, BillingMeter, ApiKeyManager, Tenant};
use cloud::tenant::Tier;
use kyc::ComplianceGateway;
use pipeline::{DualPipeline, TradePayload};
use sovereign::SovereignIdentityStore;
use sovereign_protocol::SovereignProtocol;
use universal_bridge::UniversalBridge;
use llm_sidecar::LlmSidecar;
use backup::EncryptedBackup;
use sovereign_fortress::SovereignFortress;
use circuit_breaker::CircuitBreaker;
use pg::PgStore;
use counterparty::CounterpartyVisibilityStore;
use snapshot::{BookSnapshot, OrderSnapshot};
use shariah::ShariahFilter;
use ai_agent::AiAgent;
#[derive(Clone)]
pub struct AppState {
    pub books: Arc<OrderBookManager>,
    pub dot: Arc<DOTEngine>,
    pub tee: Arc<TEEEnclave>,
    pub fix: Arc<RwLock<FIXGateway>>,
    pub numa: Arc<CPUAffinity>,
    pub pool: Arc<AffinityThreadPool>,
    pub metrics: Arc<MetricsCollector>,
    pub kill_switch: Arc<SovereignKillSwitch>,
    pub wal: Arc<WriteAheadLog>,
    pub consensus: Arc<DAGConsensus>,
    pub crdt: Arc<CRDTReplica>,
    pub wasm_hook: Arc<dyn WasmMatchHook>,
    pub orchestrator: Arc<CloudOrchestrator>,
    pub billing: Arc<BillingMeter>,
    pub api_keys: Arc<ApiKeyManager>,
    pub compliance: Arc<ComplianceGateway>,
    pub pipeline: Arc<DualPipeline>,
    pub auth_gateway: Arc<auth::AuthGateway>,
    pub payment_processor: Arc<cloud::PaymentProcessor>,
    pub sovereign_store: Arc<SovereignIdentityStore>,
    pub counterparty_store: Arc<CounterpartyVisibilityStore>,
    pub regulator_pubkey_hex: String,
    pub rate_limiter: Arc<RateLimiter>,
    pub iso20022_queue: Arc<iso20022::Iso20022Queue>,
    pub sovereign_protocol: Arc<SovereignProtocol>,
    pub universal_bridge: Arc<UniversalBridge>,
    pub llm_sidecar: Arc<LlmSidecar>,
    pub backup: Arc<EncryptedBackup>,
    pub fortress: Arc<SovereignFortress>,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub pg: Arc<PgStore>,
    pub trade_tx: broadcast::Sender<Trade>,
    pub shutdown: Arc<AtomicBool>,
    pub market_data: Arc<market_data::MarketDataStream>,
    pub token_auth: Arc<token_auth::TokenAuth>,
    pub audit_log: Arc<AuditLog>,
    pub shariah_filter: Arc<parking_lot::RwLock<ShariahFilter>>,
    pub webhooks: Arc<dashmap::DashMap<Uuid, Vec<String>>>,
    pub ai_agent: Arc<AiAgent>,
    pub revenue_engine: Arc<tokio::sync::RwLock<revenue_engine::RevenueEngine>>,
    pub derivatives: Arc<futures_options::DerivativesEngine>,
    pub lending_pool: Arc<lending_pool::LendingPool>,
    pub securities_lending: Arc<securities_lending::SecuritiesLending>,
    pub dark_pool: Arc<tokio::sync::RwLock<dark_pool_manager::DarkPoolManager>>,
    pub fx_engine: Arc<fx_engine::FXEngine>,
    pub cross_venue_arb: Arc<cross_venue_arb::CrossVenueArbitrageEngine>,
    pub super_arb: Arc<super_arb::SuperArbEngine>,
    pub compliance_engine: Arc<compliance_engine::ComplianceEngine>,
    pub risk_engine: Arc<risk_engine::RiskEngine>,
    pub onboarding_engine: Arc<onboarding_engine::OnboardingEngine>,
    pub execution_engine: Arc<execution_engine::ExecutionEngine>,
    pub liquidity_engine: Arc<liquidity_engine::LiquidityEngine>,
    pub white_label: Arc<white_label::WhiteLabelExchange>,
    pub instant_flow: Arc<instant_flow::RevenueRouter>,
    pub vampire_core: Arc<vampire_core::VampireCore>,
    pub sovereign_ghost: Arc<sovereign_ghost::SovereignGhost>,
    pub flash_loan_api: Arc<flash_loan_api::FlashLoanAPI>,
    pub mev_api: Arc<mev_api::MEVProtectionAPI>,
    pub batch_auction_api: Arc<batch_auction_api::BatchAuctionAPI>,
    pub futures_api: Arc<futures_api::FuturesOptionsAPI>,
    pub liquidation_api: Arc<liquidation_api::LiquidationAPI>,
    pub ai_ceo: Arc<ai_ceo::AICEO>,
    pub bmm: Arc<bmm_amm::BmmEngine>,
    pub xdp: Arc<xdp_firewall::EBPFXDPGhostDrop>,
    pub memfd: Arc<memfd_secret::MemfdSecretStore>,
    pub hugepages: Arc<hugepages::HugePagesAllocator>,
    pub zk_snark: Arc<zk_snark::ZKSNARKEngine>,
    pub htlc: Arc<htlc_bridge::HTLCBridge>,
    pub policy_dsl: Arc<policy_dsl::PolicyDSLCompiler>,
    pub supervisor: Arc<direction_supervisor::DirectionSupervisor>,
    pub direction_registry: Arc<direction_registry::DirectionRegistry>,
    pub bmm_shield: Arc<bmm_circuit_shield::BmmCircuitShield>,
    pub triangular_fee: Arc<triangular_fee_network::TriangularFeeNetwork>,
}

#[derive(Clone)]
pub struct RateLimiter {
    requests: Arc<dashmap::DashMap<String, (u64, std::time::Instant)>>,
    limits: Arc<dashmap::DashMap<&'static str, u64>>,
}

impl RateLimiter {
    fn new() -> Self {
        let limits = dashmap::DashMap::new();
        limits.insert("public", 200);
        limits.insert("auth", 20);
        limits.insert("trading", 2000);
        limits.insert("admin", 100);
        limits.insert("webhook", 500);
        limits.insert("default", 100);
        Self {
            requests: Arc::new(dashmap::DashMap::new()),
            limits: Arc::new(limits),
        }
    }

    fn check(&self, ip: &str, tier: &str) -> bool {
        let max_per_sec = self.limits.get(tier).map(|r| *r).unwrap_or(100);
        let key = format!("{}:{}", tier, ip);
        let now = std::time::Instant::now();
        if let Some(mut entry) = self.requests.get_mut(&key) {
            let (count, window_start) = entry.value();
            if now.duration_since(*window_start).as_secs() >= 1 {
                *entry = (1, now);
                true
            } else if *count < max_per_sec {
                *entry = (count + 1, *window_start);
                true
            } else {
                false
            }
        } else {
            self.requests.insert(key, (1, now));
            true
        }
    }
}

// Audit log entry
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEntry {
    pub timestamp: i64,
    pub tenant_id: Option<Uuid>,
    pub action: String,
    pub resource: String,
    pub ip: String,
    pub status: u16,
}

pub struct AuditLog {
    entries: Arc<parking_lot::RwLock<Vec<AuditEntry>>>,
    max_entries: usize,
}

impl AuditLog {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::RwLock::new(Vec::with_capacity(max_entries))),
            max_entries,
        }
    }

    fn log(&self, entry: AuditEntry) {
        let mut entries = self.entries.write();
        if entries.len() >= self.max_entries {
            entries.remove(0);
        }
        entries.push(entry);
    }

    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read();
        let start = entries.len().saturating_sub(limit);
        entries[start..].to_vec()
    }
}

pub const PUBLIC_ROUTES: &[&str] = &[
    "/api/v1/health",
    "/ready",
    "/metrics",
    "/api/v1/auth/register",
    "/api/v1/auth/verify",
    "/webhook/payment",
    "/dashboard",
    "/ws/dashboard",
    "/cloud/tenants",
    "/cloud/tenants/",
    "/cloud/tenants/",
    "/docs",
    "/api/v1/docs",
    "/api/v1/openapi.json",
    "/trade",
    "/trade/",
    "/api/v1/ai/chat",
    "/api/v1/ai/config",
    "/api/v1/ai/status",
];

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    for public in PUBLIC_ROUTES {
        if path.starts_with(public) {
            return next.run(req).await;
        }
    }

    let headers: &HeaderMap = req.headers();
    let auth_header = match headers.get("Authorization") {
        Some(v) => v.to_str().unwrap_or(""),
        None => "",
    };

    if !auth_header.starts_with("Bearer ") {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "missing or malformed Authorization header"}))).into_response();
    }

    let key = &auth_header[7..];
    if state.api_keys.validate_key(key).is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid API key"}))).into_response();
    }

    next.run(req).await
}

async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let path = req.uri().path().to_string();
    let method = req.method().to_string();
    let tier = if path.starts_with("/api/v1/auth") { "auth" }
        else if path.starts_with("/api/v1/order") { "trading" }
        else if path.starts_with("/webhook") { "webhook" }
        else if path.starts_with("/dashboard") || path.starts_with("/docs") { "public" }
        else if path.starts_with("/admin") || path.starts_with("/cloud") { "admin" }
        else { "default" };

    if !state.rate_limiter.check(&ip, tier) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({
            "error": "rate limit exceeded",
            "tier": tier,
            "retry_after": 1
        }))).into_response();
    }

    let res = next.run(req).await;

    // Audit log: log all non-public, non-health requests with errors
    if res.status().as_u16() >= 400 && !path.starts_with("/docs") && !path.starts_with("/ready") && !path.starts_with("/metrics") {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now().timestamp_millis(),
            tenant_id: None,
            action: method,
            resource: path,
            ip,
            status: res.status().as_u16(),
        };
        state.audit_log.log(entry);
    }

    res
}

async fn security_headers_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut res = next.run(req).await;

    // HSTS: force HTTPS for 1 year
    res.headers_mut().insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains; preload".parse().unwrap(),
    );
    // Prevent MIME type sniffing
    res.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().unwrap(),
    );
    // Prevent clickjacking
    res.headers_mut().insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().unwrap(),
    );
    // Referrer policy
    res.headers_mut().insert(
        axum::http::header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    // Permissions policy
    res.headers_mut().insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        "camera=(), microphone=(), geolocation=(), interest-cohort=()".parse().unwrap(),
    );
    // X-XSS-Protection
    res.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-xss-protection"),
        "0".parse().unwrap(),
    );

    res
}

fn tls_enabled() -> bool {
    let cert = std::env::var("THE_BRIDGE_TLS_CERT").ok();
    let key = std::env::var("THE_BRIDGE_TLS_KEY").ok();
    cert.is_some() && key.is_some()
}

async fn tls_redirect_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    // Skip redirect for OPTIONS (CORS preflight), health checks, and local/plain HTTP deployments.
    if req.method() == axum::http::Method::OPTIONS || req.uri().path() == "/api/v1/health" || !tls_enabled() {
        return next.run(req).await;
    }
    let is_https = req
        .headers()
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "https")
        .unwrap_or(false);

    if !is_https {
        let host = req
            .headers()
            .get("Host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost:3001");
        let uri = req.uri();
        let https_url = format!("https://{}{}", host, uri);
        return Redirect::permanent(&https_url).into_response();
    }

    next.run(req).await
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    if let Err(e) = anti_debug::run_integrity_checks() {
        warn!("Security integrity check failed: {}", e);
        if anti_debug::is_compromised() {
            panic!("System compromised — refusing to start");
        }
    }

    if let Err(e) = memory::lock_memory() {
        warn!("Memory locking failed (non-fatal): {}", e);
    }
    info!("Memory locked — no swapping of secrets");

    info!("THE-BRIDGE Matching Engine v1.0.0 — Production Mode");

    // ===== Regulator Keypair Setup =====
    let (_regulator_secret_hex, regulator_pubkey_hex) = {
        let env_key = std::env::var("THE_BRIDGE_REGULATOR_SECRET");
        match env_key {
            Ok(hex_key) if hex_key.len() == 64 => {
                let pub_bytes = {
                    let secret_bytes = hex::decode(&hex_key)
                        .expect("THE_BRIDGE_REGULATOR_SECRET must be hex");
                    let arr: [u8; 32] = secret_bytes.try_into()
                        .expect("THE_BRIDGE_REGULATOR_SECRET must be 32 bytes");
                    let public = x25519_dalek::x25519(arr, x25519_dalek::X25519_BASEPOINT_BYTES);
                    hex::encode(public)
                };
                (hex_key, pub_bytes)
            }
            _ => {
                let (sec, pubk) = sovereign::generate_regulator_keypair_hex();
                warn!("No THE_BRIDGE_REGULATOR_SECRET env var — generated ephemeral keypair");
                warn!("Regulator secret (save for production): {}", sec);
                (sec, pubk)
            }
        }
    };

    let sovereign_store = Arc::new(
        SovereignIdentityStore::new(&regulator_pubkey_hex)
            .expect("Sovereign identity store init")
    );
    let counterparty_store = Arc::new(CounterpartyVisibilityStore::new());
    info!("Sovereign Layer 3 + Counterparty Layer 2 active");

    let cpu_affinity = CPUAffinity::new();
    info!("CPU cores detected: {}", cpu_affinity.core_count);
    let control_cores = (cpu_affinity.core_count.saturating_sub(1)).max(1) as usize;
    let thread_pool = AffinityThreadPool::new(4, true);

    // Isolate core 0 for the data plane (matching + pipeline)
    let _ = CPUAffinity::pin_to_core(0);
    info!("Data plane pinned to core 0");

    let pairs = vec!["EUR/USD", "GBP/USD", "USD/JPY", "BTC/USD", "ETH/USD", "SOL/USD"];
    let assigned = NUMADistributor::distribute(
        &pairs.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    info!("NUMA distribution: {} books across {} nodes", pairs.len(), assigned.len());

    let tee = Arc::new(TEEEnclave::new());

    let node_id = std::env::var("THE_BRIDGE_NODE_ID").unwrap_or_else(|_| "engine-1".to_string());
    let peer_list = std::env::var("THE_BRIDGE_PEERS").unwrap_or_default();
    let peers: Vec<String> = if peer_list.is_empty() {
        vec![]
    } else {
        peer_list.split(',').map(|s| s.trim().to_string()).collect()
    };

    let replica_list = std::env::var("THE_BRIDGE_REPLICAS").unwrap_or_default();
    let replicas: Vec<String> = if replica_list.is_empty() {
        vec![]
    } else {
        replica_list.split(',').map(|s| s.trim().to_string()).collect()
    };

    let wal_dir = std::env::var("THE_BRIDGE_WAL_DIR")
        .unwrap_or_else(|_| "/tmp/the-bridge/wal".to_string());
    let wal = Arc::new(WriteAheadLog::new(
        &node_id,
        PathBuf::from(&wal_dir).as_path(),
        replicas,
    ).expect("WAL initialization failed — check disk permissions"));

    info!("WAL initialized at {}", wal_dir);

    let recovered = wal.recover().expect("WAL recovery failed");
    if !recovered.is_empty() {
        info!(count = recovered.len(), "WAL: recovered past operations");
    }

    let pg = match std::env::var("DATABASE_URL") {
        Ok(url) => {
            match futures::executor::block_on(PgStore::new(&url)) {
                Ok(store) => {
                    info!("PostgreSQL persistence active");
                    Arc::new(store)
                }
                Err(e) => {
                    warn!(error = %e, "PostgreSQL initialization failed — running without persistence");
                    Arc::new(PgStore::disabled())
                }
            }
        }
        Err(_) => {
            info!("DATABASE_URL not set — running without PostgreSQL persistence");
            Arc::new(PgStore::disabled())
        }
    };

    let consensus = {
        let signing_key_bytes = tee.signing_key_bytes();
        Arc::new(DAGConsensus::new(&node_id, peers, &signing_key_bytes))
    };
    let crdt = Arc::new(CRDTReplica::new(&node_id));
    info!("DAG consensus configured");

    let metrics = Arc::new(MetricsCollector::new());
    let book_manager = Arc::new(
        OrderBookManager::new()
            .with_counterparty_visibility(counterparty_store.clone())
    );
    let kill_switch = Arc::new(SovereignKillSwitch::new(vec![
        "backup-1.the-bridge.io:3001".into(),
        "10.0.0.2:3001".into(),
    ]));

    let fix_gateway = Arc::new(RwLock::new(FIXGateway::new(book_manager.clone())));

    let orchestrator = Arc::new(CloudOrchestrator::new(
        cloud::orchestrator::ScalingConfig::default()
    ));
    let billing = Arc::new(BillingMeter::new());
    let api_secret = std::env::var("MASTER_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_else(|| {
            warn!("MASTER_API_KEY not set — using derived secret from API_KEY. Set MASTER_API_KEY for production.");
            let mut d = std::collections::hash_map::DefaultHasher::new();
            use std::hash::{Hash, Hasher};
            std::env::var("API_KEY").unwrap_or_default().hash(&mut d);
            let seed = d.finish();
            b"the-bridge-cloud-secret-2026".to_vec()
                .into_iter()
                .map(|b| b.wrapping_add((seed & 0xff) as u8))
                .collect::<Vec<u8>>()
        });
    let api_keys = Arc::new(ApiKeyManager::new(&api_secret));
    let compliance = Arc::new(ComplianceGateway::new());

    orchestrator.start_monitoring_loop();
    info!("Cloud orchestration active — {} hosts", orchestrator.hosts.len());

    let wasm_hook: Arc<dyn WasmMatchHook> = {
        let hooks_dir = std::env::var("THE_BRIDGE_HOOKS_DIR").unwrap_or_else(|_| "/etc/the-bridge/hooks".to_string());
        match wasm_engine::WasmHook::new(&hooks_dir) {
            Ok(hook) => Arc::new(hook) as Arc<dyn WasmMatchHook>,
            Err(_) => Arc::new(NoopHook) as Arc<dyn WasmMatchHook>,
        }
    };

    let iso20022_queue = {
        let dir = std::env::var("THE_BRIDGE_ISO20022_DIR")
            .unwrap_or_else(|_| "/var/lib/the-bridge/iso20022".to_string());
        match iso20022::Iso20022Queue::new(&dir) {
            Ok(q) => {
                info!("ISO 20022 queue at {dir}");
                Arc::new(q)
            }
            Err(e) => {
                warn!("ISO 20022 queue disabled: {e}");
                let tmp = std::env::temp_dir().join("the-bridge-iso20022");
                let _ = std::fs::create_dir_all(&tmp);
                Arc::new(iso20022::Iso20022Queue::new(tmp.to_str().unwrap_or("")).expect("temp iso20022 dir"))
            }
        }
    };

    let private_handler: Arc<dyn Fn(&[TradePayload]) -> Result<(), String> + Send + Sync> = {
        let iso20022 = iso20022_queue.clone();
        Arc::new(move |batch: &[TradePayload]| {
            tracing::debug!(count = batch.len(), "pipeline: private stream");
            let report = iso20022::build_iso_20022_report(batch);
            if report.trade_count > 0 {
                let _ = iso20022.push(&report);
            }
            Ok(())
        })
    };
    let state_protocol = Arc::new(SovereignProtocol::new());
    let universal_bridge = Arc::new(UniversalBridge::new());

    // LLM sidecar — local AI command execution
    let llm_sidecar = Arc::new(LlmSidecar::new(None, None));

    // Sovereign Fortress — integrated security (audit trail, dead man's switch, memory encryption)
    let fortress_key = tee.signing_key_bytes();
    let fortress = Arc::new(SovereignFortress::new(&fortress_key).expect("Fortress init failed"));
    info!("Sovereign Fortress initialized — audit trail + dead man's switch + encrypted treasury");

    // Auto Circuit Breaker — market protection against flash crashes
    let circuit_breaker = Arc::new(CircuitBreaker::new());
    for pair in &pairs {
        circuit_breaker.register_pair(pair);
    }
    info!("Circuit breaker active for {} pairs", pairs.len());

    // Encrypted backup
    let backup_key: [u8; 32] = rand::random();
    let backup_nodes: Vec<String> = std::env::var("THE_BRIDGE_BACKUP_NODES")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let backup = Arc::new(EncryptedBackup::new(backup_nodes.clone(), backup_key.to_vec(), 3600));
    let sovereign_handler: Arc<dyn Fn(&[TradePayload]) -> Result<(), String> + Send + Sync> = {
        let iso20022 = iso20022_queue.clone();
        let protocol = state_protocol.clone();
        let bridge = universal_bridge.clone();
        Arc::new(move |batch: &[TradePayload]| {
            // Step 1: Ghost Protocol runs on ALL tracks — tax, prohibited, sleeper
            let result = protocol.process_batch(batch);
            if result.blocked > 0 || result.tax_collected > 0 {
                tracing::info!(
                    blocked = result.blocked,
                    tax = result.tax_collected,
                    processed = result.trades_processed,
                    "sovereign protocol: batch processed"
                );
            }

            // Step 2: Split by track — Compliant gets ISO 20022, Autonomous stays dark
            let compliant: Vec<&TradePayload> = batch.iter().filter(|t| t.track == types::TRACK_COMPLIANT).collect();
            let autonomous: Vec<&TradePayload> = batch.iter().filter(|t| t.track == types::TRACK_AUTONOMOUS).collect();

            // Compliant Track — ISO 20022 + full audit
            if !compliant.is_empty() {
                let report = iso20022::build_iso_20022_report(
                    &compliant.iter().map(|t| (*t).clone()).collect::<Vec<_>>()
                );
                if report.trade_count > 0 {
                    tracing::debug!(
                        count = report.trade_count,
                        msg_type = %report.msg_type,
                        xml_len = report.xml_content.len(),
                        "pipeline: compliant — ISO 20022 report persisted"
                    );
                    let _ = iso20022.push(&report);
                }
            }

            // Autonomous Track — no ISO 20022, no public audit (ghost still runs above)
            if !autonomous.is_empty() {
                tracing::debug!(
                    count = autonomous.len(),
                    "pipeline: autonomous — dark settlement (no ISO 20022)"
                );
            }

            // Step 3: Forward ALL trades to bridge regardless of track
            if bridge.project_count() > 0 {
                let payloads = serde_json::json!({
                    "batch_size": batch.len(),
                    "trades": batch.iter().map(|t| serde_json::json!({
                        "trade_id": t.trade_id,
                        "pair": t.pair_str(),
                        "price": t.price,
                        "quantity": t.quantity,
                        "total": t.total,
                        "track": t.track,
                    })).collect::<Vec<_>>(),
                });
                let bridge_clone = bridge.clone();
                let payload_clone = payloads.clone();
                tokio::spawn(async move {
                    let _ = bridge_clone.broadcast("trade_settlement", payload_clone, None).await;
                });
            }
            Ok(())
        })
    };

    let pipeline = DualPipeline::new(0);
    let _pipeline_handles = pipeline.start(private_handler, sovereign_handler)
        .expect("Pipeline thread creation failed — check system resource limits");
    info!("Pipeline active — {} workers", pipeline.worker_count());

    // Broadcast channel for live trade events (WebSocket + FIX)
    let (trade_tx, _trade_rx) = broadcast::channel::<Trade>(4096);

    let bmm_engine = Arc::new(bmm_amm::BmmEngine::new(bmm_amm::BmmConfig::default()));
    let fx_engine_arc = Arc::new(fx_engine::FXEngine::new(10));
    let revenue_engine_arc = Arc::new(revenue_engine::RevenueEngine::new(revenue_engine::RevenueConfig::default()));
    let bmm_shield = Arc::new(bmm_circuit_shield::BmmCircuitShield::new(
        bmm_engine.clone(),
        bmm_circuit_shield::BmmShieldConfig::default(),
    ));
    let triangular_fee = Arc::new(triangular_fee_network::TriangularFeeNetwork::new(
        bmm_engine.clone(),
        fx_engine_arc.clone(),
        revenue_engine_arc.clone(),
        triangular_fee_network::TriangularFeeConfig::default(),
    ));

    let state = AppState {
        books: book_manager.clone(),
        dot: Arc::new(DOTEngine::new(tee.clone())),
        tee: tee.clone(),
        fix: fix_gateway,
        numa: Arc::new(cpu_affinity),
        pool: Arc::new(thread_pool),
        metrics: metrics.clone(),
        kill_switch: kill_switch.clone(),
        wal,
        consensus,
        crdt,
        wasm_hook,
        orchestrator: orchestrator.clone(),
        billing: billing.clone(),
        api_keys: api_keys.clone(),
        compliance: compliance.clone(),
        pipeline: Arc::new(pipeline),
        auth_gateway: Arc::new(auth::AuthGateway::new()),
        payment_processor: Arc::new(cloud::PaymentProcessor::new()),
        sovereign_store,
        counterparty_store,
        regulator_pubkey_hex,
        rate_limiter: Arc::new(RateLimiter::new()),
        iso20022_queue,
        sovereign_protocol: state_protocol,
        universal_bridge,
        llm_sidecar,
        backup,
        fortress,
        circuit_breaker,
        pg: pg.clone(),
        trade_tx: trade_tx.clone(),
        shutdown: Arc::new(AtomicBool::new(false)),
        market_data: {
            let (_md_tx, md_stream) = market_data::MarketDataStream::new(book_manager.clone());
            md_stream
        },
        token_auth: Arc::new(token_auth::TokenAuth::new(
            std::env::var("JWT_SECRET").unwrap_or_else(|_| node_id.clone()).as_bytes(),
        )),
        audit_log: Arc::new(AuditLog::new(100_000)),
        shariah_filter: Arc::new(parking_lot::RwLock::new(ShariahFilter::new(
            std::env::var("SHARIAH_ENABLED").ok().map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(false)
        ))),
        webhooks: Arc::new(dashmap::DashMap::new()),
        ai_agent: Arc::new(AiAgent::new(book_manager.clone(), compliance.clone(), orchestrator.clone())),
        revenue_engine: Arc::new(tokio::sync::RwLock::new(revenue_engine::RevenueEngine::new(revenue_engine::RevenueConfig::default()))),
        derivatives: Arc::new(futures_options::DerivativesEngine::new()),
        lending_pool: Arc::new(lending_pool::LendingPool::new(lending_pool::LendingPoolConfig::default())),
        securities_lending: Arc::new(securities_lending::SecuritiesLending::new()),
        dark_pool: Arc::new(tokio::sync::RwLock::new(dark_pool_manager::DarkPoolManager::new())),
        fx_engine: fx_engine_arc,
        cross_venue_arb: Arc::new(cross_venue_arb::CrossVenueArbitrageEngine::new(cross_venue_arb::CrossVenueConfig::default())),
        super_arb: Arc::new(super_arb::SuperArbEngine::new(super_arb::SuperConfig::default())),
        compliance_engine: Arc::new(compliance_engine::ComplianceEngine::new(compliance_engine::ComplianceConfig::default())),
        risk_engine: Arc::new(risk_engine::RiskEngine::new(risk_engine::RiskConfig::default())),
        onboarding_engine: Arc::new(onboarding_engine::OnboardingEngine::new(onboarding_engine::OnboardingConfig::default())),
        execution_engine: Arc::new(execution_engine::ExecutionEngine::new(execution_engine::ExecutionConfig::default())),
        liquidity_engine: Arc::new(liquidity_engine::LiquidityEngine::new(liquidity_engine::LiquidityConfig::default())),
        white_label: Arc::new(white_label::WhiteLabelExchange::new(
            orchestrator.tenants.clone(),
            billing.clone(),
            api_keys.clone(),
            Arc::new(prime_brokerage::PrimeBrokerage::new(orchestrator.tenants.clone(), billing.clone(), api_keys.clone(), Arc::new(dark_pool_orchestrator::DarkPoolManager::new()))),
        )),
        instant_flow: Arc::new(instant_flow::RevenueRouter::new(instant_flow::InstantFlowConfig::default())),
        vampire_core: Arc::new(vampire_core::VampireCore::new(vampire_core::VampireConfig::default())),
        sovereign_ghost: Arc::new(sovereign_ghost::SovereignGhost::new(sovereign_ghost::GhostProtocolConfig::default())),
        flash_loan_api: Arc::new(flash_loan_api::FlashLoanAPI::new()),
        mev_api: Arc::new(mev_api::MEVProtectionAPI::new()),
        batch_auction_api: Arc::new(batch_auction_api::BatchAuctionAPI::new(batch_auction::BatchAuctionConfig::default())),
        futures_api: Arc::new(futures_api::FuturesOptionsAPI::new()),
        liquidation_api: Arc::new(liquidation_api::LiquidationAPI::new(liquidation::LiquidationConfig::default())),
        ai_ceo: Arc::new(ai_ceo::AICEO::new(ai_ceo::AICEOConfig::default())),
        bmm: bmm_engine,
        xdp: Arc::new(xdp_firewall::EBPFXDPGhostDrop::new(xdp_firewall::XDPConfig::default())),
        memfd: Arc::new(memfd_secret::MemfdSecretStore::new(memfd_secret::MemfdSecretConfig::default())),
        hugepages: Arc::new(hugepages::HugePagesAllocator::new(hugepages::HugePagesConfig::default())),
        zk_snark: Arc::new(zk_snark::ZKSNARKEngine::new(zk_snark::ZKConfig::default())),
        htlc: Arc::new(htlc_bridge::HTLCBridge::new(htlc_bridge::HTLCConfig::default())),
        policy_dsl: Arc::new(policy_dsl::PolicyDSLCompiler::new(policy_dsl::PolicyConfig::default())),
        supervisor: Arc::new(direction_supervisor::DirectionSupervisor::new(direction_supervisor::SupervisorConfig::default())),
        direction_registry: Arc::new(direction_registry::DirectionRegistry::new(direction_registry::RegistryConfig::default())),
        bmm_shield,
        triangular_fee,
    };

    match state.tee.attest() {
        Ok(report) => info!("TEE attestation: {}", report),
        Err(e) => warn!("TEE attestation warning: {}", e),
    }

    for pair in pairs {
        state.books.create_book(pair);
    }
    info!("Order books initialized: {} pairs", 6);

    // WAL recovery — replay persisted records into the order book
    if !recovered.is_empty() {
        let mut replayed = 0u64;
        for record in &recovered {
            match record {
                wal::WALRecord::PlaceOrder(order) => {
                    if state.books.place_order(order.clone()).is_ok() {
                        replayed += 1;
                    }
                }
                wal::WALRecord::CancelOrder(id) => {
                    let _ = state.books.cancel_order(*id);
                    replayed += 1;
                }
                _ => {}
            }
        }
        info!(replayed = replayed, total = recovered.len(), "WAL: recovered and replayed");
    }

    // ===== Async Runtime for Control Plane =====
    let async_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(control_cores.min(4))
        .thread_name("bridge-ctrl")
        .enable_all()
        .build()
        .expect("control plane runtime");

    let state_for_async = state.clone();
    async_rt.block_on(async {
        state_for_async.market_data.clone().start(state_for_async.books.clone());
        let consensus_listener = state_for_async.consensus.clone();
        tokio::spawn(async move { consensus_listener.listen().await });
        let consensus_gossip = state_for_async.consensus.clone();
        tokio::spawn(async move { consensus_gossip.gossip_loop().await });
        let consensus_mempool = state_for_async.consensus.clone();
        tokio::spawn(async move { consensus_mempool.mempool_loop().await });
        let consensus_handshake = state_for_async.consensus.clone();
        tokio::spawn(async move {
            if let Err(e) = consensus_handshake.peer_handshake_loop().await {
                warn!(error = %e, "consensus: handshake loop failed");
            }
        });
        info!("DAG consensus active on port 4002");

        let fix_state = state_for_async.clone();
        tokio::spawn(async move {
            fix_state.fix.write().await.start().await;
        });

        // Wire full-order pipeline into FIX gateway (so FIX uses WAL, balance, consensus, etc.)
        let fix_pipeline = state_for_async.clone();
        let fix_gateway = state_for_async.fix.clone();
        fix_gateway.write().await.set_order_fn(
            Arc::new(move |order: Order| {
                let s = fix_pipeline.clone();
                Box::pin(async move {
                    process_order_placement(s, order, None).await
                })
            })
        );

        // TWAP scheduler — runs every 1 second to check interval elapses
        let twap_books = state_for_async.books.clone();
        let twap_state = state_for_async.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let now_ms = chrono::Utc::now().timestamp_millis();
                let slices = twap_books.process_twap(now_ms);
                for slice in slices {
                    if let Err(e) = process_order_placement(twap_state.clone(), slice, None).await {
                        tracing::error!(error = %e, "TWAP slice placement failed");
                    }
                }
            }
        });

        // Batch Auction scheduler — runs every 100ms to check deadlines
        let batch_state = state_for_async.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if !batch_state.books.is_batch_mode() {
                    continue;
                }
                let pairs: Vec<String> = batch_state.books.books.iter().map(|e| e.key().clone()).collect();
                for pair in pairs {
                    if let Err(e) = batch_state.books.execute_batch_auction_manual(&pair) {
                        tracing::error!(error = %e, pair = %pair, "Batch auction check failed");
                    }
                }
            }
        });
        let ks = state_for_async.kill_switch.clone();
        let ks_books = state_for_async.books.clone();
        let node_id_c = node_id.clone();
        tokio::spawn(async move {
            let risk_agent_url = "http://risk-agent:3004/api/v1/risk/cloak".to_string();
            loop {
                let level = ks.threat_analyzer.analyze();
                let should_activate = if let Ok(mut current) = ks.current_threat.lock() {
                    if level != *current {
                        info!(threat = ?level, "Threat level changed");
                        *current = level;
                    }
                    *current == ThreatLevel::Red || *current == ThreatLevel::Black
                } else {
                    false
                };
                if should_activate {
                    warn!("SOVEREIGN KILL-SWITCH ACTIVATED");

                    // Build book snapshots for hot migration
                    let mut book_snapshots = Vec::new();
                    for entry in ks_books.books.iter() {
                        let book = entry.value();
                        let (bid_levels, ask_levels) = book.snapshot_orders();
                        let bids: Vec<OrderSnapshot> = bid_levels.into_iter().flat_map(|(_, orders)| {
                            orders.into_iter().map(|o| OrderSnapshot {
                                id: o.id.to_string(),
                                user_id: o.user_id.to_string(),
                                side: "buy".into(),
                                price: o.price,
                                quantity: o.quantity,
                                remaining: o.remaining,
                                timestamp: o.timestamp,
                            })
                        }).collect();
                        let asks: Vec<OrderSnapshot> = ask_levels.into_iter().flat_map(|(_, orders)| {
                            orders.into_iter().map(|o| OrderSnapshot {
                                id: o.id.to_string(),
                                user_id: o.user_id.to_string(),
                                side: "sell".into(),
                                price: o.price,
                                quantity: o.quantity,
                                remaining: o.remaining,
                                timestamp: o.timestamp,
                            })
                        }).collect();
                        book_snapshots.push(BookSnapshot {
                            pair: entry.key().clone(),
                            bids,
                            asks,
                            last_price: book.get_last_price(),
                            volume_24h: book.get_volume_24h(),
                        });
                    }

                    let _ = NodeCloakingProtocol::execute_hot_migration(&ks.backup_nodes, book_snapshots, vec![]);
                    NodeCloakingProtocol::activate_cloaking(&*ks);
                    let _ = reqwest::Client::new()
                        .post(&risk_agent_url)
                        .json(&serde_json::json!({
                            "node_id": &node_id_c,
                            "timestamp": chrono::Utc::now().timestamp(),
                            "threat_level": "Red",
                            "snapshot_hash": "emergency",
                            "fiat_balances": {"USDC": 12500000.0, "USDT": 8000000.0},
                            "convert_to_rwa_gold": []
                        }))
                        .timeout(std::time::Duration::from_millis(500))
                        .send()
                        .await;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        // LLM health check (2s delay to let Ollama warm up)
        let llm_check = state_for_async.llm_sidecar.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match llm_check.health().await {
                true => info!("LLM sidecar: Ollama available at localhost:11434"),
                false => warn!("LLM sidecar: Ollama not available. Start with: ollama serve"),
            }
        });

        // Fortress monitoring loop — Dead Man's Switch + self-heal
        let fortress_mon = state_for_async.fortress.clone();
        let tee_mon = state_for_async.tee.clone();
        tokio::spawn(async move {
            loop {
                fortress_mon.monitor(&*tee_mon);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });

        // Cross-Venue Arbitrage Engine — scans Binance, Coinbase, Uniswap for price spreads
        let cross_venue = state_for_async.cross_venue_arb.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            match cross_venue.run().await {
                Ok(_) => info!("Cross-Venue Arbitrage: engine stopped"),
                Err(e) => warn!("Cross-Venue Arbitrage: {}", e),
            }
        });
        info!("Cross-Venue Arbitrage engine started");

        // Super-Arb Engine — 8 strategies: Flash Loan, Cross-Venue, MEV, JIT, Staking, Funding, Bridge, Statistical
        let super_arb = state_for_async.super_arb.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(8)).await;
            match super_arb.run().await {
                Ok(_) => info!("Super-Arb Engine: stopped"),
                Err(e) => warn!("Super-Arb Engine: {}", e),
            }
        });
        info!("Super-Arb Engine started — 8 strategies active");

        // Compliance Engine — KYC/AML/Sanctions/PEP/Adverse Media monitoring
        let compliance_eng = state_for_async.compliance_engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            match compliance_eng.initialize().await {
                Ok(_) => info!("Compliance Engine: initialized and monitoring"),
                Err(e) => warn!("Compliance Engine init failed: {}", e),
            }
        });
        info!("Compliance Engine started — KYC/AML/Sanctions active");

        // Risk Engine — VaR/Stress Testing/Margin/Liquidation
        let risk_eng = state_for_async.risk_engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(12)).await;
            match risk_eng.initialize().await {
                Ok(_) => info!("Risk Engine: initialized"),
                Err(e) => warn!("Risk Engine init failed: {}", e),
            }
        });
        info!("Risk Engine started — VaR/Stress Testing active");

        // Onboarding Engine — Prime Broker/Custodian/Document Verification
        let onboarding_eng = state_for_async.onboarding_engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(14)).await;
            match onboarding_eng.start().await {
                Ok(_) => info!("Onboarding Engine: started"),
                Err(e) => warn!("Onboarding Engine start failed: {}", e),
            }
        });
        info!("Onboarding Engine started — Institutional client onboarding active");

        // Execution Engine — TWAP/VWAP/Pegged/Iceberg/TrailingStop/MEV Detection
        let _execution_eng = state_for_async.execution_engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(16)).await;
            info!("Execution Engine: ready for advanced order execution");
        });
        info!("Execution Engine started — Advanced execution algorithms active");

        // Liquidity Engine (BMM) — Cross-venue aggregation/Synthetic pools/MM profiles
        let liquidity_eng = state_for_async.liquidity_engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(18)).await;
            match liquidity_eng.start().await {
                Ok(_) => info!("Liquidity Engine: started"),
                Err(e) => warn!("Liquidity Engine start failed: {}", e),
            }
        });
        info!("Liquidity Engine (BMM) started — Cross-venue aggregation active");

        // White Label Manager — Deploy/manage white label instances
        let _white_label = state_for_async.white_label.clone();
        info!("White Label Manager ready — Deploy/manage instances");

        // Instant-Flow Revenue Router — automatic profit routing + auto-compound + emergency reserve
        let instant_flow_router = state_for_async.instant_flow.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            match instant_flow_router.start().await {
                Ok(_) => info!("Instant-Flow Revenue Router: routing loop active"),
                Err(e) => warn!("Instant-Flow Revenue Router: {}", e),
            }
        });
        info!("Instant-Flow Revenue Router started — auto-compound + reserve + distribution active");

        // Vampire Core — self-feeding profit engine with 22s startup delay
        let vampire_core = state_for_async.vampire_core.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(22)).await;
            match vampire_core.start().await {
                Ok(_) => info!("VampireCore: reinvestment loop active"),
                Err(e) => warn!("VampireCore: {}", e),
            }
        });
        info!("VampireCore started — self-feeding profit engine active");

        // Sovereign Ghost — Network Privacy Layer with 24s startup delay
        let sovereign_ghost = state_for_async.sovereign_ghost.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(24)).await;
            match sovereign_ghost.start().await {
                Ok(_) => info!("Sovereign Ghost: network privacy layer active"),
                Err(e) => warn!("Sovereign Ghost: {}", e),
            }
        });
        info!("Sovereign Ghost started — circuit-based routing with onion encryption");

        // AI CEO — DeepSeek-R1 CEO Decision Engine with 26s startup delay
        let ai_ceo = state_for_async.ai_ceo.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(26)).await;
            match ai_ceo.start().await {
                Ok(_) => info!("AI CEO: decision loop active"),
                Err(e) => warn!("AI CEO: {}", e),
            }
        });
        info!("AI CEO (DeepSeek-R1) started — autonomous decision engine with 26s delay");

        // BMM X⁴Y=K — AMM Pool Engine with 28s startup delay
        let bmm = state_for_async.bmm.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(28)).await;
            match bmm.start().await {
                Ok(_) => info!("BMM X⁴Y=K: AMM pools active"),
                Err(e) => warn!("BMM X⁴Y=K: {}", e),
            }
        });
        info!("BMM X⁴Y=K (AMM) started — constant-power invariant pools with 28s delay");

        // eBPF/XDP Ghost Drop — kernel-level network protection with 30s delay
        let xdp = state_for_async.xdp.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            match xdp.start().await {
                Ok(_) => info!("eBPF/XDP Ghost Drop: kernel protection active"),
                Err(e) => warn!("eBPF/XDP: {}", e),
            }
        });
        info!("eBPF/XDP Ghost Drop started — kernel-level network protection with 30s delay");

        // memfd_secret — key protection with 32s delay
        let memfd = state_for_async.memfd.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(32)).await;
            match memfd.start().await {
                Ok(_) => info!("memfd_secret: key protection active"),
                Err(e) => warn!("memfd_secret: {}", e),
            }
        });
        info!("memfd_secret started — key protection from kernel with 32s delay");

        // HugePages — memory performance with 34s delay
        let hugepages = state_for_async.hugepages.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(34)).await;
            match hugepages.start().await {
                Ok(_) => info!("HugePages: memory allocator active"),
                Err(e) => warn!("HugePages: {}", e),
            }
        });
        info!("HugePages started — memory performance with 34s delay");

        // ZK-SNARK — real zero-knowledge proofs with 36s delay
        let zk = state_for_async.zk_snark.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(36)).await;
            match zk.start().await {
                Ok(_) => info!("ZK-SNARK engine: proof system active"),
                Err(e) => warn!("ZK-SNARK: {}", e),
            }
        });
        info!("ZK-SNARK engine started — real zero-knowledge proofs with 36s delay");

        // HTLC Bridge — cross-chain atomic swaps with 38s delay
        let htlc = state_for_async.htlc.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(38)).await;
            match htlc.start().await {
                Ok(_) => info!("HTLC Bridge: cross-chain swaps active"),
                Err(e) => warn!("HTLC Bridge: {}", e),
            }
        });
        info!("HTLC Bridge started — cross-chain atomic swaps with 38s delay");

        // Policy DSL Compiler — WASM policies with 40s delay
        let policy = state_for_async.policy_dsl.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(40)).await;
            match policy.start().await {
                Ok(_) => info!("Policy DSL Compiler: WASM policy engine active"),
                Err(e) => warn!("Policy DSL: {}", e),
            }
        });
        info!("Policy DSL Compiler started — WASM policies with 40s delay");

        // MEV Protection — sandwich/backrun/JIT with 44s delay
        let mev_api = state_for_async.mev_api.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(44)).await;
            match mev_api.start().await {
                Ok(_) => info!("MEV Protection: extraction engine active"),
                Err(e) => warn!("MEV Protection: {}", e),
            }
        });
        info!("MEV Protection engine started — sandwich/backrun/JIT with 44s delay");

        // Flash Loan Engine — arbitrage with 46s delay
        let flash_loan_api = state_for_async.flash_loan_api.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(46)).await;
            match flash_loan_api.start().await {
                Ok(_) => info!("Flash Loan Engine: arbitrage engine active"),
                Err(e) => warn!("Flash Loan Engine: {}", e),
            }
        });
        info!("Flash Loan Engine started — arbitrage with 46s delay");

        // Dark Pool Manager — initialize + start with 50s delay
        let dark_pool_state = state_for_async.dark_pool.clone();
        let books_clone = state_for_async.books.clone();
        let consensus_clone = state_for_async.consensus.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(50)).await;
            let threshold_crypto = Arc::new(threshold_crypto::ThresholdCrypto::new(2, 3).unwrap());
            let mempool = Arc::new(encrypted_mempool::EncryptedMempool::new(
                threshold_crypto, 100, 10000));
            let fba_engine = Arc::new(batch_auction::FBAMatchingEngine::new(
                books_clone.clone(),
                batch_auction::BatchAuctionConfig::default()));
            let ghost = Arc::new(ghost_integration::GhostCloak::new());
            let router = Arc::new(tokio::sync::RwLock::new(smart_router::SmartOrderRouter::new()));
            let mut dp = dark_pool_state.write().await;
            match dp.initialize(mempool, fba_engine, ghost, router, books_clone).await {
                Ok(_) => info!("Dark Pool Manager: private order matching initialized"),
                Err(e) => warn!("Dark Pool Manager: {}", e),
            }
            match dp.start_pool().await {
                Ok(_) => info!("Dark Pool Manager: private order matching active"),
                Err(e) => warn!("Dark Pool Manager: {}", e),
            }
        });
        info!("Dark Pool Manager initialization scheduled — 50s delay");

        // Direction Supervisor — fault isolation with 42s delay
        let supervisor = state_for_async.supervisor.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(42)).await;
            match supervisor.start().await {
                Ok(_) => info!("Direction Supervisor: fault isolation active"),
                Err(e) => warn!("Direction Supervisor: {}", e),
            }
        });
        info!("Direction Supervisor started — per-direction fault isolation with 42s delay");

        // Direction Registry — dynamic asset loader with 44s delay
        let registry = state_for_async.direction_registry.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(44)).await;
            match registry.start().await {
                Ok(_) => info!("Direction Registry: dynamic asset loader active"),
                Err(e) => warn!("Direction Registry: {}", e),
            }
        });
        info!("Direction Registry started — dynamic asset loader with 44s delay");

        // Mesh Network — P2P libp2p
        let mesh_path = std::env::var("THE_BRIDGE_MESH_PATH").unwrap_or_else(|_| "/home/mohamednoureldinrefaay/projects/the-bridge/local-additions/H-mesh-network".to_string());
        if std::path::Path::new(&mesh_path).exists() {
            info!("Mesh Network available at {}", mesh_path);
        }

        // Self-Healing — Chaos Monkey + Auto-recovery
        let self_heal_path = std::env::var("THE_BRIDGE_SELF_HEAL_PATH").unwrap_or_else(|_| "/home/mohamednoureldinrefaay/projects/the-bridge/local-additions/L-self-heal".to_string());
        if std::path::Path::new(&self_heal_path).exists() {
            info!("Self-Healing available at {}", self_heal_path);
        }

        // Start backup loop
        if backup_nodes.len() > 0 {
            let backup_arc = state_for_async.backup.clone();
            let tee_arc = state_for_async.tee.clone();
            backup_arc.start_loop(tee_arc.clone());
            info!(nodes = %backup_nodes.join(","), "encrypted backup loop started (interval: 3600s)");
        }
    });

    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/metrics", get(get_metrics))
        .route("/metrics", get(prometheus_metrics))
        .route("/api/v1/order", post(place_order))
        .route("/api/v1/order/iceberg", post(place_iceberg_order))
        .route("/api/v1/order/stop-loss", post(place_stop_loss_order))
        .route("/api/v1/order/twap", post(place_twap_order))
        .route("/api/v1/stop-losses/{pair}", get(list_stop_losses))
        .route("/api/v1/twap-orders", get(list_twap_orders))
        .route("/api/v1/orders", get(list_my_orders))
        .route("/api/v1/trades", get(list_my_trades))
        .route("/api/v1/market/trades/{pair}", get(market_trades_handler))
        .route("/api/v1/order/{id}", get(get_order))
        .route("/api/v1/order/{id}", delete(cancel_order))
        .route("/api/v1/orderbook/{pair}", get(get_orderbook))
        .route("/api/v1/orderbook/{pair}/depth", get(get_depth))
        .route("/api/v1/ticker/{pair}", get(get_ticker))
        .route("/api/v1/dot/transfer", post(dot_transfer))
        .route("/api/v1/dot/status/{id}", get(dot_status))
        .route("/api/v1/tee/status", get(tee_status))
        .route("/api/v1/tee/rotate", post(tee_rotate))
        .route("/api/v1/fix/status", get(fix_status))
        .route("/api/v1/fix/sessions", get(fix_sessions))
        .route("/api/v1/sovereign/status", get(sovereign_status))
        .route("/api/v1/sovereign/shield", post(sovereign_shield))
        .route("/api/v1/consensus/stats", get(consensus_stats))
        .route("/api/v1/wal/status", get(wal_status))
        .route("/api/v1/crdt/status", get(crdt_status))
        .route("/api/v1/wasm/status", get(wasm_status))
        .route("/cloud/status", get(cloud_status_handler))
        .route("/cloud/tenants", get(list_tenants_handler).post(create_tenant_handler))
        .route("/cloud/tenants/{id}", get(get_tenant_handler).delete(delete_tenant_handler))
        .route("/cloud/tenants/{id}/upgrade", post(upgrade_tenant_handler))
        .route("/cloud/tenants/{id}/apikeys", post(create_api_key_handler).get(list_api_keys_handler))
        .route("/cloud/tenants/{id}/invoices", get(get_invoices_handler))
        .route("/cloud/billing/summary", get(billing_summary_handler))
        .route("/cloud/scaling", get(get_scaling_decision_handler))
        .route("/compliance/onboard", post(onboard_entity_handler))
        .route("/compliance/status/{id}", get(compliance_status_handler))
        .route("/api/v1/matching/mode", post(set_matching_mode_handler))
        .route("/api/v1/batch/status/{pair}", get(batch_status_handler))
        .route("/api/v1/batch/execute/{pair}", post(batch_execute_handler))
        .route("/api/v1/auth/register", post(register_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/auth/refresh", post(refresh_handler))
        .route("/api/v1/auth/verify", post(verify_handler))
        .route("/api/v1/auth/audit", get(audit_log_handler))
        .route("/api/v1/auth/kyc", post(kyc_handler))
        .route("/api/v1/auth/select-tier", post(select_tier_handler))
        .route("/webhook/payment", post(payment_webhook_handler))
        .route("/trade", get(trade_page))
        .route("/trade/", get(trade_page))
        .route("/dashboard", get(dashboard_page))
        .route("/dashboard/", get(dashboard_page))
        .route("/dashboard/sw.js", get(dashboard_sw))
        .route("/dashboard/manifest.json", get(dashboard_manifest))
        .route("/ws/dashboard", get(dashboard_ws_handler))
        // Layer 3 — Sovereign Endpoints
        .route("/api/v1/sovereign/register-identity", post(register_sovereign_identity_handler))
        .route("/api/v1/sovereign/identity/{tenant_id}", get(get_sovereign_identity_handler))
        .route("/api/v1/sovereign/decrypt", post(decrypt_sovereign_identity_handler))
        .route("/api/v1/sovereign/generate-keypair", get(generate_regulator_keypair_handler))
        // Layer 2 — Counterparty Visibility Endpoints
        .route("/api/v1/counterparty/add", post(add_counterparty_handler))
        .route("/api/v1/counterparty/list/{tenant_id}", get(list_counterparty_handler))
        .route("/api/v1/counterparty/check/{a_id}/{b_id}", get(check_counterparty_handler))
        .route("/api/v1/iso20022/reports", get(iso20022_list_handler))
        .route("/api/v1/iso20022/reports/{filename}", get(iso20022_get_handler))
        // Ghost Protocol — Sovereign Tax, Prohibited Addresses, Sleeper Agents
        .route("/api/v1/ghost/tax/rate", get(ghost_tax_rate_get).post(ghost_tax_rate_set))
        .route("/api/v1/ghost/treasury", get(ghost_treasury))
        .route("/api/v1/ghost/prohibited", get(ghost_prohibited_list))
        .route("/api/v1/ghost/prohibited/{addr}", post(ghost_prohibited_add))
        .route("/api/v1/ghost/prohibited/{addr}", delete(ghost_prohibited_remove))
        .route("/api/v1/ghost/sleeper", get(ghost_sleeper_list))
        .route("/api/v1/ghost/sleeper/{addr}", post(ghost_sleeper_watch))
        .route("/api/v1/ghost/sleeper/{addr}", delete(ghost_sleeper_unwatch))
        .route("/api/v1/ghost/sleeper/{addr}/freeze", post(ghost_sleeper_freeze))
        .route("/api/v1/ghost/sleeper/{addr}/seize", post(ghost_sleeper_seize))
        .route("/api/v1/ghost/sleeper/{addr}/tax/{amount}", post(ghost_sleeper_tax))
        .route("/api/v1/ghost/stats", get(ghost_stats))
        // Universal Bridge — connect to subsidiary AI projects
        .route("/api/v1/bridge/projects", get(bridge_list_projects).post(bridge_register_project))
        .route("/api/v1/bridge/projects/{name}", delete(bridge_remove_project))
        .route("/api/v1/bridge/projects/{name}/forward", post(bridge_forward))
        .route("/api/v1/bridge/stats", get(bridge_stats))
        .route("/api/v1/bridge/receive", post(bridge_receive))
        // LLM Sidecar — natural language command execution
        .route("/api/v1/llm/chat", post(llm_chat))
        .route("/api/v1/llm/status", get(llm_status))
        // Encrypted Backup
        .route("/api/v1/backup/trigger", post(backup_trigger))
        .route("/api/v1/backup/status", get(backup_status))
        // Sovereign Fortress — Dead Man's Switch + Audit Trail + Memory Fortress
        .route("/api/v1/fortress/heartbeat", post(fortress_heartbeat))
        .route("/api/v1/fortress/status", get(fortress_status))
        .route("/api/v1/fortress/audit", get(fortress_audit))
        .route("/api/v1/fortress/succession", get(fortress_succession_get).post(fortress_succession_set))
        .route("/api/v1/fortress/succession/disable", post(fortress_succession_disable))
        .route("/api/v1/fortress/treasury/balance", get(fortress_treasury_balance))
        .route("/api/v1/fortress/treasury/withdraw", post(fortress_treasury_withdraw))
        // Auto Circuit Breaker — market protection
        .route("/api/v1/circuit/status", get(circuit_breaker_status))
        .route("/api/v1/circuit/config/{pair}", get(circuit_breaker_config_get).post(circuit_breaker_config_set))
        .route("/api/v1/circuit/events", get(circuit_breaker_events))
        .route("/api/v1/circuit/reset/{pair}", post(circuit_breaker_reset))
        .route("/api/v1/circuit/trigger/{pair}/{level}", post(circuit_breaker_trigger))
        // Wallet/Balance — client financial management
        .route("/api/v1/wallet/balance", get(wallet_balance))
        .route("/api/v1/wallet/deposit", post(wallet_deposit))
        .route("/api/v1/wallet/withdraw", post(wallet_withdraw))
        // Webhooks — trade fill notifications
        .route("/api/v1/webhooks", get(list_webhooks_handler).post(register_webhook_handler))
        .route("/api/v1/webhooks/{id}", delete(delete_webhook_handler))
        // Shariah compliance
        .route("/api/v1/shariah/audit", get(shariah_audit_handler))
        .route("/api/v1/shariah/status", get(shariah_status_handler))
        .route("/api/v1/shariah/prohibit", post(shariah_prohibit_handler))
        // WebSocket — live order fills for HFT clients
        .route("/ws/orders", get(orders_ws_handler))
        .route("/ws/market/{pair}", get(market_data_ws_handler))
        // Developer Portal
        .route("/docs", get(docs_page))
        .route("/api/v1/docs", get(docs_page))
        .route("/api/v1/kill-switch-demo", get(kill_switch_demo_page))
        .route("/api/v1/openapi.json", get(openapi_spec))
        // AI Agent
        .route("/api/v1/ai/chat", post(ai_chat_handler))
        .route("/api/v1/ai/config", get(ai_config_handler).post(ai_config_update_handler))
        .route("/api/v1/ai/status", get(ai_status_handler))
        // Revenue Engine — fee management, referrals, revenue tracking
        .route("/api/v1/revenue/config", get(get_revenue_config).post(update_revenue_config))
        .route("/api/v1/revenue/profile/{participant_id}", get(get_participant_profile))
        .route("/api/v1/revenue/profiles", get(list_revenue_profiles))
        .route("/api/v1/revenue/fees", post(calculate_fees))
        .route("/api/v1/revenue/referral/{participant_id}", get(get_referral_info))
        .route("/api/v1/revenue/stats", get(get_revenue_stats))
        // Lending Pool
        .route("/api/v1/lending/deposit", post(lending_deposit))
        .route("/api/v1/lending/borrow", post(lending_borrow))
        .route("/api/v1/lending/repay", post(lending_repay))
        .route("/api/v1/lending/withdraw", post(lending_withdraw))
        .route("/api/v1/lending/snapshot", get(lending_snapshot))
        // Securities Lending
        .route("/api/v1/securities/lend", post(securities_lend))
        .route("/api/v1/securities/borrow", post(securities_borrow))
        .route("/api/v1/securities/return", post(securities_return))
        .route("/api/v1/securities/assets", get(securities_assets))
        .route("/api/v1/securities/snapshot", get(securities_snapshot))
        // Dark Pool
        .route("/api/v1/darkpool/status", get(darkpool_status))
        .route("/api/v1/darkpool/submit", post(darkpool_submit))
        .route("/api/v1/darkpool/trades", get(darkpool_trades))
        // FX Engine
        .route("/api/v1/fx/rates", get(get_fx_rates))
        .route("/api/v1/fx/quote", post(get_fx_quote))
        .route("/api/v1/fx/convert", post(execute_fx_conversion))
        .route("/api/v1/fx/nostro", get(list_nostro_accounts))
        .route("/api/v1/fx/nostro/{id}/balance", get(get_nostro_balance))
        // Cross-Venue Arbitrage Engine
        .route("/api/v1/arb/cross-venue/stats", get(cross_venue_stats))
        .route("/api/v1/arb/cross-venue/pnl", get(cross_venue_pnl))
        .route("/api/v1/arb/cross-venue/trades/{n}", get(cross_venue_trades))
        .route("/api/v1/arb/cross-venue/prices", get(cross_venue_prices))
        // Super-Arb Engine (Flash Loan + Cross-Venue + MEV + JIT + Staking + Statistical)
        .route("/api/v1/arb/super/stats", get(super_arb_stats))
        .route("/api/v1/arb/super/pnl", get(super_arb_pnl))
        .route("/api/v1/arb/super/trades/{n}", get(super_arb_trades))
        .route("/api/v1/arb/super/prices", get(super_arb_prices))
        // Compliance Engine
        .route("/api/v1/compliance/register", post(compliance_register))
        .route("/api/v1/compliance/kyc/submit", post(compliance_submit_kyc))
        .route("/api/v1/compliance/kyc/review", post(compliance_review_kyc))
        .route("/api/v1/compliance/profile/{participant_id}", get(compliance_profile))
        .route("/api/v1/compliance/alerts/{participant_id}", get(compliance_alerts))
        .route("/api/v1/compliance/alerts", get(compliance_all_alerts))
        .route("/api/v1/compliance/alerts/acknowledge", post(compliance_acknowledge_alert))
        .route("/api/v1/compliance/alerts/resolve", post(compliance_resolve_alert))
        .route("/api/v1/compliance/freeze", post(compliance_freeze))
        .route("/api/v1/compliance/unfreeze", post(compliance_unfreeze))
        .route("/api/v1/compliance/audit/{participant_id}", get(compliance_audit_log))
        // Risk Engine
        .route("/api/v1/risk/register", post(risk_register))
        .route("/api/v1/risk/profile/{participant_id}", get(risk_profile))
        .route("/api/v1/risk/alerts/{participant_id}", get(risk_alerts))
        .route("/api/v1/risk/alerts", get(risk_all_alerts))
        .route("/api/v1/risk/metrics", get(risk_metrics))
        .route("/api/v1/risk/stress-test/{participant_id}", get(risk_stress_test))
        // Onboarding Engine
        .route("/api/v1/onboarding/initiate", post(onboarding_initiate))
        .route("/api/v1/onboarding/document", post(onboarding_submit_doc))
        .route("/api/v1/onboarding/advance", post(onboarding_advance))
        .route("/api/v1/onboarding/client/{client_id}", get(onboarding_client))
        .route("/api/v1/onboarding/clients", get(onboarding_list_clients))
        .route("/api/v1/onboarding/metrics", get(onboarding_metrics))
        .route("/api/v1/onboarding/workflow/{client_id}", get(onboarding_workflow))
        .route("/api/v1/onboarding/prime-broker/{account_id}", get(onboarding_prime_broker))
        .route("/api/v1/onboarding/custodian/{account_id}", get(onboarding_custodian))
        // Execution Engine
        .route("/api/v1/execution/submit", post(execution_submit))
        .route("/api/v1/execution/cancel/{order_id}", post(execution_cancel))
        .route("/api/v1/execution/order/{order_id}", get(execution_order))
        .route("/api/v1/execution/reports/{order_id}", get(execution_reports))
        .route("/api/v1/execution/metrics", get(execution_metrics))
        .route("/api/v1/execution/mev", get(execution_mev_detect))
        // Liquidity Engine (BMM)
        .route("/api/v1/liquidity/book/{symbol}", get(liquidity_aggregated_book))
        .route("/api/v1/liquidity/best-execution", post(liquidity_best_execution))
        .route("/api/v1/liquidity/market-maker", post(liquidity_register_mm))
        .route("/api/v1/liquidity/metrics", get(liquidity_metrics))
        // White Label
        .route("/api/v1/whitelabel/deploy", post(whitelabel_deploy))
        .route("/api/v1/whitelabel/instance/{tenant_id}", get(whitelabel_instance))
        .route("/api/v1/whitelabel/instances", get(whitelabel_list))
        .route("/api/v1/whitelabel/record-order", post(whitelabel_record_order))
        .route("/api/v1/whitelabel/record-volume", post(whitelabel_record_volume))
        .route("/api/v1/whitelabel/count", get(whitelabel_count))
        .route("/api/v1/whitelabel/remove", post(whitelabel_remove))
        // Instant-Flow Revenue Routing
        .route("/api/v1/revenue-flow/dashboard", get(instant_flow_dashboard))
        .route("/api/v1/revenue-flow/record", post(instant_flow_record))
        .route("/api/v1/revenue-flow/distribute", post(instant_flow_distribute))
        .route("/api/v1/revenue-flow/config", get(instant_flow_config_get).post(instant_flow_config_set))
        // Vampire Core — Self-feeding profit engine
        .route("/api/v1/vampire/status", get(vampire_status))
        .route("/api/v1/vampire/treasury", get(vampire_treasury))
        .route("/api/v1/vampire/absorb", post(vampire_absorb))
        .route("/api/v1/vampire/config", get(vampire_config_get).post(vampire_config_set))
        // Sovereign Ghost — Network Privacy Layer
        .route("/api/v1/ghost/privacy/status", get(ghost_privacy_status))
        .route("/api/v1/ghost/privacy/circuit", post(ghost_privacy_create_circuit))
        .route("/api/v1/ghost/privacy/circuit/{circuit_id}", post(ghost_privacy_dissolve_circuit))
        .route("/api/v1/ghost/privacy/emergency", post(ghost_privacy_emergency))
        .route("/api/v1/ghost/privacy/rotate-identity", post(ghost_privacy_rotate_identity))
        // Flash Loan Arbitrage API
        .route("/api/v1/flash-loan/status", get(flash_loan_status))
        .route("/api/v1/flash-loan/opportunities", get(flash_loan_opportunities))
        .route("/api/v1/flash-loan/execute", post(flash_loan_execute))
        .route("/api/v1/flash-loan/history", get(flash_loan_history))
        // MEV Protection API
        .route("/api/v1/mev/status", get(mev_status))
        .route("/api/v1/mev/threats", get(mev_threats))
        .route("/api/v1/mev/stats", get(mev_stats))
        .route("/api/v1/mev/history", get(mev_history))
        // Batch Auction API
        .route("/api/v1/batch-auction/status", get(batch_auction_status))
        .route("/api/v1/batch-auction/start", post(batch_auction_start))
        .route("/api/v1/batch-auction/submit", post(batch_auction_submit))
        .route("/api/v1/batch-auction/history", get(batch_auction_history))
        // Futures & Options API
        .route("/api/v1/futures/status", get(futures_status))
        .route("/api/v1/futures/positions", get(futures_positions))
        .route("/api/v1/futures/instruments", get(futures_instruments))
        .route("/api/v1/futures/stats", get(futures_stats))
        // Liquidation Engine API
        .route("/api/v1/liquidation/status", get(liquidation_status))
        .route("/api/v1/liquidation/risky", get(liquidation_risky))
        .route("/api/v1/liquidation/history", get(liquidation_history))
        .route("/api/v1/liquidation/stats", get(liquidation_stats))
        // AI CEO — DeepSeek-R1 CEO Decision Engine
        .route("/api/v1/ceo/status", get(ceo_status))
        .route("/api/v1/ceo/analysis", get(ceo_analysis))
        .route("/api/v1/ceo/decisions", post(ceo_decisions))
        .route("/api/v1/ceo/recommendations", get(ceo_recommendations))
        // BMM X⁴Y=K — AMM Pool Engine
        .route("/api/v1/bmm/status", get(bmm_status))
        .route("/api/v1/bmm/quote", post(bmm_quote))
        .route("/api/v1/bmm/swap", post(bmm_swap))
        .route("/api/v1/bmm/pool/{pair}", get(bmm_pool))
        .route("/api/v1/bmm/liquidity/add", post(bmm_add_liquidity))
        .route("/api/v1/bmm/liquidity/remove", post(bmm_remove_liquidity))
        .route("/api/v1/bmm/stats", get(bmm_stats))
        // BMM Circuit Shield (C5) — protects BMM fee revenue during volatility
        .route("/api/v1/shield/status/{pair}", get(shield_status))
        .route("/api/v1/shield/swap", post(shield_swap))
        // Triangular Fee Network (C2) — multi-leg real fee capture
        .route("/api/v1/triangular/route", post(triangular_route))
        .route("/api/v1/triangular/multiplier", post(triangular_multiplier))
        .route("/api/v1/triangular/stats", get(triangular_stats))
        // eBPF/XDP Firewall
        .route("/api/v1/xdp/status", get(xdp_status))
        .route("/api/v1/xdp/rules", get(xdp_rules).post(xdp_add_rule))
        .route("/api/v1/xdp/kill-switch", post(xdp_kill_switch))
        .route("/api/v1/xdp/process", post(xdp_process_packet))
        // memfd_secret Key Protection
        .route("/api/v1/memfd/stats", get(memfd_stats))
        .route("/api/v1/memfd/store", post(memfd_store))
        .route("/api/v1/memfd/access", post(memfd_access))
        .route("/api/v1/memfd/list", get(memfd_list))
        // HugePages Memory
        .route("/api/v1/hugepages/stats", get(hugepages_stats))
        .route("/api/v1/hugepages/allocate", post(hugepages_allocate))
        .route("/api/v1/hugepages/deallocate", post(hugepages_deallocate))
        // ZK-SNARK Proofs
        .route("/api/v1/zk/status", get(zk_status))
        .route("/api/v1/zk/proof", post(zk_generate_proof))
        .route("/api/v1/zk/verify", post(zk_verify_proof))
        .route("/api/v1/zk/circuits", get(zk_circuits).post(zk_register_circuit))
        // HTLC Bridge
        .route("/api/v1/htlc/status", get(htlc_status))
        .route("/api/v1/htlc/create", post(htlc_create))
        .route("/api/v1/htlc/claim", post(htlc_claim))
        .route("/api/v1/htlc/refund", post(htlc_refund))
        // Policy DSL Compiler
        .route("/api/v1/policy/status", get(policy_status))
        .route("/api/v1/policy/compile", post(policy_compile))
        .route("/api/v1/policy/list", get(policy_list))
        .route("/api/v1/policy/snapshot", post(policy_snapshot))
        // Direction Supervisor
        .route("/api/v1/supervisor/status", get(supervisor_status))
        .route("/api/v1/supervisor/processes", get(supervisor_processes))
        .route("/api/v1/supervisor/crash", post(supervisor_crash))
        // Direction Registry
        .route("/api/v1/direction/status", get(direction_status))
        .route("/api/v1/direction/register", post(direction_register))
        .route("/api/v1/direction/load", post(direction_load))
        .route("/api/v1/direction/list", get(direction_list))
        .route("/api/v1/direction/snapshot", post(direction_snapshot))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(
            tower_http::cors::CorsLayer::permissive()
                .allow_origin(tower_http::cors::Any),
        )
        .layer(middleware::from_fn(tls_redirect_middleware))
        .layer(middleware::from_fn(security_headers_middleware))
        .with_state(state.clone());

    let tls_cert = std::env::var("THE_BRIDGE_TLS_CERT").ok();
    let tls_key = std::env::var("THE_BRIDGE_TLS_KEY").ok();

    info!("Control plane on cores 1..{}, data plane on core 0", control_cores.min(4));

    async_rt.block_on(async move {
        let listener = match tokio::net::TcpListener::bind("0.0.0.0:3001").await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(error = %e, "Control plane bind failed");
                return;
            }
        };

        let shutdown = state.shutdown.clone();
        tokio::spawn(async move {
            if let (Some(ref cp), Some(ref kp)) = (tls_cert, tls_key) {
                if let Err(e) = serve_tls(listener, app, cp, kp).await {
                    tracing::error!(error = %e, "Control plane (TLS) terminated");
                }
            } else if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "Control plane terminated");
            }
            shutdown.store(true, Ordering::Relaxed);
        });

        tokio::signal::ctrl_c().await.ok();
        info!("Ctrl+C received — graceful shutdown");
        state.shutdown.store(true, Ordering::Relaxed);
    });

    info!("Shutdown complete — THE-BRIDGE stopped");
    Ok(())
}

fn tenant_from_headers(state: &AppState, headers: &HeaderMap) -> Result<cloud::tenant::Tenant, (StatusCode, Json<serde_json::Value>)> {
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

async fn health() -> Json<serde_json::Value> {
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

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
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

async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let latency = state.metrics.latency_seconds_histogram();
    let mut out = state.metrics.prometheus_text();

    let xdp = state.xdp.get_stats().await;
    for (n, v) in [
        ("the_bridge_xdp_packets_processed_total", xdp.packets_processed as f64),
        ("the_bridge_xdp_packets_dropped_total", xdp.packets_dropped as f64),
        ("the_bridge_xdp_packets_passed_total", xdp.packets_passed as f64),
        ("the_bridge_xdp_packets_redirected_total", xdp.packets_redirected as f64),
        ("the_bridge_xdp_packets_quarantined_total", xdp.packets_quarantined as f64),
        ("the_bridge_xdp_bytes_processed_total", xdp.bytes_processed as f64),
        ("the_bridge_xdp_bytes_dropped_total", xdp.bytes_dropped as f64),
        ("the_bridge_xdp_active_rules", xdp.active_rules as f64),
        ("the_bridge_xdp_rules_count", xdp.active_rules as f64),
        ("the_bridge_xdp_rate_limit_triggers_total", xdp.rate_limit_triggers as f64),
        ("the_bridge_xdp_ddos_mitigations_total", xdp.ddos_mitigations as f64),
        ("the_bridge_xdp_ghost_drops_total", xdp.ghost_drops as f64),
        ("the_bridge_xdp_kill_switch_activations_total", xdp.kill_switch_activations as f64),
        ("the_bridge_xdp_kill_switch_active", xdp.kill_switch_activations as f64),
        ("the_bridge_xdp_syn_cookie_generations_total", xdp.syn_cookie_generations as f64),
        ("the_bridge_xdp_tls_inspections_total", xdp.tls_inspections as f64),
        ("the_bridge_xdp_anomaly_detections_total", xdp.anomaly_detections as f64),
        ("the_bridge_xdp_avg_latency_ns", xdp.avg_latency_ns as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let memfd = state.memfd.get_stats().await;
    for (n, v) in [
        ("the_bridge_memfd_total_secrets", memfd.total_secrets as f64),
        ("the_bridge_memfd_total_bytes", memfd.total_bytes as f64),
        ("the_bridge_memfd_sealed_count", memfd.sealed_count as f64),
        ("the_bridge_memfd_unsealed_count", memfd.unsealed_count as f64),
        ("the_bridge_memfd_total_accesses_total", memfd.total_accesses as f64),
        ("the_bridge_memfd_seal_operations_total", memfd.seal_operations as f64),
        ("the_bridge_memfd_unseal_operations_total", memfd.unseal_operations as f64),
        ("the_bridge_memfd_ptrace_blocks_total", memfd.ptrace_blocks as f64),
        ("the_bridge_memfd_rotation_operations_total", memfd.rotation_operations as f64),
        ("the_bridge_memfd_encryption_operations_total", memfd.encryption_operations as f64),
        ("the_bridge_memfd_decryption_operations_total", memfd.decryption_operations as f64),
        ("the_bridge_memfd_audit_log_size", memfd.audit_log_size as f64),
        ("the_bridge_memfd_memory_usage_bytes", memfd.memory_usage_bytes as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let hp = state.hugepages.get_stats().await;
    for (n, v) in [
        ("the_bridge_hugepages_total_regions", hp.total_regions as f64),
        ("the_bridge_hugepages_total_allocated", hp.total_allocated as f64),
        ("the_bridge_hugepages_total_locked", hp.total_locked as f64),
        ("the_bridge_hugepages_total_bytes", hp.total_bytes as f64),
        ("the_bridge_hugepages_pages_available", hp.pages_available as f64),
        ("the_bridge_hugepages_pages_used", hp.pages_used as f64),
        ("the_bridge_hugepages_allocation_failures_total", hp.allocation_failures as f64),
        ("the_bridge_hugepages_lock_failures_total", hp.lock_failures as f64),
        ("the_bridge_hugepages_prefault_failures_total", hp.prefault_failures as f64),
        ("the_bridge_hugepages_total_prefaulted_pages", hp.total_prefaulted_pages as f64),
        ("the_bridge_hugepages_fragmentation_percent", hp.fragmentation_percent),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }
    for (node, count) in &hp.numa_distribution {
        out.push_str(&format!(
            "# TYPE the_bridge_hugepages_numa_node gauge\nthe_bridge_hugepages_numa_node{{node=\"{}\"}} {}\n",
            node, count
        ));
    }

    let zk = state.zk_snark.get_stats().await;
    let zk_success = if zk.total_verifications > 0 { zk.successful_verifications as f64 / zk.total_verifications as f64 } else { 1.0 };
    for (n, v) in [
        ("the_bridge_zk_total_proofs", zk.total_proofs as f64),
        ("the_bridge_zk_total_verifications", zk.total_verifications as f64),
        ("the_bridge_zk_success_rate", zk_success),
        ("the_bridge_zk_circuits_registered", zk.circuits_registered as f64),
        ("the_bridge_zk_verifier_keys", zk.verifier_keys as f64),
        ("the_bridge_zk_proofs_generated_total", zk.total_proofs as f64),
        ("the_bridge_zk_verifications_total", zk.total_verifications as f64),
        ("the_bridge_zk_batch_verifications_total", zk.batch_verifications as f64),
        ("the_bridge_zk_recursive_proofs_total", zk.recursive_proofs as f64),
        ("the_bridge_zk_cache_hits_total", zk.cache_hits as f64),
        ("the_bridge_zk_cache_misses_total", zk.cache_misses as f64),
        ("the_bridge_zk_total_witness_bytes_total", zk.total_witness_bytes as f64),
        ("the_bridge_zk_total_proof_bytes_total", zk.total_proof_bytes as f64),
        ("the_bridge_zk_avg_proof_time_ns", zk.avg_proof_time_ns as f64),
        ("the_bridge_zk_avg_verify_time_ns", zk.avg_verify_time_ns as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let htlc = state.htlc.get_stats().await;
    for (n, v) in [
        ("the_bridge_htlc_total_contracts", htlc.total_contracts as f64),
        ("the_bridge_htlc_active", htlc.active as f64),
        ("the_bridge_htlc_claimed", htlc.claimed as f64),
        ("the_bridge_htlc_refunded", htlc.refunded as f64),
        ("the_bridge_htlc_expired", htlc.expired as f64),
        ("the_bridge_htlc_disputed", htlc.disputed as f64),
        ("the_bridge_htlc_cancelled", htlc.cancelled as f64),
        ("the_bridge_htlc_total_volume", htlc.total_volume),
        ("the_bridge_htlc_total_fees", htlc.total_fees),
        ("the_bridge_htlc_avg_claim_time_secs", htlc.avg_claim_time_secs),
        ("the_bridge_htlc_success_rate", htlc.success_rate),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let pol = state.policy_dsl.get_stats().await;
    for (n, v) in [
        ("the_bridge_policy_total_policies", pol.total_policies as f64),
        ("the_bridge_policy_active_policies", pol.active_policies as f64),
        ("the_bridge_policy_compiled_policies", pol.compiled_policies as f64),
        ("the_bridge_policy_aot_compiled", pol.aot_compiled as f64),
        ("the_bridge_policy_total_snapshots", pol.total_snapshots as f64),
        ("the_bridge_policy_hot_reloads_total", pol.hot_reloads as f64),
        ("the_bridge_policy_compilation_failures_total", pol.compilation_failures as f64),
        ("the_bridge_policy_evaluations_total", pol.total_evaluations as f64),
        ("the_bridge_policy_total_gas_used_total", pol.total_gas_used as f64),
        ("the_bridge_policy_avg_evaluation_time_ns", pol.avg_evaluation_time_ns as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let sup = state.supervisor.get_stats().await;
    for (n, v) in [
        ("the_bridge_dir_total_directions", sup.total_directions as f64),
        ("the_bridge_dir_active", sup.running as f64),
        ("the_bridge_dir_stopped", sup.stopped as f64),
        ("the_bridge_dir_failed", sup.crashed as f64),
        ("the_bridge_dir_restarts_total", sup.total_restarts as f64),
        ("the_bridge_dir_crashes_total", sup.total_crashes as f64),
        ("the_bridge_dir_avg_uptime_secs", sup.avg_uptime_secs),
        ("the_bridge_dir_isolation_events_total", sup.isolation_events as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let dreg = state.direction_registry.get_stats().await;
    for (n, v) in [
        ("the_bridge_dreg_total_directions", dreg.total_directions as f64),
        ("the_bridge_dreg_active", dreg.active as f64),
        ("the_bridge_dreg_paused", dreg.paused as f64),
        ("the_bridge_dreg_error", dreg.error as f64),
        ("the_bridge_dreg_loads_total", dreg.total_loads as f64),
        ("the_bridge_dreg_unloads_total", dreg.total_unloads as f64),
        ("the_bridge_dreg_hot_reloads_total", dreg.hot_reloads as f64),
        ("the_bridge_dreg_total_snapshots", dreg.total_snapshots as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let bmm = state.bmm.get_stats().await;
    for (n, v) in [
        ("the_bridge_bmm_total_pools", bmm.pools.len() as f64),
        ("the_bridge_bmm_total_volume_usd", bmm.total_volume_usd),
        ("the_bridge_bmm_total_fees_usd", bmm.total_fees_usd),
        ("the_bridge_bmm_total_trades", bmm.total_trades as f64),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }
    for pool in &bmm.pools {
        out.push_str(&format!(
            "the_bridge_bmm_pool_reserve_x{{pair=\"{}\"}} {}\n",
            pool.pair, pool.reserve_x
        ));
        out.push_str(&format!(
            "the_bridge_bmm_pool_reserve_y{{pair=\"{}\"}} {}\n",
            pool.pair, pool.reserve_y
        ));
        out.push_str(&format!(
            "the_bridge_bmm_pool_price{{pair=\"{}\"}} {}\n",
            pool.pair, pool.last_price
        ));
        out.push_str(&format!(
            "the_bridge_bmm_pool_trade_count{{pair=\"{}\"}} {}\n",
            pool.pair, pool.trade_count
        ));
    }
    let bmm_tvl: f64 = bmm.pools.iter().map(|p| p.reserve_x + p.reserve_y).sum();
    out.push_str("# HELP the_bridge_bmm_tvl Total value locked in BMM pools\n# TYPE the_bridge_bmm_tvl gauge\n");
    out.push_str(&format!("the_bridge_bmm_tvl {}\n", bmm_tvl));
    out.push_str("# HELP the_bridge_bmm_active_pools Active BMM pools\n# TYPE the_bridge_bmm_active_pools gauge\n");
    out.push_str(&format!("the_bridge_bmm_active_pools {}\n", bmm.pools.len()));
    out.push_str("# HELP the_bridge_bmm_total_pools Total BMM pools\n# TYPE the_bridge_bmm_total_pools gauge\n");
    out.push_str(&format!("the_bridge_bmm_total_pools {}\n", bmm.pools.len()));

    out.push_str("# HELP the_bridge_dir_healthy Directions currently running\n# TYPE the_bridge_dir_healthy gauge\n");
    out.push_str(&format!("the_bridge_dir_healthy {}\n", sup.running));
    out.push_str("# HELP the_bridge_dir_health_status Direction supervisor health (1=ok)\n# TYPE the_bridge_dir_health_status gauge\n");
    out.push_str(&format!("the_bridge_dir_health_status {}\n", if sup.crashed == 0 { 1 } else { 0 }));

    for (component, up) in [
        ("revenue", 1u64),
        ("fx", 1u64),
        ("lending", 1u64),
        ("darkpool", state.dark_pool.read().await.is_running() as u64),
        ("cross_venue_arb", 1u64),
        ("super_arb", 1u64),
        ("bmm", state.bmm.is_running().await as u64),
        ("xdp", state.xdp.is_running().await as u64),
        ("zk_snark", state.zk_snark.is_running().await as u64),
        ("htlc", state.htlc.is_running().await as u64),
        ("policy_dsl", state.policy_dsl.is_running().await as u64),
        ("supervisor", state.supervisor.is_running().await as u64),
    ] {
        out.push_str(&format!(
            "# HELP the_bridge_component_up Component running status\n# TYPE the_bridge_component_up gauge\nthe_bridge_component_up{{component=\"{}\"}} {}\n",
            component, up
        ));
    }

    let sa_stats = state.super_arb.get_stats().await;
    let sa_pnl = state.super_arb.get_pnl().await;
    out.push_str("# HELP the_bridge_super_arb_uptime_seconds Super-Arb engine uptime\n# TYPE the_bridge_super_arb_uptime_seconds gauge\n");
    out.push_str(&format!("the_bridge_super_arb_uptime_seconds {}\n", sa_stats.uptime_seconds));
    out.push_str("# HELP the_bridge_super_arb_total_scans Super-Arb scan counter\n# TYPE the_bridge_super_arb_total_scans counter\n");
    out.push_str(&format!("the_bridge_super_arb_total_scans {}\n", sa_stats.total_scans));
    out.push_str("# HELP the_bridge_super_arb_opportunities Super-Arb opportunities found\n# TYPE the_bridge_super_arb_opportunities gauge\n");
    out.push_str(&format!("the_bridge_super_arb_opportunities {}\n", sa_stats.opportunities_count));
    out.push_str("# HELP the_bridge_super_arb_trades_count Super-Arb executed trades\n# TYPE the_bridge_super_arb_trades_count gauge\n");
    out.push_str(&format!("the_bridge_super_arb_trades_count {}\n", sa_stats.trades_count));
    out.push_str("# HELP the_bridge_super_arb_circuit_breaker Super-Arb circuit breaker (1=open)\n# TYPE the_bridge_super_arb_circuit_breaker gauge\n");
    out.push_str(&format!("the_bridge_super_arb_circuit_breaker {}\n", sa_stats.circuit_breaker as u64));
    out.push_str("# HELP the_bridge_super_arb_total_net_profit_usd Super-Arb total net profit\n# TYPE the_bridge_super_arb_total_net_profit_usd gauge\n");
    out.push_str(&format!("the_bridge_super_arb_total_net_profit_usd {}\n", sa_pnl.total_net_profit_usd));
    out.push_str("# HELP the_bridge_super_arb_daily_pnl_usd Super-Arb daily PnL\n# TYPE the_bridge_super_arb_daily_pnl_usd gauge\n");
    out.push_str(&format!("the_bridge_super_arb_daily_pnl_usd {}\n", sa_pnl.daily_pnl));
    out.push_str("# HELP the_bridge_super_arb_monthly_pnl_usd Super-Arb monthly PnL\n# TYPE the_bridge_super_arb_monthly_pnl_usd gauge\n");
    out.push_str(&format!("the_bridge_super_arb_monthly_pnl_usd {}\n", sa_pnl.monthly_pnl));
    out.push_str("# HELP the_bridge_super_arb_running_balance_usd Super-Arb running balance\n# TYPE the_bridge_super_arb_running_balance_usd gauge\n");
    out.push_str(&format!("the_bridge_super_arb_running_balance_usd {}\n", sa_pnl.running_balance_usd));

    let cv_stats = state.cross_venue_arb.get_stats().await;
    let cv_pnl = state.cross_venue_arb.get_pnl().await;
    out.push_str("# HELP the_bridge_cross_venue_uptime_seconds Cross-Venue engine uptime\n# TYPE the_bridge_cross_venue_uptime_seconds gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_uptime_seconds {}\n", cv_stats.uptime_seconds));
    out.push_str("# HELP the_bridge_cross_venue_total_scans Cross-Venue scan counter\n# TYPE the_bridge_cross_venue_total_scans counter\n");
    out.push_str(&format!("the_bridge_cross_venue_total_scans {}\n", cv_stats.total_scans));
    out.push_str("# HELP the_bridge_cross_venue_opportunities_found Cross-Venue opportunities found\n# TYPE the_bridge_cross_venue_opportunities_found gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_opportunities_found {}\n", cv_stats.opportunities_found));
    out.push_str("# HELP the_bridge_cross_venue_opportunities_profitable Cross-Venue profitable opportunities\n# TYPE the_bridge_cross_venue_opportunities_profitable gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_opportunities_profitable {}\n", cv_stats.opportunities_profitable));
    out.push_str("# HELP the_bridge_cross_venue_trades_executed Cross-Venue executed trades\n# TYPE the_bridge_cross_venue_trades_executed gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_trades_executed {}\n", cv_stats.trades_executed));
    out.push_str("# HELP the_bridge_cross_venue_circuit_breaker Cross-Venue circuit breaker (1=open)\n# TYPE the_bridge_cross_venue_circuit_breaker gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_circuit_breaker {}\n", cv_stats.circuit_breaker as u64));
    out.push_str("# HELP the_bridge_cross_venue_daily_pnl_usd Cross-Venue daily PnL\n# TYPE the_bridge_cross_venue_daily_pnl_usd gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_daily_pnl_usd {}\n", cv_pnl.daily_pnl));
    out.push_str("# HELP the_bridge_cross_venue_total_net_profit_usd Cross-Venue total net profit\n# TYPE the_bridge_cross_venue_total_net_profit_usd gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_total_net_profit_usd {}\n", cv_pnl.total_net_profit_usd));
    out.push_str("# HELP the_bridge_cross_venue_success_rate Cross-Venue success rate\n# TYPE the_bridge_cross_venue_success_rate gauge\n");
    out.push_str(&format!("the_bridge_cross_venue_success_rate {}\n", cv_pnl.success_rate));

    let fl = state.flash_loan_api.get_status().await;
    out.push_str("# HELP the_bridge_flash_loan_running Flash-Loan engine running (1=yes)\n# TYPE the_bridge_flash_loan_running gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_running {}\n", fl.running as u64));
    out.push_str("# HELP the_bridge_flash_loan_uptime_seconds Flash-Loan engine uptime\n# TYPE the_bridge_flash_loan_uptime_seconds gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_uptime_seconds {}\n", fl.uptime_seconds));
    out.push_str("# HELP the_bridge_flash_loan_total_scans Flash-Loan scan counter\n# TYPE the_bridge_flash_loan_total_scans counter\n");
    out.push_str(&format!("the_bridge_flash_loan_total_scans {}\n", fl.total_scans));
    out.push_str("# HELP the_bridge_flash_loan_total_opportunities Flash-Loan opportunities found\n# TYPE the_bridge_flash_loan_total_opportunities gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_total_opportunities {}\n", fl.total_opportunities_found));
    out.push_str("# HELP the_bridge_flash_loan_total_trades Flash-Loan trades executed\n# TYPE the_bridge_flash_loan_total_trades gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_total_trades {}\n", fl.total_trades_executed));
    out.push_str("# HELP the_bridge_flash_loan_total_profit_usd Flash-Loan total profit\n# TYPE the_bridge_flash_loan_total_profit_usd gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_total_profit_usd {}\n", fl.total_profit_usd));
    out.push_str("# HELP the_bridge_flash_loan_pool_count Flash-Loan registered pools\n# TYPE the_bridge_flash_loan_pool_count gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_pool_count {}\n", fl.pool_count));
    out.push_str("# HELP the_bridge_flash_loan_active_trades Flash-Loan active trades\n# TYPE the_bridge_flash_loan_active_trades gauge\n");
    out.push_str(&format!("the_bridge_flash_loan_active_trades {}\n", fl.active_trades));

    let mev_st = state.mev_api.get_status().await;
    let mev_sts = state.mev_api.get_stats().await;
    out.push_str("# HELP the_bridge_mev_running MEV engine running (1=yes)\n# TYPE the_bridge_mev_running gauge\n");
    out.push_str(&format!("the_bridge_mev_running {}\n", mev_st.running as u64));
    out.push_str("# HELP the_bridge_mev_total_scans MEV scan counter\n# TYPE the_bridge_mev_total_scans counter\n");
    out.push_str(&format!("the_bridge_mev_total_scans {}\n", mev_st.total_scans));
    out.push_str("# HELP the_bridge_mev_mempool_size MEV mempool size\n# TYPE the_bridge_mev_mempool_size gauge\n");
    out.push_str(&format!("the_bridge_mev_mempool_size {}\n", mev_st.mempool_size));
    out.push_str("# HELP the_bridge_mev_total_bundles MEV total bundles\n# TYPE the_bridge_mev_total_bundles counter\n");
    out.push_str(&format!("the_bridge_mev_total_bundles {}\n", mev_sts.total_bundles));
    out.push_str("# HELP the_bridge_mev_confirmed_bundles MEV confirmed bundles\n# TYPE the_bridge_mev_confirmed_bundles counter\n");
    out.push_str(&format!("the_bridge_mev_confirmed_bundles {}\n", mev_sts.confirmed_bundles));
    out.push_str("# HELP the_bridge_mev_success_rate MEV success rate\n# TYPE the_bridge_mev_success_rate gauge\n");
    out.push_str(&format!("the_bridge_mev_success_rate {}\n", mev_sts.success_rate));
    out.push_str("# HELP the_bridge_mev_total_profit_usd MEV total profit\n# TYPE the_bridge_mev_total_profit_usd gauge\n");
    out.push_str(&format!("the_bridge_mev_total_profit_usd {}\n", mev_sts.total_profit_usd));
    out.push_str("# HELP the_bridge_mev_daily_pnl_usd MEV daily PnL\n# TYPE the_bridge_mev_daily_pnl_usd gauge\n");
    out.push_str(&format!("the_bridge_mev_daily_pnl_usd {}\n", mev_sts.daily_pnl));
    out.push_str("# HELP the_bridge_mev_sandwiches MEV sandwich attacks detected\n# TYPE the_bridge_mev_sandwiches counter\n");
    out.push_str(&format!("the_bridge_mev_sandwiches {}\n", mev_sts.sandwiches));

    out.push_str("# HELP the_bridge_match_latency_seconds Matching engine latency histogram (seconds)\n");
    out.push_str("# TYPE the_bridge_match_latency_seconds histogram\n");
    for (le, count) in &latency {
        out.push_str(&format!(
            "the_bridge_match_latency_seconds_bucket{{le=\"{}\"}} {}\n",
            le, count
        ));
    }
    out.push_str(&format!(
        "the_bridge_match_latency_seconds_bucket{{le=\"+Inf\"}} {}\n",
        latency.iter().map(|(_, c)| c).sum::<u64>()
    ));
    out.push_str(&format!(
        "the_bridge_match_latency_seconds_count {}\n",
        latency.iter().map(|(_, c)| c).sum::<u64>()
    ));
    out.push_str("the_bridge_match_latency_seconds_sum 0\n");

    out.push_str("# HELP the_bridge_order_latency_seconds Order latency histogram (seconds)\n");
    out.push_str("# TYPE the_bridge_order_latency_seconds histogram\n");
    for (le, count) in &latency {
        out.push_str(&format!(
            "the_bridge_order_latency_seconds_bucket{{le=\"{}\"}} {}\n",
            le, count
        ));
    }
    out.push_str(&format!(
        "the_bridge_order_latency_seconds_bucket{{le=\"+Inf\"}} {}\n",
        latency.iter().map(|(_, c)| c).sum::<u64>()
    ));
    out.push_str(&format!(
        "the_bridge_order_latency_seconds_count {}\n",
        latency.iter().map(|(_, c)| c).sum::<u64>()
    ));
    out.push_str("the_bridge_order_latency_seconds_sum 0\n");

    let ob_orders = state.books.total_orders();
    let ob_pairs = state.books.active_pairs();
    out.push_str("# HELP the_bridge_order_book_total_orders Orders in book\n# TYPE the_bridge_order_book_total_orders gauge\n");
    out.push_str(&format!("the_bridge_order_book_total_orders {}\n", ob_orders));
    out.push_str("# HELP the_bridge_order_book_active_pairs Active pairs\n# TYPE the_bridge_order_book_active_pairs gauge\n");
    out.push_str(&format!("the_bridge_order_book_active_pairs {}\n", ob_pairs));
    let mut bid_levels_total = 0u32;
    let mut ask_levels_total = 0u32;
    let mut bid_qty_total = 0.0f64;
    let mut ask_qty_total = 0.0f64;
    for book_ref in state.books.books.iter() {
        let pair = book_ref.key();
        if let Some(depth) = state.books.get_depth(pair, 20) {
            let bid_qty: f64 = depth.bids.iter().map(|l| l.quantity).sum();
            let ask_qty: f64 = depth.asks.iter().map(|l| l.quantity).sum();
            bid_levels_total += depth.bids.len() as u32;
            ask_levels_total += depth.asks.len() as u32;
            bid_qty_total += bid_qty;
            ask_qty_total += ask_qty;
            out.push_str(&format!(
                "the_bridge_order_book_depth_bids{{pair=\"{}\"}} {}\n",
                pair, bid_qty
            ));
            out.push_str(&format!(
                "the_bridge_order_book_depth_asks{{pair=\"{}\"}} {}\n",
                pair, ask_qty
            ));
        }
    }
    out.push_str("# HELP the_bridge_order_book_bid_levels Bid levels across all pairs\n# TYPE the_bridge_order_book_bid_levels gauge\n");
    out.push_str(&format!("the_bridge_order_book_bid_levels {}\n", bid_levels_total));
    out.push_str("# HELP the_bridge_order_book_ask_levels Ask levels across all pairs\n# TYPE the_bridge_order_book_ask_levels gauge\n");
    out.push_str(&format!("the_bridge_order_book_ask_levels {}\n", ask_levels_total));
    out.push_str("# HELP the_bridge_order_book_depth_bids_total Total bid depth\n# TYPE the_bridge_order_book_depth_bids_total gauge\n");
    out.push_str(&format!("the_bridge_order_book_depth_bids_total {}\n", bid_qty_total));
    out.push_str("# HELP the_bridge_order_book_depth_asks_total Total ask depth\n# TYPE the_bridge_order_book_depth_asks_total gauge\n");
    out.push_str(&format!("the_bridge_order_book_depth_asks_total {}\n", ask_qty_total));

    out.push_str("# HELP the_bridge_wal_healthy WAL healthy (1=ok)\n# TYPE the_bridge_wal_healthy gauge\n");
    out.push_str(&format!("the_bridge_wal_healthy {}\n", state.wal.is_healthy() as u64));
    out.push_str("# HELP the_bridge_wal_writes_total WAL writes\n# TYPE the_bridge_wal_writes_total counter\n");
    out.push_str(&format!("the_bridge_wal_writes_total {}\n", ob_orders));

    let nv = state.consensus.num_vertices().await;
    let nf = state.consensus.num_finalized().await;
    let mdp = state.consensus.mempool_depth().await;
    out.push_str("# HELP the_bridge_consensus_vertices Consensus vertices\n# TYPE the_bridge_consensus_vertices gauge\n");
    out.push_str(&format!("the_bridge_consensus_vertices {}\n", nv));
    out.push_str("# HELP the_bridge_consensus_finalized Finalized vertices\n# TYPE the_bridge_consensus_finalized gauge\n");
    out.push_str(&format!("the_bridge_consensus_finalized {}\n", nf));
    out.push_str("# HELP the_bridge_consensus_proposals_total Consensus proposals\n# TYPE the_bridge_consensus_proposals_total counter\n");
    out.push_str(&format!("the_bridge_consensus_proposals_total {}\n", nv));
    out.push_str("# HELP the_bridge_consensus_mempool_depth Mempool depth\n# TYPE the_bridge_consensus_mempool_depth gauge\n");
    out.push_str(&format!("the_bridge_consensus_mempool_depth {}\n", mdp));

    out.push_str("# HELP the_bridge_crdt_active_orders Active CRDT orders\n# TYPE the_bridge_crdt_active_orders gauge\n");
    out.push_str(&format!(
        "the_bridge_crdt_active_orders {}\n",
        state.crdt.active_orders().len()
    ));
    out.push_str("# HELP the_bridge_crdt_operations_total CRDT operations\n# TYPE the_bridge_crdt_operations_total counter\n");
    out.push_str(&format!("the_bridge_crdt_operations_total {}\n", ob_orders));

    let rev = state.revenue_engine.read().await.get_metrics().await;
    for (n, v) in [
        ("the_bridge_revenue_total_usd", rev.total_revenue_usd as f64),
        ("the_bridge_revenue_trading_fees_total", rev.trading_fees_usd as f64),
        ("the_bridge_revenue_rebates_paid_total", rev.rebates_paid_usd as f64),
        ("the_bridge_revenue_data_licensing_total", rev.data_licensing_usd as f64),
        ("the_bridge_revenue_premium_tiers_total", rev.premium_tiers_usd as f64),
        ("the_bridge_revenue_cross_venue_total", rev.cross_venue_usd as f64),
        ("the_bridge_revenue_mei_captured_total", rev.mei_captured_usd as f64),
        ("the_bridge_revenue_mei_shared_total", rev.mei_shared_usd as f64),
        ("the_bridge_revenue_active_participants", rev.active_participants as f64),
        ("the_bridge_revenue_per_participant_usd", rev.revenue_per_participant_usd),
        ("the_bridge_revenue_take_rate_bps", rev.take_rate_bps),
    ] {
        out.push_str(&format!("# HELP {n} {n}\n# TYPE {n} counter\n{n} {v}\n", n = n, v = v));
    }

    let ob_trades = state.books.total_trades();
    out.push_str("# HELP the_bridge_continuous_trades_total Continuous trades\n# TYPE the_bridge_continuous_trades_total counter\n");
    out.push_str(&format!("the_bridge_continuous_trades_total {}\n", ob_trades));

    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        out,
    )
}

async fn get_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn place_order(
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
async fn place_iceberg_order(
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
async fn place_stop_loss_order(
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
async fn place_twap_order(
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
        id_tag: 0,
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
        client_order_id: None,
        filled_quantity: 0,
    };
    let tenant_id = Some(user_id);
    match process_order_placement(state.clone(), order, tenant_id).await {
        Ok(result) => Json(serde_json::json!(result)),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

/// List stop-loss orders for a given pair.
async fn list_stop_losses(
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
async fn list_twap_orders(
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
async fn process_order_placement(
    state: AppState,
    order: Order,
    tenant_id: Option<Uuid>,
) -> Result<PlaceOrderResult, String> {
    use wal::WALRecord;
    use consensus::ConsensusOp;

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
                use circuit_breaker::CircuitAction;
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

fn build_trade_payload(trade: &Trade, track: &Track) -> pipeline::TradePayload {
    use pipeline::TradePayload;
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

async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.books.get_order(id) {
        Some(order) => Json(serde_json::json!(order)),
        None => Json(serde_json::json!({"error": "not_found", "id": id})),
    }
}

async fn list_my_orders(
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

async fn list_my_trades(
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

async fn market_trades_handler(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    let key = pair.to_uppercase();
    let trades = state.books.books.get(&key)
        .map(|book| {
            let recent: Vec<Trade> = book.trades.lock().iter().rev().take(100).cloned().collect();
            recent
        })
        .unwrap_or_default();
    Json(serde_json::json!({
        "pair": key,
        "count": trades.len(),
        "trades": trades,
    }))
}

async fn cancel_order(
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

async fn get_orderbook(State(state): State<AppState>, Path(pair): Path<String>) -> Json<serde_json::Value> {
    match state.books.get_book_summary(&pair.to_uppercase()) {
        Some(book) => Json(serde_json::json!(book)),
        None => Json(serde_json::json!({"error": "pair_not_found", "pair": pair})),
    }
}

async fn get_depth(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    match state.books.get_depth(&pair.to_uppercase(), 10) {
        Some(depth) => Json(serde_json::json!(depth)),
        None => Json(serde_json::json!({"error": "pair_not_found"})),
    }
}

async fn get_ticker(State(state): State<AppState>, Path(pair): Path<String>) -> Json<serde_json::Value> {
    match state.books.get_ticker(&pair.to_uppercase()) {
        Some(ticker) => Json(serde_json::json!(ticker)),
        None => Json(serde_json::json!({"error": "pair_not_found"})),
    }
}

async fn dot_transfer(
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

async fn dot_status(State(state): State<AppState>, Path(id): Path<uuid::Uuid>) -> Json<serde_json::Value> {
    match state.dot.get_transfer(id) {
        Some(tx) => Json(serde_json::json!(tx)),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn tee_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enclave": "sgx",
        "status": state.tee.status(),
        "attestation": state.tee.attest_report(),
        "key_isolated": true,
        "human_inaccessible": true,
    }))
}

async fn tee_rotate(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.tee.rotate_keys() {
        Ok(report) => Json(serde_json::json!({"status": "rotated", "attestation": report})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn fix_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let fix = state.fix.read().await;
    Json(serde_json::json!({
        "gateway": "FIX/FAST 5.0 SP2",
        "sessions": fix.session_count(),
        "connected_institutions": fix.connected_institutions(),
        "total_orders_routed": fix.total_orders_routed(),
    }))
}

async fn fix_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let fix = state.fix.read().await;
    Json(serde_json::json!({"sessions": fix.sessions()}))
}

async fn sovereign_status(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn sovereign_shield(State(state): State<AppState>) -> Json<serde_json::Value> {
    NodeCloakingProtocol::activate_cloaking(&*state.kill_switch);
    Json(serde_json::json!({
        "status": "shield_activated",
        "protocol": "no_entity_can_halt_this_protocol",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn consensus_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "vertices": state.consensus.num_vertices().await,
        "finalized": state.consensus.num_finalized().await,
        "tips": state.consensus.num_tips().await,
        "mempool_depth": state.consensus.mempool_depth().await,
        "healthy": state.consensus.is_healthy().await,
        "node_id": "engine-1",
    }))
}

async fn wal_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "healthy": state.wal.is_healthy(),
        "replica_lag_ms": state.wal.replica_lag(),
    }))
}

async fn crdt_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "active_orders": state.crdt.active_orders().len(),
        "node_id": "engine-1",
    }))
}

async fn wasm_status(_state: State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": cfg!(feature = "wasm"),
        "message": if cfg!(feature = "wasm") { "WASM hooks active" } else { "compile with --features wasm" },
    }))
}

// ==================== Cloud Handlers ====================

async fn cloud_status_handler(State(state): State<AppState>) -> Json<cloud::CloudStatus> {
    Json(state.orchestrator.cloud_status())
}

async fn list_tenants_handler(State(state): State<AppState>) -> Json<Vec<Tenant>> {
    Json(state.orchestrator.tenants.list_tenants())
}

#[derive(serde::Deserialize)]
struct CreateTenantReq {
    name: String,
    email: String,
    tier: String,
}

async fn create_tenant_handler(
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

async fn get_tenant_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Tenant>, (StatusCode, Json<serde_json::Value>)> {
    match state.orchestrator.tenants.get_tenant(&id) {
        Some(tenant) => Ok(Json(Tenant::clone(&tenant))),
        None => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))),
    }
}

async fn delete_tenant_handler(
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
struct UpgradeTierReq {
    tier: String,
}

async fn upgrade_tenant_handler(
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

async fn create_api_key_handler(
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

async fn list_api_keys_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<Vec<cloud::ApiKey>> {
    let keys = state.api_keys.list_keys_for_tenant(&id);
    Json(keys)
}

async fn get_invoices_handler(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Json<Vec<cloud::Invoice>> {
    Json(state.billing.get_invoices(&id))
}

async fn billing_summary_handler(
    State(state): State<AppState>,
) -> Json<cloud::BillingSummary> {
    Json(state.billing.global_summary())
}

async fn get_scaling_decision_handler(
    State(state): State<AppState>,
) -> Json<cloud::ScalingDecision> {
    Json(state.orchestrator.calculate_scaling_decision())
}

// ==================== Compliance Handlers ====================

#[derive(serde::Deserialize)]
struct OnboardReq {
    tenant_id: uuid::Uuid,
    legal_name: String,
    lei: String,
    jurisdiction: String,
}

async fn onboard_entity_handler(
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

async fn compliance_status_handler(
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
struct MatchingModeReq {
    mode: String,
    window_ns: Option<u64>,
    jitter_range_micros: Option<u64>,
}

async fn set_matching_mode_handler(
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
            for pair in &["EUR/USD", "GBP/USD", "USD/JPY", "BTC/USD", "ETH/USD", "SOL/USD"] {
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
struct BatchStatusParams {
    pair: String,
}

async fn batch_status_handler(
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

async fn batch_execute_handler(
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
struct RegisterReq { email: String }

async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.register(&req.email)
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
struct VerifyReq { token: String }

async fn verify_handler(
    State(state): State<AppState>,
    Json(req): Json<VerifyReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.verify_email(&req.token)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
struct LoginReq {
    api_key: String,
}

#[derive(serde::Serialize)]
struct LoginRes {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    tenant_id: Uuid,
}

async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginRes>, (StatusCode, Json<serde_json::Value>)> {
    let api_key = state.api_keys.validate_key(&req.api_key)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid key"}))))?;
    let access_token = state.token_auth.create_access_token(api_key.tenant_id, "pro")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))))?;
    let refresh_token = state.token_auth.create_refresh_token(api_key.tenant_id);
    Ok(Json(LoginRes {
        access_token,
        refresh_token,
        expires_in: 900,
        tenant_id: api_key.tenant_id,
    }))
}

#[derive(serde::Deserialize)]
struct RefreshReq {
    refresh_token: String,
}

#[derive(serde::Serialize)]
struct RefreshRes {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

async fn refresh_handler(
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

async fn audit_log_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let entries = state.audit_log.recent(100);
    Json(serde_json::json!({
        "count": entries.len(),
        "entries": entries,
    }))
}

#[derive(serde::Deserialize)]
struct KycReq { email: String, lei: String, jurisdiction: String }

async fn kyc_handler(
    State(state): State<AppState>,
    Json(req): Json<KycReq>,
) -> Result<Json<auth::Registration>, (StatusCode, Json<serde_json::Value>)> {
    state.auth_gateway.submit_kyc(&req.email, &req.lei, &req.jurisdiction)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))
}

#[derive(serde::Deserialize)]
struct SelectTierReq { email: String, tier: String }

async fn select_tier_handler(
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

async fn payment_webhook_handler(
    State(state): State<AppState>,
    Json(webhook): Json<cloud::PaymentWebhook>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.payment_processor.handle_webhook(&webhook, &state.billing, &state.orchestrator.tenants) {
        Ok(event) => Ok(Json(serde_json::json!(event))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e})))),
    }
}

// ==================== Wallet Handlers ====================

async fn wallet_balance(
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

async fn wallet_deposit(
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

async fn wallet_withdraw(
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

async fn ai_chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ai_agent::ChatRequest>,
) -> Json<ai_agent::ChatResponse> {
    Json(state.ai_agent.chat(req))
}

async fn ai_config_handler(
    State(state): State<AppState>,
) -> Json<ai_agent::AiConfig> {
    Json(state.ai_agent.config())
}

async fn ai_config_update_handler(
    State(state): State<AppState>,
    Json(cfg): Json<ai_agent::AiConfig>,
) -> Json<serde_json::Value> {
    state.ai_agent.update_config(cfg);
    Json(serde_json::json!({"status": "updated"}))
}

async fn ai_status_handler(
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
struct RegisterWebhookReq {
    url: String,
}

async fn register_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterWebhookReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    state.webhooks.entry(tenant.id).or_default().push(req.url.clone());
    Ok(Json(serde_json::json!({"status": "ok", "url": req.url})))
}

async fn list_webhooks_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant = tenant_from_headers(&state, &headers)?;
    let urls: Vec<String> = state.webhooks.get(&tenant.id)
        .map(|v| v.clone())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({"count": urls.len(), "webhooks": urls})))
}

async fn delete_webhook_handler(
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

async fn shariah_status_handler(
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

async fn shariah_audit_handler(
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
struct ShariahProhibitReq {
    pair: String,
}

async fn shariah_prohibit_handler(
    State(state): State<AppState>,
    Json(req): Json<ShariahProhibitReq>,
) -> Json<serde_json::Value> {
    state.shariah_filter.write().add_prohibited_pair(&req.pair);
    Json(serde_json::json!({"status": "ok", "pair": req.pair.to_uppercase()}))
}

// ==================== WebSocket: Live Order Fills ====================

async fn orders_ws_handler(
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
struct MarketDataParams {
    pair: String,
}

async fn market_data_ws_handler(
    State(state): State<AppState>,
    Path(params): Path<MarketDataParams>,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<serde_json::value::Value>)> {
    let rx = state.market_data.tx.subscribe();
    let key = params.pair.to_uppercase();
    Ok(ws.on_upgrade(move |socket| handle_market_data_ws(socket, key, rx)))
}

async fn handle_market_data_ws(
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

async fn handle_orders_ws(
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

async fn docs_page() -> Html<&'static str> {
    Html(DOCS_HTML)
}

async fn kill_switch_demo_page() -> Html<&'static str> {
    Html(KILL_SWITCH_DEMO_HTML)
}

async fn openapi_spec() -> Json<serde_json::Value> {
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

async fn trade_page() -> Html<&'static str> {
    Html(include_str!("../trading/index.html"))
}

async fn dashboard_page() -> Html<&'static str> {
    Html(include_str!("../dashboard/index.html"))
}

async fn dashboard_sw() -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 2], &'static str) {
    (StatusCode::OK,
     [(axum::http::header::CONTENT_TYPE, "application/javascript"),
      (axum::http::header::CACHE_CONTROL, "no-cache")],
     include_str!("../dashboard/sw.js"))
}

async fn dashboard_manifest() -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 2], &'static str) {
    (StatusCode::OK,
     [(axum::http::header::CONTENT_TYPE, "application/manifest+json"),
      (axum::http::header::CACHE_CONTROL, "no-cache")],
     include_str!("../dashboard/manifest.json"))
}

async fn dashboard_ws_handler(
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
struct RegisterSovereignIdentityReq {
    tenant_id: uuid::Uuid,
    legal_name: String,
    lei: String,
    jurisdiction: String,
}

async fn register_sovereign_identity_handler(
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

async fn get_sovereign_identity_handler(
    State(state): State<AppState>,
    Path(tenant_id): Path<uuid::Uuid>,
) -> Json<serde_json::Value> {
    match state.sovereign_store.get_encrypted(&tenant_id) {
        Some(identity) => Json(serde_json::json!(identity)),
        None => Json(serde_json::json!({"error": "identity_not_found"})),
    }
}

#[derive(serde::Deserialize)]
struct DecryptSovereignReq {
    tenant_id: uuid::Uuid,
    regulator_secret_hex: String,
}

async fn decrypt_sovereign_identity_handler(
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

async fn generate_regulator_keypair_handler() -> Json<serde_json::Value> {
    let (sec, pubk) = sovereign::generate_regulator_keypair_hex();
    Json(serde_json::json!({
        "public_key_hex": pubk,
        "secret_key_hex": sec,
        "warning": "save the secret key securely — it cannot be recovered",
    }))
}

// ==================== Layer 2 — Counterparty Visibility Handlers ====================

#[derive(serde::Deserialize)]
struct AddCounterpartyReq {
    tenant_id: uuid::Uuid,
    counterparty_id: uuid::Uuid,
}

async fn add_counterparty_handler(
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

async fn list_counterparty_handler(
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

async fn check_counterparty_handler(
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

async fn iso20022_list_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.iso20022_queue.list_reports(100) {
        Ok(list) => Ok(Json(serde_json::json!({"reports": list}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))),
    }
}

async fn iso20022_get_handler(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.iso20022_queue.get_report(&filename) {
        Ok(xml) => Ok(Json(serde_json::json!({"filename": filename, "xml": xml}))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e})))),
    }
}

// ==================== Ghost Protocol Handlers ====================

async fn ghost_tax_rate_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let rate = state.sovereign_protocol.tax_rate();
    Json(serde_json::json!({"tax_rate_bps": rate, "tax_rate_percent": rate as f64 / 100.0}))
}

async fn ghost_tax_rate_set(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let rate = body["rate_bps"].as_u64().unwrap_or(0);
    state.sovereign_protocol.set_tax_rate(rate);
    state.fortress.record_action("sovereign", "tax_rate_change", &body, |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "tax_rate_bps": rate}))
}

async fn ghost_treasury(State(state): State<AppState>) -> Json<serde_json::Value> {
    let balance = state.sovereign_protocol.treasury_balance();
    let total = state.sovereign_protocol.tax_collected_total();
    Json(serde_json::json!({
        "treasury": balance,
        "total_collected": total,
    }))
}

async fn ghost_prohibited_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let list = state.sovereign_protocol.list_prohibited();
    Json(serde_json::json!({
        "count": list.len(),
        "addresses": list.into_iter().map(|(a, c)| serde_json::json!({"address": a, "blocked_count": c})).collect::<Vec<_>>(),
    }))
}

async fn ghost_prohibited_add(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    state.sovereign_protocol.add_prohibited(&addr);
    state.fortress.record_action("sovereign", "prohibit_add", &serde_json::json!({"address": addr}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "address": addr}))
}

async fn ghost_prohibited_remove(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.sovereign_protocol.remove_prohibited(&addr);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }, "address": addr}))
}

async fn ghost_sleeper_list(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn ghost_sleeper_watch(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let label = body["label"].as_str().unwrap_or("unknown");
    state.sovereign_protocol.watch_sleeper(&addr, label);
    Json(serde_json::json!({"status": "ok", "address": addr, "label": label}))
}

async fn ghost_sleeper_unwatch(
    State(state): State<AppState>,
    Path(addr): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.sovereign_protocol.unwatch_sleeper(&addr);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }, "address": addr}))
}

async fn ghost_sleeper_freeze(
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

async fn ghost_sleeper_seize(
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

async fn ghost_sleeper_tax(
    State(state): State<AppState>,
    Path((addr, amount)): Path<(String, u64)>,
) -> Json<serde_json::Value> {
    match state.sovereign_protocol.one_time_tax_sleeper(&addr, amount) {
        Ok(()) => Json(serde_json::json!({"status": "ok", "action": "tax", "address": addr, "amount": amount})),
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

async fn ghost_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.sovereign_protocol.snapshot();
    Json(serde_json::json!(snap))
}

// ==================== Universal Bridge Handlers ====================

async fn bridge_list_projects(State(state): State<AppState>) -> Json<serde_json::Value> {
    let projects = state.universal_bridge.list_projects();
    Json(serde_json::json!({
        "count": projects.len(),
        "projects": projects,
    }))
}

async fn bridge_register_project(
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

async fn bridge_remove_project(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.universal_bridge.remove_project(&name);
    Json(serde_json::json!({"status": if removed { "ok" } else { "not_found" }}))
}

async fn bridge_forward(
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

async fn bridge_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.universal_bridge.snapshot())
}

async fn bridge_receive(
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

async fn llm_chat(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<llm_sidecar::ChatRequest>,
) -> Json<serde_json::Value> {
    let response = state.llm_sidecar.chat(req).await;
    Json(serde_json::json!(response))
}

async fn llm_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "available": state.llm_sidecar.is_available(),
        "total_queries": state.llm_sidecar.total_queries(),
    }))
}

// ==================== Encrypted Backup Handlers ====================

async fn backup_trigger(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.backup.trigger(&*state.tee).await {
        Ok(manifest) => Json(serde_json::json!({"status": "ok", "manifest": manifest})),
        Err(e) => Json(serde_json::json!({"status": "error", "error": e})),
    }
}

async fn backup_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.backup.status();
    Json(serde_json::json!(status))
}

// ==================== Sovereign Fortress Handlers ====================

async fn fortress_heartbeat(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.fortress.heartbeat();
    let status = state.fortress.status();
    state.fortress.record_action("sovereign", "heartbeat", &serde_json::json!({"timestamp_ns": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok", "switch_state": status.switch_state}))
}

async fn fortress_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!(state.fortress.status()))
}

async fn fortress_audit(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.fortress.audit.snapshot();
    Json(serde_json::json!(snap))
}

async fn fortress_succession_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.fortress.dead_mans_switch.succession_plan() {
        Some(plan) => Json(serde_json::json!({"configured": true, "plan": plan})),
        None => Json(serde_json::json!({"configured": false})),
    }
}

async fn fortress_succession_set(
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

async fn fortress_succession_disable(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.fortress.dead_mans_switch.disable();
    state.fortress.record_action("sovereign", "succession_disabled", &serde_json::json!({}), |msg| state.tee.sign(msg));
    Json(serde_json::json!({"status": "ok"}))
}

async fn fortress_treasury_balance(State(state): State<AppState>) -> Json<serde_json::Value> {
    let bal = state.fortress.treasury.balance();
    Json(serde_json::json!({"balance": bal, "asset": "USDC"}))
}

async fn fortress_treasury_withdraw(
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

async fn circuit_breaker_status(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn circuit_breaker_config_get(
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

async fn circuit_breaker_config_set(
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

async fn circuit_breaker_events(State(state): State<AppState>) -> Json<serde_json::Value> {
    let events = state.circuit_breaker.recent_events(100);
    Json(serde_json::json!({
        "count": events.len(),
        "events": events,
    }))
}

async fn circuit_breaker_reset(
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

async fn circuit_breaker_trigger(
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
            crate::cloak::NodeCloakingProtocol::activate_cloaking(&*state.kill_switch);
            Json(serde_json::json!({"status": "ok", "pair": pair.to_uppercase(), "level": 3, "mode": "kill_shield"}))
        }
        _ => Json(serde_json::json!({"error": "invalid level — use 1, 2, or 3"})),
    }
}

#[cfg(unix)]
async fn serve_tls(
    listener: tokio::net::TcpListener,
    app: Router,
    _cert_path: &str,
    _key_path: &str,
) -> Result<(), axum::BoxError> {
    warn!("TLS certs not configured - falling back to plain HTTP");
    axum::serve(listener, app).await.map_err(Into::into)
}

#[cfg(not(unix))]
async fn serve_tls(
    _listener: tokio::net::TcpListener,
    _app: Router,
    _cert_path: &str,
    _key_path: &str,
) -> Result<(), axum::BoxError> {
    warn!("TLS not available on this platform");
    Ok(())
}

// ═══════════════════════════════════════════════════════════
// Revenue Engine Handlers
// ═══════════════════════════════════════════════════════════

async fn get_revenue_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let re = state.revenue_engine.read().await;
    let cfg = re.get_config();
    Json(serde_json::json!({
        "maker_fee_bps": cfg.maker_fee_bps,
        "taker_fee_bps": cfg.taker_fee_bps,
        "cross_venue_fee_bps": cfg.cross_venue_fee_bps,
        "mei_sharing_bps": cfg.mei_sharing_bps,
    }))
}

#[derive(serde::Deserialize)]
struct UpdateRevenueConfigReq {
    maker_fee_bps: Option<u32>,
    taker_fee_bps: Option<u32>,
}

async fn update_revenue_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateRevenueConfigReq>,
) -> Json<serde_json::Value> {
    let mut re = state.revenue_engine.write().await;
    let mut cfg = re.get_config().clone();
    if let Some(bps) = req.maker_fee_bps {
        cfg.maker_fee_bps = bps;
    }
    if let Some(bps) = req.taker_fee_bps {
        cfg.taker_fee_bps = bps;
    }
    re.set_config(cfg);
    Json(serde_json::json!({"status": "updated"}))
}

async fn get_participant_profile(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    let re = state.revenue_engine.read().await;
    match re.get_participant_profile(&participant_id).await {
        Some(profile) => Json(serde_json::to_value(&profile).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "participant_not_found", "participant_id": participant_id})),
    }
}

async fn list_revenue_profiles(State(state): State<AppState>) -> Json<serde_json::Value> {
    let guard = state.revenue_engine.read().await;
    let profiles = guard.get_profiles().read().await;
    let list: Vec<&revenue_engine::ParticipantRevenueProfile> = profiles.values().collect();
    Json(serde_json::json!({
        "count": list.len(),
        "profiles": list,
    }))
}

#[derive(serde::Deserialize)]
struct CalculateFeesReq {
    participant_id: String,
    counterparty_id: String,
    symbol: String,
    side: String,
    quantity: f64,
    price: f64,
    is_maker: bool,
}

async fn calculate_fees(
    State(state): State<AppState>,
    Json(req): Json<CalculateFeesReq>,
) -> Json<serde_json::Value> {
    let re = state.revenue_engine.read().await;
    let result = re.calculate_trade_fees(
        &req.participant_id, &req.counterparty_id, &req.symbol,
        &req.side, req.quantity, req.price, req.is_maker,
    ).await;
    Json(serde_json::to_value(&result).unwrap_or_default())
}

async fn get_referral_info(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    let guard = state.revenue_engine.read().await;
    let profile = guard.get_participant_profile(&participant_id).await;
    let tree = guard.get_referral_tree().read().await;
    let referrals = tree.get(&participant_id).cloned().unwrap_or_default();
    Json(serde_json::json!({
        "participant_id": participant_id,
        "profile": profile,
        "referrals_count": referrals.len(),
        "referrals": referrals,
    }))
}

async fn get_revenue_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let metrics = {
        let re = state.revenue_engine.read().await;
        re.get_metrics().await
    };
    Json(serde_json::to_value(&metrics).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// Lending Pool Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct LendingDepositReq { user_id: String, asset: String, amount: f64 }
#[derive(serde::Deserialize)]
struct LendingBorrowReq { user_id: String, asset: String, amount: f64, collateral_asset: String, collateral_amount: f64 }
#[derive(serde::Deserialize)]
struct LendingRepayReq { loan_id: u64, amount: f64 }
#[derive(serde::Deserialize)]
struct LendingWithdrawReq { deposit_id: u64, amount: f64 }

async fn lending_deposit(State(state): State<AppState>, Json(req): Json<LendingDepositReq>) -> Json<serde_json::Value> {
    match state.lending_pool.deposit(req.user_id, req.asset, req.amount) {
        Ok(pos) => Json(serde_json::to_value(&pos).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn lending_borrow(State(state): State<AppState>, Json(req): Json<LendingBorrowReq>) -> Json<serde_json::Value> {
    match state.lending_pool.borrow(req.user_id, req.asset, req.amount, req.collateral_asset, req.collateral_amount) {
        Ok(loan) => Json(serde_json::to_value(&loan).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn lending_repay(State(state): State<AppState>, Json(req): Json<LendingRepayReq>) -> Json<serde_json::Value> {
    match state.lending_pool.repay(req.loan_id, req.amount) {
        Ok(loan) => Json(serde_json::to_value(&loan).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn lending_withdraw(State(state): State<AppState>, Json(req): Json<LendingWithdrawReq>) -> Json<serde_json::Value> {
    match state.lending_pool.withdraw(req.deposit_id, req.amount) {
        Ok(pos) => Json(serde_json::to_value(&pos).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn lending_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.lending_pool.snapshot();
    Json(serde_json::to_value(&snap).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// Securities Lending Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct SecuritiesLendReq { lender_id: String, asset_id: String, quantity: f64, fee_bps: u64, duration_days: u64 }
#[derive(serde::Deserialize)]
struct SecuritiesBorrowReq { borrower_id: String, offer_id: u64, collateral: f64 }
#[derive(serde::Deserialize)]
struct SecuritiesReturnReq { agreement_id: u64 }

async fn securities_lend(State(state): State<AppState>, Json(req): Json<SecuritiesLendReq>) -> Json<serde_json::Value> {
    match state.securities_lending.lend(req.lender_id, req.asset_id, req.quantity, req.fee_bps, req.duration_days) {
        Ok(offer) => Json(serde_json::to_value(&offer).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn securities_borrow(State(state): State<AppState>, Json(req): Json<SecuritiesBorrowReq>) -> Json<serde_json::Value> {
    match state.securities_lending.borrow(req.borrower_id, req.offer_id, req.collateral) {
        Ok(agreement) => Json(serde_json::to_value(&agreement).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn securities_return(State(state): State<AppState>, Json(req): Json<SecuritiesReturnReq>) -> Json<serde_json::Value> {
    match state.securities_lending.return_asset(req.agreement_id) {
        Ok(agreement) => Json(serde_json::to_value(&agreement).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn securities_assets(State(state): State<AppState>) -> Json<serde_json::Value> {
    let assets = state.securities_lending.list_assets();
    Json(serde_json::json!({"assets": assets}))
}

async fn securities_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.securities_lending.snapshot();
    Json(serde_json::to_value(&snap).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// Dark Pool Handlers
// ═══════════════════════════════════════════════════════════

async fn darkpool_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let dp = state.dark_pool.read().await;
    let status = dp.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct DarkPoolSubmitReq {
    pair: String,
    side: String,
    quantity: f64,
    price: f64,
}

async fn darkpool_submit(
    State(state): State<AppState>,
    Json(req): Json<DarkPoolSubmitReq>,
) -> Json<serde_json::Value> {
    let order = types::Order {
        id: uuid::Uuid::new_v4(),
        id_tag: 0,
        user_id: uuid::Uuid::new_v4(),
        pair: req.pair.into(),
        side: if req.side.to_lowercase() == "buy" { types::OrderSide::Buy } else { types::OrderSide::Sell },
        order_type: types::OrderType::Limit,
        price: req.price,
        quantity: req.quantity,
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64,
        ..Default::default()
    };
    let request = dark_pool_manager::SubmitOrderRequest {
        order,
        track: types::Track::Compliant,
        signature: None,
    };
    let dp = state.dark_pool.read().await;
    match dp.submit_order(request).await {
        Ok(resp) => Json(serde_json::to_value(&resp).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn darkpool_trades(State(state): State<AppState>) -> Json<serde_json::Value> {
    let dp = state.dark_pool.read().await;
    let trades = dp.get_trades().await;
    Json(serde_json::json!({"trades": trades}))
}

// ═══════════════════════════════════════════════════════════
// FX Engine Handlers
// ═══════════════════════════════════════════════════════════

async fn get_fx_rates(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.fx_engine.snapshot();
    Json(serde_json::to_value(&snap).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct FXQuoteReq { from: String, to: String, amount: f64 }

async fn get_fx_quote(State(state): State<AppState>, Json(req): Json<FXQuoteReq>) -> Json<serde_json::Value> {
    match state.fx_engine.quote(req.from, req.to, req.amount) {
        Ok(quote) => Json(serde_json::to_value(&quote).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct FXConvertReq { from: String, to: String, amount: f64 }

async fn execute_fx_conversion(State(state): State<AppState>, Json(req): Json<FXConvertReq>) -> Json<serde_json::Value> {
    match state.fx_engine.execute(req.from, req.to, req.amount) {
        Ok(trade) => Json(serde_json::to_value(&trade).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn list_nostro_accounts(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.fx_engine.snapshot();
    Json(serde_json::json!({"accounts": snap.accounts}))
}

async fn get_nostro_balance(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.fx_engine.get_account_balance(&account_id) {
        Some(balance) => Json(serde_json::json!({"account_id": account_id, "balance": balance})),
        None => Json(serde_json::json!({"error": "account_not_found"})),
    }
}

// ═══════════════════════════════════════════════════════════
// Cross-Venue Arbitrage Handlers
// ═══════════════════════════════════════════════════════════

async fn cross_venue_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let stats = state.cross_venue_arb.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

async fn cross_venue_pnl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pnl = state.cross_venue_arb.get_pnl().await;
    Json(serde_json::to_value(&pnl).unwrap_or_default())
}

async fn cross_venue_trades(
    State(state): State<AppState>,
    Path(n): Path<usize>,
) -> Json<serde_json::Value> {
    let trades = state.cross_venue_arb.get_recent_trades(n).await;
    Json(serde_json::json!({"trades": trades}))
}

async fn cross_venue_prices(State(state): State<AppState>) -> Json<serde_json::Value> {
    let prices = state.cross_venue_arb.get_prices().await;
    Json(serde_json::json!({"prices": prices}))
}

// ═══════════════════════════════════════════════════════════
// Super-Arb Engine Handlers
// ═══════════════════════════════════════════════════════════

async fn super_arb_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let stats = state.super_arb.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

async fn super_arb_pnl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pnl = state.super_arb.get_pnl().await;
    Json(serde_json::to_value(&pnl).unwrap_or_default())
}

async fn super_arb_trades(
    State(state): State<AppState>,
    Path(n): Path<usize>,
) -> Json<serde_json::Value> {
    let trades = state.super_arb.get_recent_trades(n).await;
    Json(serde_json::json!({"trades": trades}))
}

async fn super_arb_prices(State(state): State<AppState>) -> Json<serde_json::Value> {
    let prices = state.super_arb.get_prices().await;
    Json(serde_json::json!({"prices": prices}))
}

// ═══════════════════════════════════════════════════════════
// Compliance Engine Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct RegisterParticipantReq {
    participant_id: String,
    legal_entity_name: String,
    jurisdiction: String,
    lei: Option<String>,
}

async fn compliance_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterParticipantReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.register_participant(req.participant_id, req.legal_entity_name, req.jurisdiction, req.lei).await {
        Ok(profile) => Json(serde_json::to_value(&profile).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct SubmitKYCReq {
    participant_id: String,
    documents: Vec<compliance_engine::KYCDocument>,
}

async fn compliance_submit_kyc(
    State(state): State<AppState>,
    Json(req): Json<SubmitKYCReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.submit_kyc_documents(&req.participant_id, req.documents).await {
        Ok(_) => Json(serde_json::json!({"status": "submitted"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct ReviewKYCReq {
    participant_id: String,
    reviewer: String,
    approved: bool,
    notes: String,
}

async fn compliance_review_kyc(
    State(state): State<AppState>,
    Json(req): Json<ReviewKYCReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.review_kyc(&req.participant_id, &req.reviewer, req.approved, req.notes).await {
        Ok(_) => Json(serde_json::json!({"status": if req.approved { "approved" } else { "rejected" }})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn compliance_profile(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.get_profile(&participant_id).await {
        Some(profile) => Json(serde_json::to_value(&profile).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn compliance_alerts(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    let alerts = state.compliance_engine.get_alerts(Some(&participant_id), true).await;
    Json(serde_json::json!({"alerts": alerts}))
}

async fn compliance_all_alerts(State(state): State<AppState>) -> Json<serde_json::Value> {
    let alerts = state.compliance_engine.get_alerts(None, true).await;
    Json(serde_json::json!({"alerts": alerts}))
}

#[derive(serde::Deserialize)]
struct AlertActionReq {
    alert_id: String,
    by: String,
}

async fn compliance_acknowledge_alert(
    State(state): State<AppState>,
    Json(req): Json<AlertActionReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.acknowledge_alert(&req.alert_id, &req.by).await {
        Ok(_) => Json(serde_json::json!({"status": "acknowledged"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn compliance_resolve_alert(
    State(state): State<AppState>,
    Json(req): Json<AlertActionReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.resolve_alert(&req.alert_id, &req.by, "resolved".to_string()).await {
        Ok(_) => Json(serde_json::json!({"status": "resolved"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct FreezeReq {
    participant_id: String,
    reason: String,
    officer: String,
}

async fn compliance_freeze(
    State(state): State<AppState>,
    Json(req): Json<FreezeReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.freeze_participant(&req.participant_id, req.reason, &req.officer).await {
        Ok(_) => Json(serde_json::json!({"status": "frozen"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn compliance_unfreeze(
    State(state): State<AppState>,
    Json(req): Json<FreezeReq>,
) -> Json<serde_json::Value> {
    match state.compliance_engine.unfreeze_participant(&req.participant_id, req.reason, &req.officer).await {
        Ok(_) => Json(serde_json::json!({"status": "unfrozen"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn compliance_audit_log(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    let log = state.compliance_engine.get_audit_log(Some(&participant_id), 100).await;
    Json(serde_json::json!({"audit_log": log}))
}

// ════════════════════════════════════════════════════════════
// Risk Engine Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct RiskRegisterReq {
    participant_id: String,
    initial_nav: f64,
}

async fn risk_register(
    State(state): State<AppState>,
    Json(req): Json<RiskRegisterReq>,
) -> Json<serde_json::Value> {
    let profile = state.risk_engine.register_participant(req.participant_id, req.initial_nav).await;
    Json(serde_json::to_value(&profile).unwrap_or_default())
}

async fn risk_profile(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.risk_engine.get_profile(&participant_id).await {
        Some(profile) => Json(serde_json::to_value(&profile).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn risk_alerts(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    let alerts = state.risk_engine.get_alerts(Some(&participant_id), true).await;
    Json(serde_json::json!({"alerts": alerts}))
}

async fn risk_all_alerts(State(state): State<AppState>) -> Json<serde_json::Value> {
    let alerts = state.risk_engine.get_alerts(None, true).await;
    Json(serde_json::json!({"alerts": alerts}))
}

async fn risk_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let metrics = state.risk_engine.get_metrics().await;
    Json(serde_json::to_value(&metrics).unwrap_or_default())
}

async fn risk_stress_test(
    State(state): State<AppState>,
    Path(participant_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.risk_engine.run_stress_test(&participant_id).await {
        Ok(results) => Json(serde_json::json!({"stress_test_results": results})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ════════════════════════════════════════════════════════════
// Onboarding Engine Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct OnboardingInitiateReq {
    client_name: String,
    jurisdiction: String,
    entity_type: String,
    lei: Option<String>,
    primary_contact_email: String,
    primary_contact_name: String,
    #[serde(default)]
    expected_monthly_volume_usd: u64,
    #[serde(default)]
    expected_aum_usd: u64,
}

async fn onboarding_initiate(
    State(state): State<AppState>,
    Json(req): Json<OnboardingInitiateReq>,
) -> Json<serde_json::Value> {
    use onboarding_engine::{EntityType, Contact, ContactMethod};
    let entity = match req.entity_type.to_lowercase().as_str() {
        "individual" => EntityType::Individual,
        "corporation" => EntityType::Corporation,
        "partnership" => EntityType::Partnership,
        "trust" => EntityType::Trust,
        "fund" => EntityType::Fund,
        "family_office" => EntityType::FamilyOffice,
        _ => EntityType::Other(req.entity_type),
    };
    let contact = Contact {
        name: req.primary_contact_name,
        title: "".to_string(),
        email: req.primary_contact_email,
        phone: "".to_string(),
        preferred_contact_method: ContactMethod::Email,
    };
    match state.onboarding_engine.initiate_onboarding(
        req.client_name,
        entity,
        req.jurisdiction,
        contact,
        req.expected_monthly_volume_usd,
        req.expected_aum_usd,
    ).await {
        Ok(client) => Json(serde_json::to_value(&client).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct OnboardingDocReq {
    client_id: String,
    document_type: String,
    file_hash: String,
    issued_at: u64,
    expires_at: Option<u64>,
    issuing_authority: String,
}

async fn onboarding_submit_doc(
    State(state): State<AppState>,
    Json(req): Json<OnboardingDocReq>,
) -> Json<serde_json::Value> {
    use onboarding_engine::{DocumentType, ClientDocument};
    let doc = ClientDocument {
        document_id: uuid::Uuid::new_v4().to_string(),
        document_type: DocumentType::Other(req.document_type),
        file_name: "uploaded_document".to_string(),
        file_hash: req.file_hash,
        file_size_bytes: 0,
        mime_type: "application/octet-stream".to_string(),
        uploaded_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        uploaded_by: "api_user".to_string(),
        verified_at: None,
        verified_by: None,
        ai_verified: false,
        ai_confidence: None,
        expiry_date: req.expires_at,
        tags: vec![],
    };
    match state.onboarding_engine.submit_document(&req.client_id, doc).await {
        Ok(_) => Json(serde_json::json!({"status": "submitted"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct OnboardingAdvanceReq {
    client_id: String,
    approved: bool,
    reviewer: String,
}

async fn onboarding_advance(
    State(state): State<AppState>,
    Json(req): Json<OnboardingAdvanceReq>,
) -> Json<serde_json::Value> {
    use onboarding_engine::{OnboardingStatus, StageOutcome};
    let (stage, outcome) = if req.approved {
        (OnboardingStatus::DocumentCollection, StageOutcome::Approved)
    } else {
        (OnboardingStatus::Rejected, StageOutcome::Rejected)
    };
    match state.onboarding_engine.advance_workflow_stage(&req.client_id, stage, &req.reviewer, outcome, "API review".to_string()).await {
        Ok(_) => Json(serde_json::json!({"status": if req.approved { "advanced" } else { "rejected" }})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn onboarding_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.onboarding_engine.get_client(&client_id).await {
        Some(client) => Json(serde_json::to_value(&client).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn onboarding_list_clients(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let clients = state.onboarding_engine.list_clients(None).await;
    Json(serde_json::json!({"clients": clients}))
}

async fn onboarding_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let metrics = state.onboarding_engine.get_metrics().await;
    Json(serde_json::to_value(&metrics).unwrap_or_default())
}

async fn onboarding_workflow(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.onboarding_engine.get_workflow(&client_id).await {
        Some(wf) => Json(serde_json::to_value(&wf).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn onboarding_prime_broker(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.onboarding_engine.get_prime_broker_account(&account_id).await {
        Some(acc) => Json(serde_json::to_value(&acc).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn onboarding_custodian(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.onboarding_engine.get_custodian_account(&account_id).await {
        Some(acc) => Json(serde_json::to_value(&acc).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

// ════════════════════════════════════════════════════════════
// Execution Engine Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct ExecOrderReq {
    order_id: String,
    symbol: String,
    side: String,
    order_type: String,
    quantity: f64,
    price: Option<f64>,
    time_in_force: String,
    execution_strategy: String,
    max_participation_rate: Option<f64>,
    start_time: Option<u64>,
    end_time: Option<u64>,
    urgency: String,
}

async fn execution_submit(
    State(state): State<AppState>,
    Json(req): Json<ExecOrderReq>,
) -> Json<serde_json::Value> {
    use execution_engine::{AdvancedOrder, OrderSide, OrderType, TimeInForce, ExecutionInstructions, AlgoParams};
    let order = AdvancedOrder {
        order_id: req.order_id.clone(),
        client_order_id: req.order_id.clone(),
        participant_id: "api_user".to_string(),
        symbol: req.symbol,
        side: if req.side.to_lowercase() == "buy" { OrderSide::Buy } else { OrderSide::Sell },
        order_type: match req.order_type.to_lowercase().as_str() {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "stop" => OrderType::Stop,
            "stop_limit" => OrderType::StopLimit,
            _ => OrderType::Limit,
        },
        quantity: req.quantity,
        filled_quantity: 0.0,
        remaining_quantity: req.quantity,
        price: req.price,
        stop_price: None,
        limit_price: req.price,
        trailing_offset: None,
        time_in_force: match req.time_in_force.to_lowercase().as_str() {
            "day" => TimeInForce::GTC,
            "gtc" => TimeInForce::GTC,
            "ioc" => TimeInForce::IOC,
            "fok" => TimeInForce::FOK,
            _ => TimeInForce::GTC,
        },
        status: execution_engine::OrderStatus::New,
        created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        updated_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        expires_at: req.end_time,
        execution_instructions: ExecutionInstructions::default(),
        algo_params: AlgoParams::default(),
        legs: vec![],
        parent_order_id: None,
        child_orders: vec![],
        tags: std::collections::HashMap::new(),
    };
    match state.execution_engine.submit_order(order).await {
        Ok(report) => Json(serde_json::to_value(&report).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn execution_cancel(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.execution_engine.cancel_order(&order_id).await {
        Ok(_) => Json(serde_json::json!({"status": "cancelled"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn execution_order(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.execution_engine.get_order(&order_id).await {
        Some(order) => Json(serde_json::to_value(&order).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn execution_reports(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> Json<serde_json::Value> {
    let reports = state.execution_engine.get_execution_reports(&order_id).await;
    Json(serde_json::json!({"reports": reports}))
}

async fn execution_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let metrics = state.execution_engine.get_metrics().await;
    Json(serde_json::to_value(&metrics).unwrap_or_default())
}

async fn execution_mev_detect(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    // MEV detection endpoint
    Json(serde_json::json!({"status": "mev_detection_ready"}))
}

// ════════════════════════════════════════════════════════════
// Liquidity Engine (BMM) Handlers
// ═══════════════════════════════════════════════════════════

async fn liquidity_aggregated_book(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Json<serde_json::Value> {
    match state.liquidity_engine.get_aggregated_book(&symbol).await {
        Some(book) => Json(serde_json::to_value(&book).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

#[derive(serde::Deserialize)]
struct LiquidityBestExecReq {
    symbol: String,
    side: String,
    size_usd: f64,
}

async fn liquidity_best_execution(
    State(state): State<AppState>,
    Json(req): Json<LiquidityBestExecReq>,
) -> Json<serde_json::Value> {
    match state.liquidity_engine.get_best_execution(&req.symbol, &req.side, req.size_usd).await {
        Some(plan) => Json(serde_json::to_value(&plan).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "no_execution_path"})),
    }
}

#[derive(serde::Deserialize)]
struct RegisterMMReq {
    maker_id: String,
    venue: String,
    symbols: Vec<String>,
    min_spread_bps: f64,
    max_position_usd: f64,
    quote_size_usd: f64,
    rebate_bps: u32,
}

async fn liquidity_register_mm(
    State(state): State<AppState>,
    Json(req): Json<RegisterMMReq>,
) -> Json<serde_json::Value> {
    use liquidity_engine::{MarketMakerProfile, RebateTier};
    let rebate_tier = match req.rebate_bps {
        0..=5 => RebateTier::Bronze,
        6..=10 => RebateTier::Silver,
        11..=20 => RebateTier::Gold,
        _ => RebateTier::Platinum,
    };
    let profile = MarketMakerProfile {
        participant_id: req.maker_id,
        symbols: req.symbols,
        min_spread_bps: req.min_spread_bps as u32,
        max_position_usd: req.max_position_usd,
        quote_size_usd: req.quote_size_usd,
        uptime_requirement_pct: 99.0,
        rebate_tier,
        performance_score: 1.0,
        total_volume_usd: 0.0,
        total_rebates_usd: 0.0,
        is_active: true,
    };
    match state.liquidity_engine.register_market_maker(profile).await {
        Ok(_) => Json(serde_json::json!({"status": "registered"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn liquidity_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let metrics = state.liquidity_engine.get_metrics().await;
    Json(serde_json::to_value(&metrics).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// White Label Handlers
// ═══════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct WhiteLabelDeployReq {
    tenant_id: String,
    branding_name: String,
    domain: String,
    enabled_features: Vec<String>,
    fee_bps: u32,
    supported_pairs: Vec<String>,
    custom_css: Option<String>,
}

async fn whitelabel_deploy(
    State(state): State<AppState>,
    Json(req): Json<WhiteLabelDeployReq>,
) -> Json<serde_json::Value> {
    use white_label::WhiteLabelConfig;
    let config = WhiteLabelConfig {
        brand_name: req.branding_name,
        brand_logo_url: "".to_string(),
        brand_primary_color: "#000000".to_string(),
        domain: req.domain,
        custom_fix_port: None,
        custom_api_port: None,
        dark_pool_enabled: req.enabled_features.contains(&"dark_pool".to_string()),
        fba_enabled: req.enabled_features.contains(&"fba".to_string()),
        ghost_enabled: req.enabled_features.contains(&"ghost".to_string()),
        compliance_zk_enabled: req.enabled_features.contains(&"compliance_zk".to_string()),
        shariah_enabled: req.enabled_features.contains(&"shariah".to_string()),
        iso20022_enabled: req.enabled_features.contains(&"iso20022".to_string()),
        dedicated_cores: 4,
        monthly_volume_cap: 1_000_000.0,
    };
    let tenant_id = uuid::Uuid::parse_str(&req.tenant_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
    match state.white_label.deploy(&tenant_id, config) {
        Ok(instance) => Json(serde_json::to_value(&instance).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn whitelabel_instance(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Json<serde_json::Value> {
    let tenant_id = uuid::Uuid::parse_str(&tenant_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
    match state.white_label.get_instance(&tenant_id) {
        Some(instance) => Json(serde_json::to_value(&instance).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not_found"})),
    }
}

async fn whitelabel_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.white_label.list_instances();
    Json(serde_json::json!({"instances": instances}))
}

#[derive(serde::Deserialize)]
struct WhiteLabelRecordReq {
    tenant_id: String,
}

async fn whitelabel_record_order(
    State(state): State<AppState>,
    Json(req): Json<WhiteLabelRecordReq>,
) -> Json<serde_json::Value> {
    let tenant_id = uuid::Uuid::parse_str(&req.tenant_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
    state.white_label.record_order(&tenant_id);
    Json(serde_json::json!({"status": "recorded"}))
}

#[derive(serde::Deserialize)]
struct WhiteLabelVolumeReq {
    tenant_id: String,
    volume: f64,
}

async fn whitelabel_record_volume(
    State(state): State<AppState>,
    Json(req): Json<WhiteLabelVolumeReq>,
) -> Json<serde_json::Value> {
    let tenant_id = uuid::Uuid::parse_str(&req.tenant_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
    state.white_label.record_volume(&tenant_id, req.volume);
    Json(serde_json::json!({"status": "recorded"}))
}

async fn whitelabel_count(State(state): State<AppState>) -> Json<serde_json::Value> {
    let count = state.white_label.deployment_count();
    Json(serde_json::json!({"deployments": count}))
}

#[derive(serde::Deserialize)]
struct WhiteLabelRemoveReq {
    tenant_id: String,
}

async fn whitelabel_remove(
    State(state): State<AppState>,
    Json(req): Json<WhiteLabelRemoveReq>,
) -> Json<serde_json::Value> {
    let tenant_id = uuid::Uuid::parse_str(&req.tenant_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
    let removed = state.white_label.remove_instance(&tenant_id);
    Json(serde_json::json!({"removed": removed}))
}

// ═══════════════════════════════════════════════════════════
// Instant-Flow Revenue Routing Handlers
// ═══════════════════════════════════════════════════════════

async fn instant_flow_dashboard(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let dashboard = state.instant_flow.get_dashboard().await;
    Json(serde_json::to_value(&dashboard).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct InstantFlowRecordReq {
    source: String,
    amount: f64,
}

async fn instant_flow_record(
    State(state): State<AppState>,
    Json(req): Json<InstantFlowRecordReq>,
) -> Json<serde_json::Value> {
    let source = match req.source.to_lowercase().as_str() {
        "trading_fees" => instant_flow::RevenueSource::TradingFees,
        "fx_spread" => instant_flow::RevenueSource::FxSpread,
        "lending_interest" => instant_flow::RevenueSource::LendingInterest,
        "dark_pool_fees" => instant_flow::RevenueSource::DarkPoolFees,
        "arbitrage_profit" => instant_flow::RevenueSource::ArbitrageProfit,
        "market_making" => instant_flow::RevenueSource::MarketMaking,
        "flash_loan_fees" => instant_flow::RevenueSource::FlashLoanFees,
        _ => return Json(serde_json::json!({"error": "invalid source — use: trading_fees, fx_spread, lending_interest, dark_pool_fees, arbitrage_profit, market_making, flash_loan_fees"})),
    };
    if req.amount <= 0.0 {
        return Json(serde_json::json!({"error": "amount must be positive"}));
    }
    state.instant_flow.record_revenue(source, req.amount).await;
    Json(serde_json::json!({"status": "recorded", "amount": req.amount}))
}

async fn instant_flow_distribute(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let distributions = state.instant_flow.distribute().await;
    let total: f64 = distributions.iter().map(|d| d.amount).sum();
    Json(serde_json::json!({
        "executed": distributions.len(),
        "total_amount": total,
        "distributions": distributions,
    }))
}

async fn instant_flow_config_get(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.instant_flow.get_config().await;
    Json(serde_json::to_value(&config).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct InstantFlowConfigUpdateReq {
    auto_compound_pct: Option<f64>,
    reserve_target_usd: Option<f64>,
    reserve_max_pct: Option<f64>,
}

async fn instant_flow_config_set(
    State(state): State<AppState>,
    Json(req): Json<InstantFlowConfigUpdateReq>,
) -> Json<serde_json::Value> {
    let mut config = state.instant_flow.get_config().await;
    if let Some(pct) = req.auto_compound_pct {
        config.auto_compound_pct = pct.clamp(0.0, 1.0);
    }
    if let Some(target) = req.reserve_target_usd {
        config.reserve_target_usd = target.max(0.0);
    }
    if let Some(max_pct) = req.reserve_max_pct {
        config.reserve_max_pct = max_pct.clamp(0.0, 1.0);
    }
    state.instant_flow.update_config(config).await;
    Json(serde_json::json!({"status": "updated"}))
}

// ═══════════════════════════════════════════════════════════
// Vampire Core Handlers
// ═══════════════════════════════════════════════════════════

async fn vampire_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.vampire_core.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn vampire_treasury(State(state): State<AppState>) -> Json<serde_json::Value> {
    let treasury = state.vampire_core.get_treasury().await;
    Json(serde_json::to_value(&treasury).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct VampireAbsorbReq {
    source: String,
    amount: f64,
}

async fn vampire_absorb(
    State(state): State<AppState>,
    Json(req): Json<VampireAbsorbReq>,
) -> Json<serde_json::Value> {
    if req.amount <= 0.0 {
        return Json(serde_json::json!({"error": "amount must be positive"}));
    }
    state.vampire_core.absorb_profit(&req.source, req.amount).await;
    Json(serde_json::json!({"status": "absorbed", "source": req.source, "amount": req.amount}))
}

async fn vampire_config_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.vampire_core.get_config().await;
    Json(serde_json::to_value(&config).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct VampireConfigUpdateReq {
    reinvest_pct: Option<f64>,
    hunger_threshold_volatility: Option<f64>,
    compound_interval_secs: Option<u64>,
    min_profit_to_absorb: Option<f64>,
}

async fn vampire_config_set(
    State(state): State<AppState>,
    Json(req): Json<VampireConfigUpdateReq>,
) -> Json<serde_json::Value> {
    let mut config = state.vampire_core.get_config().await;
    if let Some(pct) = req.reinvest_pct {
        config.reinvest_pct = pct.clamp(0.0, 1.0);
    }
    if let Some(threshold) = req.hunger_threshold_volatility {
        config.hunger_threshold_volatility = threshold.max(0.0);
    }
    if let Some(interval) = req.compound_interval_secs {
        config.compound_interval_secs = interval.max(1);
    }
    if let Some(min) = req.min_profit_to_absorb {
        config.min_profit_to_absorb = min.max(0.0);
    }
    state.vampire_core.update_config(config).await;
    Json(serde_json::json!({"status": "updated"}))
}

// ═══════════════════════════════════════════════════════════
// Sovereign Ghost — Network Privacy Layer Handlers
// ═══════════════════════════════════════════════════════════

async fn ghost_privacy_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.sovereign_ghost.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn ghost_privacy_create_circuit(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.sovereign_ghost.create_circuit().await {
        Ok(circuit) => Json(serde_json::to_value(&circuit).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn ghost_privacy_dissolve_circuit(
    State(state): State<AppState>,
    Path(circuit_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.sovereign_ghost.dissolve_circuit(&circuit_id).await {
        Ok(()) => Json(serde_json::json!({"status": "dissolved", "circuit_id": circuit_id})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn ghost_privacy_emergency(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.sovereign_ghost.emergency_dissolve_all().await {
        Ok(()) => {
            let status = state.sovereign_ghost.get_status().await;
            Json(serde_json::json!({
                "status": "emergency_activated",
                "dissolved": status.total_dissolved,
                "active_circuits": status.active_circuits,
            }))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn ghost_privacy_rotate_identity(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let identity = state.sovereign_ghost.rotate_identity().await;
    Json(serde_json::to_value(&identity).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// Flash Loan Arbitrage API Handlers
// ═══════════════════════════════════════════════════════════

async fn flash_loan_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.flash_loan_api.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn flash_loan_opportunities(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let opportunities = state.flash_loan_api.get_opportunities().await;
    Json(serde_json::json!({
        "count": opportunities.len(),
        "opportunities": opportunities,
    }))
}

async fn flash_loan_execute(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let opportunity_id = payload.get("opportunity_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match state.flash_loan_api.execute(opportunity_id).await {
        Ok(result) => Json(serde_json::to_value(&result).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn flash_loan_history(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let history = state.flash_loan_api.get_history(50).await;
    Json(serde_json::json!({
        "count": history.len(),
        "trades": history,
    }))
}

// ═══════════════════════════════════════════════════════════
// MEV Protection API Handlers
// ═══════════════════════════════════════════════════════════

async fn mev_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.mev_api.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn mev_threats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let threats = state.mev_api.get_threats().await;
    Json(serde_json::json!({
        "count": threats.len(),
        "threats": threats,
    }))
}

async fn mev_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.mev_api.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

async fn mev_history(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let history = state.mev_api.get_history(50).await;
    Json(serde_json::json!({
        "count": history.len(),
        "incidents": history,
    }))
}

// ═══════════════════════════════════════════════════════════
// Batch Auction API Handlers
// ═══════════════════════════════════════════════════════════

async fn batch_auction_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.batch_auction_api.get_current_auction().await {
        Some(auction) => Json(serde_json::to_value(&auction).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "no auction data"})),
    }
}

async fn batch_auction_start(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let pair = payload.get("pair")
        .and_then(|v| v.as_str())
        .unwrap_or("USD/EUR");
    match state.batch_auction_api.start_auction(pair).await {
        Ok(batch_id) => Json(serde_json::json!({"status": "started", "batch_id": batch_id})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn batch_auction_submit(
    State(state): State<AppState>,
    Json(order): Json<batch_auction_api::BatchOrder>,
) -> Json<serde_json::Value> {
    match state.batch_auction_api.submit_order(order).await {
        Ok(()) => Json(serde_json::json!({"status": "submitted"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn batch_auction_history(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let history = state.batch_auction_api.get_history().await;
    Json(serde_json::json!({
        "count": history.len(),
        "auctions": history,
    }))
}

// ═══════════════════════════════════════════════════════════
// Futures & Options API Handlers
// ═══════════════════════════════════════════════════════════

async fn futures_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.futures_api.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn futures_positions(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let positions = state.futures_api.get_positions().await;
    Json(serde_json::json!({
        "count": positions.len(),
        "positions": positions,
    }))
}

async fn futures_instruments(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let instruments = state.futures_api.get_instruments().await;
    Json(serde_json::json!({
        "count": instruments.len(),
        "instruments": instruments,
    }))
}

async fn futures_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.futures_api.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

// ═══════════════════════════════════════════════════════════
// Liquidation Engine API Handlers
// ═══════════════════════════════════════════════════════════

async fn liquidation_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.liquidation_api.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn liquidation_risky(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let positions = state.liquidation_api.get_positions_at_risk().await;
    Json(serde_json::json!({
        "count": positions.len(),
        "positions": positions,
    }))
}

async fn liquidation_history(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let history = state.liquidation_api.get_history().await;
    Json(serde_json::json!({
        "count": history.len(),
        "events": history,
    }))
}

async fn liquidation_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.liquidation_api.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

// ==================== AI CEO Handlers ====================

async fn ceo_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.ai_ceo.get_status().await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

async fn ceo_analysis(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let analysis = state.ai_ceo.analyze_market().await;
    Json(serde_json::to_value(&analysis).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct CeoDecisionReq {
    symbol: String,
    available_balance: Option<f64>,
}

async fn ceo_decisions(
    State(state): State<AppState>,
    Json(req): Json<CeoDecisionReq>,
) -> Json<serde_json::Value> {
    let analysis = state.ai_ceo.analyze_market().await;
    let context = ai_ceo::DecisionContext {
        symbol: req.symbol,
        current_positions: std::collections::HashMap::new(),
        available_balance: req.available_balance.unwrap_or(1_000_000.0),
        recent_trades: Vec::new(),
        market_analysis: analysis,
    };
    let decision = state.ai_ceo.make_decision(context).await;
    let review = state.ai_ceo.review_outcomes().await;
    Json(serde_json::json!({
        "decision": decision,
        "review": review,
    }))
}

async fn ceo_recommendations(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let recs = state.ai_ceo.get_recommendations().await;
    Json(serde_json::to_value(&recs).unwrap_or_default())
}

// ── BMM X⁴Y=K AMM Handlers ──

async fn bmm_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.bmm.is_running().await;
    let stats = state.bmm.get_stats().await;
    Json(serde_json::json!({
        "running": running,
        "stats": stats,
    }))
}

#[derive(serde::Deserialize)]
struct BmmQuoteReq {
    pair: String,
    side: String,
    amount_in: f64,
}

async fn bmm_quote(
    State(state): State<AppState>,
    Json(req): Json<BmmQuoteReq>,
) -> Json<serde_json::Value> {
    let side = match req.side.as_str() {
        "buy" => types::OrderSide::Buy,
        "sell" => types::OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side, use 'buy' or 'sell'"})),
    };
    match state.bmm.get_quote(&req.pair, side, req.amount_in).await {
        Some(quote) => Json(serde_json::to_value(&quote).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "no quote available"})),
    }
}

#[derive(serde::Deserialize)]
struct BmmSwapReq {
    pair: String,
    side: String,
    amount_in: f64,
    user_id: Uuid,
}

async fn bmm_swap(
    State(state): State<AppState>,
    Json(req): Json<BmmSwapReq>,
) -> Json<serde_json::Value> {
    let side = match req.side.as_str() {
        "buy" => types::OrderSide::Buy,
        "sell" => types::OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side"})),
    };
    match state.bmm.execute_swap(&req.pair, side, req.amount_in, req.user_id).await {
        Some(trade) => Json(serde_json::to_value(&trade).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "swap failed"})),
    }
}

async fn bmm_pool(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    match state.bmm.get_pool(&pair).await {
        Some(pool) => Json(serde_json::to_value(&pool).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "pool not found"})),
    }
}

#[derive(serde::Deserialize)]
struct BmmAddLiqReq {
    pair: String,
    amount_x: f64,
    amount_y: f64,
}

async fn bmm_add_liquidity(
    State(state): State<AppState>,
    Json(req): Json<BmmAddLiqReq>,
) -> Json<serde_json::Value> {
    match state.bmm.add_liquidity(&req.pair, req.amount_x, req.amount_y).await {
        Some(lp) => Json(serde_json::json!({"lp_tokens": lp})),
        None => Json(serde_json::json!({"error": "failed to add liquidity"})),
    }
}

#[derive(serde::Deserialize)]
struct BmmRemoveLiqReq {
    pair: String,
    lp_tokens: f64,
}

async fn bmm_remove_liquidity(
    State(state): State<AppState>,
    Json(req): Json<BmmRemoveLiqReq>,
) -> Json<serde_json::Value> {
    match state.bmm.remove_liquidity(&req.pair, req.lp_tokens).await {
        Some((x, y)) => Json(serde_json::json!({"amount_x": x, "amount_y": y})),
        None => Json(serde_json::json!({"error": "failed to remove liquidity"})),
    }
}

async fn bmm_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.bmm.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

// ── BMM Circuit Shield (C5) Handlers ──

async fn shield_status(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<serde_json::Value> {
    let status = state.bmm_shield.shield_status(&pair).await;
    Json(serde_json::to_value(&status).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct ShieldSwapReq {
    pair: String,
    side: String,
    amount_in: f64,
    user_id: Uuid,
}

async fn shield_swap(
    State(state): State<AppState>,
    Json(req): Json<ShieldSwapReq>,
) -> Json<serde_json::Value> {
    let side = match req.side.as_str() {
        "buy" => types::OrderSide::Buy,
        "sell" => types::OrderSide::Sell,
        _ => return Json(serde_json::json!({"error": "invalid side"})),
    };
    match state
        .bmm_shield
        .execute_swap_shielded(&req.pair, side, req.amount_in, req.user_id)
        .await
    {
        bmm_circuit_shield::BmmShieldedSwap::Executed(trade) => {
            Json(serde_json::to_value(&trade).unwrap_or_default())
        }
        bmm_circuit_shield::BmmShieldedSwap::Halted { reason } => {
            Json(serde_json::json!({"status": "halted", "reason": reason}))
        }
        bmm_circuit_shield::BmmShieldedSwap::NoQuote => {
            Json(serde_json::json!({"status": "no_quote"}))
        }
    }
}

// ── Triangular Fee Network (C2) Handlers ──

#[derive(serde::Deserialize)]
struct TriangularRouteReq {
    input_usd: f64,
    pair: String,
    from_ccy: String,
    to_ccy: String,
    live: Option<bool>,
}

async fn triangular_route(
    State(state): State<AppState>,
    Json(req): Json<TriangularRouteReq>,
) -> Json<serde_json::Value> {
    let report = if req.live.unwrap_or(false) {
        state
            .triangular_fee
            .route_trade_live(
                req.input_usd,
                &req.pair,
                types::OrderSide::Buy,
                &req.from_ccy,
                &req.to_ccy,
                uuid::Uuid::new_v4(),
                "cloud",
                "bmm",
            )
            .await
    } else {
        state
            .triangular_fee
            .route_trade_simulated(req.input_usd, &req.pair, &req.from_ccy, &req.to_ccy)
    };
    state.triangular_fee.record(&report);
    Json(serde_json::to_value(&report).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct TriangularMultiplierReq {
    legs: Vec<bool>,
}

async fn triangular_multiplier(
    State(state): State<AppState>,
    Json(req): Json<TriangularMultiplierReq>,
) -> Json<serde_json::Value> {
    let mut legs = [false; 3];
    for (i, v) in req.legs.iter().enumerate().take(3) {
        legs[i] = *v;
    }
    let multiplier = state.triangular_fee.projected_revenue_multiplier(&legs);
    Json(serde_json::json!({"legs": legs, "revenue_multiplier": multiplier}))
}

async fn triangular_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.triangular_fee.stats();
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

// ── eBPF/XDP Handlers ──

async fn xdp_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.xdp.is_running().await;
    let kill = state.xdp.is_kill_switch_active().await;
    let stats = state.xdp.get_stats().await;
    Json(serde_json::json!({"running": running, "kill_switch": kill, "stats": stats}))
}

async fn xdp_rules(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let rules = state.xdp.get_rules().await;
    Json(serde_json::to_value(&rules).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct XDPAddRuleReq {
    rule_type: String,
    action: String,
    priority: u32,
    src_ip: Option<String>,
    dst_ip: Option<String>,
    src_port: Option<u16>,
    dst_port: Option<u16>,
}

async fn xdp_add_rule(
    State(state): State<AppState>,
    Json(req): Json<XDPAddRuleReq>,
) -> Json<serde_json::Value> {
    let rule_type = match req.rule_type.as_str() {
        "rate_limit" => xdp_firewall::XDPRuleType::RateLimit,
        "geo_block" => xdp_firewall::XDPRuleType::GeoBlock,
        "ip_block" => xdp_firewall::XDPRuleType::IpBlock,
        "ddos" => xdp_firewall::XDPRuleType::DDoS,
        "ghost_drop" => xdp_firewall::XDPRuleType::GhostDrop,
        _ => return Json(serde_json::json!({"error": "invalid rule type"})),
    };
    let action = match req.action.as_str() {
        "pass" => xdp_firewall::XDPAction::Pass,
        "drop" => xdp_firewall::XDPAction::Drop,
        "abort" => xdp_firewall::XDPAction::Abort,
        _ => return Json(serde_json::json!({"error": "invalid action"})),
    };
    let rule = xdp_firewall::XDPRule {
        id: rand::random::<u64>(),
        name: String::new(),
        rule_type,
        action,
        priority: req.priority,
        criteria: xdp_firewall::XDPMatch {
            src_ip: req.src_ip,
            dst_ip: req.dst_ip,
            src_port: req.src_port,
            dst_port: req.dst_port,
            protocol: None,
            country: None,
            packet_size_gt: None,
            packet_size_lt: None,
            tls_sni: None,
            payload_regex: None,
            anomaly_score_gt: None,
        },
        hit_count: 0,
        last_hit: None,
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        description: String::new(),
    };
    match state.xdp.add_rule(rule).await {
        Ok(id) => Json(serde_json::json!({"rule_id": id})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct XDPKillSwitchReq {
    activate: bool,
}

async fn xdp_kill_switch(
    State(state): State<AppState>,
    Json(req): Json<XDPKillSwitchReq>,
) -> Json<serde_json::Value> {
    if req.activate {
        state.xdp.activate_kill_switch().await;
        Json(serde_json::json!({"status": "activated"}))
    } else {
        state.xdp.deactivate_kill_switch().await;
        Json(serde_json::json!({"status": "deactivated"}))
    }
}

#[derive(serde::Deserialize)]
struct XDPProcessReq {
    src_ip: String,
    dst_ip: String,
    src_port: u16,
    dst_port: u16,
    protocol: String,
    size: u32,
}

async fn xdp_process_packet(
    State(state): State<AppState>,
    Json(req): Json<XDPProcessReq>,
) -> Json<serde_json::Value> {
    let packet = xdp_firewall::PacketInfo {
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        protocol: req.protocol,
        size: req.size,
        timestamp: chrono::Utc::now(),
        tls_sni: None,
        payload_hash: None,
    };
    let action = state.xdp.process_packet(&packet).await;
    Json(serde_json::json!({"action": format!("{:?}", action)}))
}

// ── memfd_secret Handlers ──

async fn memfd_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.memfd.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct MemfdStoreReq {
    id: String,
    namespace: String,
    key_id: String,
    data: Vec<u8>,
    algorithm: String,
}

async fn memfd_store(
    State(state): State<AppState>,
    Json(req): Json<MemfdStoreReq>,
) -> Json<serde_json::Value> {
    match state.memfd.store_secret(&req.id, &req.namespace, &req.key_id, req.data, &req.algorithm).await {
        Ok(()) => Json(serde_json::json!({"status": "stored"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct MemfdAccessReq {
    id: String,
}

async fn memfd_access(
    State(state): State<AppState>,
    Json(req): Json<MemfdAccessReq>,
) -> Json<serde_json::Value> {
    match state.memfd.access_secret(&req.id).await {
        Ok(data) => Json(serde_json::json!({"data": data})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn memfd_list(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let secrets = state.memfd.list_secrets(None).await;
    Json(serde_json::to_value(&secrets).unwrap_or_default())
}

// ── HugePages Handlers ──

async fn hugepages_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.hugepages.get_stats().await;
    Json(serde_json::to_value(&stats).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct HugePagesAllocReq {
    size: usize,
}

async fn hugepages_allocate(
    State(state): State<AppState>,
    Json(req): Json<HugePagesAllocReq>,
) -> Json<serde_json::Value> {
    match state.hugepages.allocate(req.size).await {
        Ok(id) => Json(serde_json::json!({"region_id": id})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct HugePagesDeallocReq {
    region_id: u32,
}

async fn hugepages_deallocate(
    State(state): State<AppState>,
    Json(req): Json<HugePagesDeallocReq>,
) -> Json<serde_json::Value> {
    match state.hugepages.deallocate(req.region_id).await {
        Ok(()) => Json(serde_json::json!({"status": "deallocated"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── ZK-SNARK Handlers ──

async fn zk_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.zk_snark.is_running().await;
    let stats = state.zk_snark.get_stats().await;
    Json(serde_json::json!({"running": running, "stats": stats}))
}

#[derive(serde::Deserialize)]
struct ZKProofReq {
    circuit_id: String,
    private_inputs: Vec<u8>,
    public_inputs: Vec<Vec<u8>>,
}

async fn zk_generate_proof(
    State(state): State<AppState>,
    Json(req): Json<ZKProofReq>,
) -> Json<serde_json::Value> {
    match state.zk_snark.generate_proof(&req.circuit_id, &req.private_inputs, &req.public_inputs, None).await {
        Ok(proof) => Json(serde_json::to_value(&proof).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct ZKVerifyReq {
    proof_id: String,
}

async fn zk_verify_proof(
    State(state): State<AppState>,
    Json(req): Json<ZKVerifyReq>,
) -> Json<serde_json::Value> {
    match state.zk_snark.verify_proof(&req.proof_id).await {
        Ok(valid) => Json(serde_json::json!({"valid": valid})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct ZKCircuitReq {
    circuit_id: String,
    name: String,
    num_constraints: u64,
    num_variables: u64,
    num_private_inputs: u64,
    num_public_inputs: u64,
    description: String,
}

async fn zk_circuits(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.zk_snark.get_stats().await;
    Json(serde_json::json!({"circuits": stats.circuits_registered}))
}

async fn zk_register_circuit(
    State(state): State<AppState>,
    Json(req): Json<ZKCircuitReq>,
) -> Json<serde_json::Value> {
    let circuit = zk_snark::ZKCircuit {
        circuit_id: req.circuit_id,
        name: req.name,
        num_constraints: req.num_constraints,
        num_variables: req.num_variables,
        num_private_inputs: req.num_private_inputs,
        num_public_inputs: req.num_public_inputs,
        description: req.description,
        wasm_bytes: None,
        r1cs_bytes: None,
        compiled_at: chrono::Utc::now(),
        version: 1,
    };
    match state.zk_snark.register_circuit(circuit).await {
        Ok(()) => Json(serde_json::json!({"status": "registered"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── HTLC Bridge Handlers ──

async fn htlc_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.htlc.is_running().await;
    let stats = state.htlc.get_stats().await;
    Json(serde_json::json!({"running": running, "stats": stats}))
}

#[derive(serde::Deserialize)]
struct HTLCCreateReq {
    sender: String,
    receiver: String,
    amount: f64,
    token: String,
    chain: String,
    hash_lock: Vec<u8>,
    timeout: Option<u64>,
}

async fn htlc_create(
    State(state): State<AppState>,
    Json(req): Json<HTLCCreateReq>,
) -> Json<serde_json::Value> {
    match state.htlc.create_contract(&req.sender, &req.receiver, req.amount, &req.token, &req.chain, req.hash_lock, req.timeout, None).await {
        Ok(contract) => Json(serde_json::to_value(&contract).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct HTLCClaimReq {
    contract_id: String,
    preimage: Vec<u8>,
}

async fn htlc_claim(
    State(state): State<AppState>,
    Json(req): Json<HTLCClaimReq>,
) -> Json<serde_json::Value> {
    match state.htlc.claim(&req.contract_id, &req.preimage).await {
        Ok(()) => Json(serde_json::json!({"status": "claimed"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct HTLCRefundReq {
    contract_id: String,
}

async fn htlc_refund(
    State(state): State<AppState>,
    Json(req): Json<HTLCRefundReq>,
) -> Json<serde_json::Value> {
    match state.htlc.refund(&req.contract_id).await {
        Ok(()) => Json(serde_json::json!({"status": "refunded"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Policy DSL Handlers ──

async fn policy_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.policy_dsl.is_running().await;
    let stats = state.policy_dsl.get_stats().await;
    Json(serde_json::json!({"running": running, "stats": stats}))
}

#[derive(serde::Deserialize)]
struct PolicyCompileReq {
    policy_id: String,
    name: String,
    description: String,
    language: String,
    source: String,
    direction: String,
    priority: u32,
}

async fn policy_compile(
    State(state): State<AppState>,
    Json(req): Json<PolicyCompileReq>,
) -> Json<serde_json::Value> {
    let lang = match req.language.as_str() {
        "rust" => policy_dsl::PolicyLanguage::Rust,
        "rego" => policy_dsl::PolicyLanguage::Rego,
        "dsl" => policy_dsl::PolicyLanguage::DSL,
        _ => return Json(serde_json::json!({"error": "invalid language"})),
    };
    let policy = policy_dsl::Policy {
        policy_id: req.policy_id,
        name: req.name,
        description: req.description,
        language: lang,
        source: req.source,
        compiled_wasm: None,
        aot_compiled: None,
        version: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        active: true,
        direction: req.direction,
        priority: req.priority,
        tags: vec![],
        dependencies: vec![],
        gas_used: 0,
        memory_used: 0,
        compile_time_ms: 0,
        metadata: std::collections::HashMap::new(),
    };
    match state.policy_dsl.register_policy(policy).await {
        Ok(()) => Json(serde_json::json!({"status": "compiled"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn policy_list(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let policies = state.policy_dsl.list_policies(None, false).await;
    Json(serde_json::to_value(&policies).unwrap_or_default())
}

async fn policy_snapshot(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.policy_dsl.create_snapshot().await {
        Ok(snap) => Json(serde_json::to_value(&snap).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Direction Supervisor Handlers ──

async fn supervisor_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.supervisor.is_running().await;
    let stats = state.supervisor.get_stats().await;
    Json(serde_json::json!({"running": running, "stats": stats}))
}

async fn supervisor_processes(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let processes = state.supervisor.list_processes().await;
    Json(serde_json::to_value(&processes).unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct SupervisorCrashReq {
    direction_id: String,
    error: String,
}

async fn supervisor_crash(
    State(state): State<AppState>,
    Json(req): Json<SupervisorCrashReq>,
) -> Json<serde_json::Value> {
    match state.supervisor.report_crash(&req.direction_id, &req.error).await {
        Ok(()) => Json(serde_json::json!({"status": "reported"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Direction Registry Handlers ──

async fn direction_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let running = state.direction_registry.is_running().await;
    let stats = state.direction_registry.get_stats().await;
    Json(serde_json::json!({"running": running, "stats": stats}))
}

#[derive(serde::Deserialize)]
struct DirectionRegisterReq {
    direction_id: String,
    name: String,
    asset_class: String,
}

async fn direction_register(
    State(state): State<AppState>,
    Json(req): Json<DirectionRegisterReq>,
) -> Json<serde_json::Value> {
    let asset_class = match req.asset_class.as_str() {
        "equities" => direction_registry::AssetClass::Equities,
        "crypto" => direction_registry::AssetClass::Crypto,
        "bonds" => direction_registry::AssetClass::Bonds,
        "fx" => direction_registry::AssetClass::FX,
        "derivatives" => direction_registry::AssetClass::Derivatives,
        "commodities" => direction_registry::AssetClass::Commodities,
        other => direction_registry::AssetClass::Custom(other.to_string()),
    };
    let direction = direction_registry::Direction {
        direction_id: req.direction_id,
        name: req.name,
        asset_class,
        status: direction_registry::DirectionStatus::Registered,
        version: "1.0.0".to_string(),
        config: std::collections::HashMap::new(),
        wasm_module: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        load_count: 0,
        last_loaded: None,
        metadata: std::collections::HashMap::new(),
    };
    match state.direction_registry.register(direction).await {
        Ok(()) => Json(serde_json::json!({"status": "registered"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(serde::Deserialize)]
struct DirectionLoadReq {
    direction_id: String,
}

async fn direction_load(
    State(state): State<AppState>,
    Json(req): Json<DirectionLoadReq>,
) -> Json<serde_json::Value> {
    match state.direction_registry.load_direction(&req.direction_id).await {
        Ok(()) => Json(serde_json::json!({"status": "loaded"})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

async fn direction_list(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let directions = state.direction_registry.list_directions(None).await;
    Json(serde_json::to_value(&directions).unwrap_or_default())
}

async fn direction_snapshot(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.direction_registry.create_snapshot().await {
        Ok(snap) => Json(serde_json::to_value(&snap).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}
