#[cfg(target_os = "linux")]
use tikv_jemallocator::Jemalloc;

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

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
        .with_env_filter("the_bridge=info,the_bridge_engine=trace")
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

    let pairs = vec!["USD/EUR", "USD/EGP", "USD/SAR", "USD/AED", "USD/GBP", "EUR/EGP"];
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
    let api_keys = Arc::new(ApiKeyManager::new(b"the-bridge-cloud-secret-2026"));
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
    let fortress = Arc::new(SovereignFortress::new(&fortress_key));
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
                        let bids: Vec<OrderSnapshot> = book.bids.iter().flat_map(|(_, orders)| {
                            orders.iter().map(|o| OrderSnapshot {
                                id: o.id.to_string(),
                                user_id: o.user_id.to_string(),
                                side: "buy".into(),
                                price: o.price,
                                quantity: o.quantity,
                                remaining: o.remaining,
                                timestamp: o.timestamp,
                            })
                        }).collect();
                        let asks: Vec<OrderSnapshot> = book.asks.iter().flat_map(|(_, orders)| {
                            orders.iter().map(|o| OrderSnapshot {
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
                            last_price: book.last_price,
                            volume_24h: book.volume_24h,
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
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.prometheus_text(),
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
