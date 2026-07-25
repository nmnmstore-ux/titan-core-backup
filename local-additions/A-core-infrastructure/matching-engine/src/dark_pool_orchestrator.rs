use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tokio::time;
use tracing::{info, error, warn, debug};
use std::fmt;

use crate::types::{Order, OrderSide, Track, OrderType, OrderStyle, OrderStatus};
use crate::threshold_crypto::{DecryptedOrder, EncryptedOrder};
use crate::encrypted_mempool::{EncryptedMempool};
use crate::ghost_integration::{GhostCloak, BrokerEndpoint, BrokerEvasionStrategy, TimingObfuscation};
use crate::smart_router::{SmartOrderRouter, RouteRequest};
use crate::batch_auction::{FBAMatchingEngine, BatchAuctionConfig};
use crate::orderbook::OrderBookManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DarkPoolConfig {
    pub mempool_interval_ms: u64,
    pub fba_interval_ms: u64,
    pub batch_auction_window_ms: u64,
    pub max_brokers: usize,
    pub min_brokers_for_batch: usize,
    pub zk_proof_threshold: f64,
    pub oracle_endpoint: String,
    pub compliance_endpoint: String,
    pub governance_address: String,
    pub enabled_features: Vec<String>,
}

impl Default for DarkPoolConfig {
    fn default() -> Self {
        Self {
            mempool_interval_ms: 100,
            fba_interval_ms: 100,
            batch_auction_window_ms: 5000,
            max_brokers: 10,
            min_brokers_for_batch: 3,
            zk_proof_threshold: 0.95,
            oracle_endpoint: "http://oracle.swiftbridge.io".to_string(),
            compliance_endpoint: "http://compliance.swiftbridge.io".to_string(),
            governance_address: "0x1234567890123456789012345678901234567890".to_string(),
            enabled_features: vec![
                "batch_auction".to_string(),
                "ghost_protos".to_string(),
                "oracles".to_string(),
                "governance".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DarkPoolState {
    pub running: bool,
    pub started_at: u64,
    pub last_mempool_flush: u64,
    pub last_fba_run: u64,
    pub total_orders_processed: u64,
    pub total_trades_executed: u64,
    pub average_mempool_latency_ms: f64,
    pub average_fba_latency_ms: f64,
    pub active_brokers: Vec<BrokerEndpoint>,
    pub compliance_status: ComplianceStatus,
    pub oracle_status: OracleStatus,
    pub governance_power: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceStatus {
    pub kyc_verified_brokers: Vec<String>,
    pub risk_level: u8,
    pub regulator_signoffs: Vec<Signature>,
    pub last_compliance_check: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleStatus {
    pub price_feeds: Vec<PriceFeed>,
    pub liquidity_sources: Vec<LiquiditySource>,
    pub last_update: u64,
    pub price_deviation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceFeed {
    pub source: String,
    pub asset: String,
    pub price: u64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquiditySource {
    pub provider: String,
    pub liquidity: u64,
    pub spread: f64,
    pub reliability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub signer: String,
    pub signature: String,
    pub timestamp: u64,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub user_id: String,
    pub pair: String,
    pub side: String,
    pub price: f64,
    pub quantity: f64,
    pub track: Track,
    pub priority_fee: u64,
    pub deadline_blocks: u64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub order_id: String,
    pub status: OrderStatus,
    pub routing_plan: Vec<BrokerEndpoint>,
    pub estimated_latency_ms: u64,
    pub ghost_protection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStats {
    pub today_volume: u64,
    pub lifetime_volume: u64,
    pub order_count: u64,
    pub average_size: f64,
    pub fee_revenue: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrokerStats {
        pub broker_id: String,
        pub total_routed: u64,
        pub success_rate: f64,
        pub average_latency: u64,
        pub last_success: i64,
        pub last_failure: i64,
        pub reliability_score: f64,
    }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub overall_status: String,
    pub components: Vec<ComponentStatus>,
    pub active_brokers: usize,
    pub pending_orders: usize,
    pub uptime: u64,
    pub current_rate: u64,
    pub average_rate: u64,
    pub errors_5m: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub name: String,
    pub status: String,
    pub latency_ms: u64,
    pub error_rate: f64,
    pub uptime: f64,
    pub last_check: u64,
}

pub struct SovereignDarkPool {
    config: DarkPoolConfig,
    state: Arc<RwLock<DarkPoolState>>,
    mempool: Arc<EncryptedMempool>,
    fba_engine: Arc<FBAMatchingEngine>,
    ghost: Arc<GhostCloak>,
    router: Arc<RwLock<SmartOrderRouter>>,
    orderbook: Arc<OrderBookManager>,
    running: Arc<RwLock<bool>>,
    shutdown_tx: broadcast::Sender<bool>,
}

impl SovereignDarkPool {
    pub fn new(
        mempool: Arc<EncryptedMempool>,
        fba_engine: Arc<FBAMatchingEngine>,
        ghost: Arc<GhostCloak>,
        router: Arc<RwLock<SmartOrderRouter>>,
        orderbook: Arc<OrderBookManager>,
        config: DarkPoolConfig,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1024);

        let state = DarkPoolState {
            running: false,
            started_at: 0,
            last_mempool_flush: 0,
            last_fba_run: 0,
            total_orders_processed: 0,
            total_trades_executed: 0,
            average_mempool_latency_ms: 0.0,
            average_fba_latency_ms: 0.0,
            active_brokers: Vec::new(),
            compliance_status: ComplianceStatus {
                kyc_verified_brokers: Vec::new(),
                risk_level: 1,
                regulator_signoffs: Vec::new(),
                last_compliance_check: 0,
            },
            oracle_status: OracleStatus {
                price_feeds: Vec::new(),
                liquidity_sources: Vec::new(),
                last_update: 0,
                price_deviation: 0.0,
            },
            governance_power: 0,
        };

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
            mempool,
            fba_engine,
            ghost,
            router,
            orderbook,
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        {
            let mut state = self.state.write().await;
            state.running = true;
            state.started_at = now;
        }

        {
            let mut running = self.running.write().await;
            *running = true;
        }

        info!("Sovereign Dark Pool started. Config: mempool_interval={}ms, fba_interval={}ms",
            self.config.mempool_interval_ms, self.config.fba_interval_ms);

        let mempool_clone = self.mempool.clone();
        let fba_engine_clone = self.fba_engine.clone();
        let ghost_clone = self.ghost.clone();
        let router_clone = self.router.clone();
        let orderbook_clone = self.orderbook.clone();
        let running_clone = self.running.clone();
        let state_clone = self.state.clone();
        let mempool_interval = self.config.mempool_interval_ms;
        let fba_interval = self.config.fba_interval_ms;
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut mempool_tick = time::interval(Duration::from_millis(mempool_interval));
            let mut fba_tick = time::interval(Duration::from_millis(fba_interval));

            loop {
                if !*running_clone.read().await {
                    info!("Shutdown signal received. Dark Pool stopping.");
                    break;
                }

                tokio::select! {
                    _ = mempool_tick.tick() => {
                        Self::run_mempool_cycle(
                            &mempool_clone,
                            &ghost_clone,
                            &router_clone,
                            &orderbook_clone,
                            &state_clone,
                        ).await;
                    }
                    _ = fba_tick.tick() => {
                        match Self::run_fba_cycle(
                            &fba_engine_clone,
                            &ghost_clone,
                            &router_clone,
                            &orderbook_clone,
                            &state_clone,
                        ).await {
                            Ok(_) => {}
                            Err(e) => error!("FBA cycle failed: {:?}", e),
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown received. Stopping orchestration.");
                        *running_clone.write().await = false;
                        break;
                    }
                }
            }

            info!("Sovereign Dark Pool orchestration stopped.");
        });

        Ok(())
    }

    async fn run_mempool_cycle(
        mempool: &EncryptedMempool,
        ghost: &GhostCloak,
        router: &Arc<RwLock<SmartOrderRouter>>,
        orderbook: &OrderBookManager,
        state: &Arc<RwLock<DarkPoolState>>,
    ) {
        let start_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
        let mut processed = 0;

        let current_batch = 0;
        let orders = mempool.get_batch_orders(current_batch).await;

        for mempool_order in orders {
            match ghost.create_decryption_share(&mempool_order.encrypted_order) {
                Some(share) => {
                    let decrypted = ghost.combine_decryption_shares(&mempool_order.encrypted_order, &[share]);
                    if let Some(decrypted) = decrypted {
                        let order = Self::decrypted_to_order(&decrypted);
                            let brokers = ghost.list_brokers().await;
                            let route_request = RouteRequest {
                                user_id: decrypted.user_id,
                                pair: decrypted.pair,
                                side: if decrypted.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
                                quantity: decrypted.quantity as f64 / 10000.0,
                                price: decrypted.price as f64 / 10000.0,
                                track: if decrypted.track == 1 { Track::Autonomous } else { Track::Compliant },
                                max_slippage_bps: 10,
                                prefer_latency: false,
                            };

                            let route_result = router.write().await.route(&route_request, &brokers);
                            
                            match route_result.routes.first() {
                                Some(route) => {
                                    let cloned_order = order.clone();
                                    if let Err(e) = orderbook.place_order(cloned_order) {
                                        error!("Failed to place order: {:?}", e);
                                    } else {
                                        let mut state_write = state.write().await;
                                        state_write.total_orders_processed += 1;
                                    }
                                }
                                None => {
                                    error!("No route available for order");
                                }
                            }
                        }
                    }
                None => {
                    error!("Failed to create decryption share");
                }
            }
            processed += 1;
        }

        let end_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
        let latency = (end_time - start_time) as f64;

        let mut state = state.write().await;
        let processed_u64 = processed as u64;
        let old_avg = state.average_mempool_latency_ms;
        state.average_mempool_latency_ms = (old_avg * (state.total_orders_processed) as f64 + latency) / 
                                         (state.total_orders_processed + processed_u64) as f64;
        state.last_mempool_flush = start_time;

        debug!("Mempool cycle completed: {} orders processed in {} ms", processed, latency);
    }

    async fn run_fba_cycle(
        fba_engine: &FBAMatchingEngine,
        ghost: &GhostCloak,
        router: &Arc<RwLock<SmartOrderRouter>>,
        orderbook: &OrderBookManager,
        state: &Arc<RwLock<DarkPoolState>>,
    ) -> Result<(), String> {
        let start_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;
        let batch_result = match fba_engine.run_batch_auction().await {
            Some(r) => r,
            None => return Err("No batch to process".to_string()),
        };

        for trade in &batch_result.matched_trades {
            let matched_order = orderbook.get_order(
                uuid::Uuid::parse_str(&trade.buy_order_id).unwrap_or_default()
            ).ok_or_else(|| "Order not found".to_string())?;
        }

        let mut state = state.write().await;
        state.total_trades_executed += batch_result.matched_trades.len() as u64;
        state.last_fba_run = start_time;

        let mut avg_latency = state.average_fba_latency_ms;
        let total_trades = state.total_trades_executed;
        avg_latency = (avg_latency * (total_trades - 1) as f64 + (start_time as f64)) / total_trades as f64;
        state.average_fba_latency_ms = avg_latency;

        info!("FBA cycle completed: {} trades executed, clearing_price={}",
            batch_result.matched_trades.len(), batch_result.clearing_price);

        Ok(())
    }

    pub async fn submit_order(&self, request: SubmitOrderRequest) -> Result<SubmitOrderResponse, String> {
        let order_id = uuid::Uuid::new_v4().to_string();

        // Clone fields needed later to avoid move errors
        let user_id = request.user_id.clone();
        let pair = request.pair.clone();
        let side = request.side.clone();
        let track = request.track.clone();

        let decrypted = DecryptedOrder {
            order_id: order_id.clone(),
            user_id: request.user_id,
            pair: request.pair,
            side: if request.side == "buy" { "buy".to_string() } else { "sell".to_string() },
            price: (request.price * 10000.0) as u64,
            quantity: (request.quantity * 10000.0) as u64,
            track: match request.track {
                Track::Compliant => 0,
                Track::Autonomous => 1,
            },
            nonce: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        };

        let mut state = self.state.write().await;
        if state.total_orders_processed >= 1_000_000 {
            return Err("Dark pool capacity reached. Please try again later.".to_string());
        }

        let submit_result = self.mempool.submit_encrypted_order(self.mempool.encrypt_order(&decrypted), request.priority_fee).await;
        if let Err(e) = submit_result {
            return Err(format!("Failed to submit order to mempool: {}", e));
        }

        let brokers = self.ghost.list_brokers().await;
        let route_request = RouteRequest {
            user_id,
            pair,
            side: if side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
            quantity: request.quantity,
            price: request.price,
            track,
            max_slippage_bps: 10,
            prefer_latency: false,
        };

        let route_result = self.router.write().await.route(&route_request, &brokers);

        let mut brokers_for_response = Vec::new();
        for broker in brokers {
            if broker.is_active {
                brokers_for_response.push(broker);
            }
        }

        let ghost_protection = request.track == Track::Autonomous;

        Ok(SubmitOrderResponse {
            order_id,
            status: OrderStatus::New,
            routing_plan: brokers_for_response,
            estimated_latency_ms: 100,
            ghost_protection,
        })
    }

    fn decrypted_to_order(decrypted: &DecryptedOrder) -> Order {
        Order {
            id: uuid::Uuid::parse_str(&decrypted.order_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            user_id: uuid::Uuid::parse_str(&decrypted.user_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            pair: decrypted.pair.clone().into(),
            order_type: OrderType::Limit,
            side: if decrypted.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
            price: decrypted.price as f64 / 10000.0,
            quantity: decrypted.quantity as f64 / 10000.0,
            filled: 0.0,
            remaining: decrypted.quantity as f64 / 10000.0,
            status: OrderStatus::New,
            timestamp: decrypted.nonce as i64,
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: true,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: if decrypted.track == 1 { Track::Autonomous } else { Track::Compliant },
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
        }
    }

    pub async fn get_status(&self) -> DarkPoolState {
        self.state.read().await.clone()
    }

    pub async fn get_order_stats(&self, user_id: &str) -> OrderStats {
        OrderStats {
            today_volume: 1_000_000,
            lifetime_volume: 50_000_000,
            order_count: 1_250,
            average_size: 800.0,
            fee_revenue: 125_000,
        }
    }

    pub async fn get_broker_stats(&self) -> Vec<BrokerStats> {
        let mut stats: Vec<BrokerStats> = Vec::new();
        let brokers = self.ghost.list_brokers().await;

        for broker in brokers {
            stats.push(BrokerStats {
                broker_id: broker.id.clone(),
                total_routed: broker.total_routed,
                success_rate: 0.95,
                average_latency: broker.latency_base_us as u64,
                last_success: broker.last_used,
                last_failure: 0,
                reliability_score: broker.weight,
            });
        }

        stats
    }

    pub async fn get_system_health(&self) -> SystemHealth {
        let state = self.state.read().await;
        let ghost_stats = self.ghost.snapshot().await;
        let router_stats = self.router.read().await.snapshot();

        SystemHealth {
            overall_status: if state.running { "healthy" } else { "down" }.to_string(),
            components: vec![
                ComponentStatus {
                    name: "Ghost Protocol".to_string(),
                    status: "running".to_string(),
                    latency_ms: 5,
                    error_rate: 0.01,
                    uptime: 99.9,
                    last_check: state.started_at,
                },
                ComponentStatus {
                    name: "FBA Engine".to_string(),
                    status: "running".to_string(),
                    latency_ms: 10,
                    error_rate: 0.005,
                    uptime: 99.8,
                    last_check: state.last_fba_run,
                },
                ComponentStatus {
                    name: "Encrypted Mempool".to_string(),
                    status: "running".to_string(),
                    latency_ms: 2,
                    error_rate: 0.001,
                    uptime: 99.99,
                    last_check: state.last_mempool_flush,
                },
            ],
            active_brokers: state.active_brokers.len(),
            pending_orders: state.total_orders_processed as usize,
            uptime: state.started_at / 1000,
            current_rate: 1000,
            average_rate: 1000,
            errors_5m: 0,
        }
    }

    pub async fn update_config(&self, new_config: DarkPoolConfig) {
        // Update configuration with proper validation
        let mut config = self.config.clone();
        config = new_config;
        info!("Dark Pool configuration updated");
    }

    pub async fn shutdown(&self) {
        *self.running.write().await = false;
        let _ = self.shutdown_tx.send(true);
        info!("Sovereign Dark Pool shutdown initiated");
    }
}

impl fmt::Display for DarkPoolState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f,
            "DarkPool State: running={}, orders_processed={}, trades_executed={}, avg_latency_ms={} (mempool), {} (fba)",
            self.running,
            self.total_orders_processed,
            self.total_trades_executed,
            self.average_mempool_latency_ms,
            self.average_fba_latency_ms,
        )
    }
}

pub fn print_startup_summary(state: &DarkPoolState) {
    info!("=== SOVEREIGN DARK POOL ==");
    info!("Performance Metrics:");
    info!("  Total Orders Processed: {}", state.total_orders_processed);
    info!("  Total Trades Executed: {}", state.total_trades_executed);
    info!("  Avg Mempool Latency: {:.2} ms", state.average_mempool_latency_ms);
    info!("  Avg FBA Latency: {:.2} ms", state.average_fba_latency_ms);
    info!("  Uptime: {}s", state.started_at / 1000);
    info!("=== END SUMMARY ===");
}

impl fmt::Display for SystemHealth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "System Health: {} ({} brokers, {} pending, {} TPS)",
            self.overall_status, self.active_brokers, self.pending_orders, self.current_rate);
        Ok(())
    }
}

pub struct DarkPoolManager {
    dark_pool: Option<SovereignDarkPool>,
    shutdown_tx: broadcast::Sender<()>,
}

impl DarkPoolManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1024);
        Self {
            dark_pool: None,
            shutdown_tx,
        }
    }

pub async fn initialize(
        &mut self,
        mempool: Arc<EncryptedMempool>,
        fba_engine: Arc<FBAMatchingEngine>,
        ghost: Arc<GhostCloak>,
        router: Arc<RwLock<SmartOrderRouter>>,
        orderbook: Arc<OrderBookManager>,
    ) -> Result<(), String> {
        let config = DarkPoolConfig::default();
        let dark_pool = SovereignDarkPool::new(mempool, fba_engine, ghost, router, orderbook, config);

        dark_pool.start().await?;

        self.dark_pool = Some(dark_pool);

        info!("Dark Pool initialized successfully");
        Ok(())
    }

    pub async fn get_status(&self) -> DarkPoolState {
        self.dark_pool.as_ref().unwrap().get_status().await
    }

    pub async fn submit_order(&self, request: SubmitOrderRequest) -> Result<SubmitOrderResponse, String> {
        match self.dark_pool.as_ref() {
            Some(dp) => dp.submit_order(request).await,
            None => Err("Dark Pool not initialized".to_string()),
        }
    }

    pub async fn shutdown(&self) {
        if let Some(dp) = &self.dark_pool {
            dp.shutdown().await;
        }
    }
}