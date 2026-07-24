use std::fmt;

use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, error, warn, debug};

use crate::types::{Order, OrderSide, Track, OrderStatus, OrderType, OrderStyle};
use crate::threshold_crypto::{ThresholdCrypto, DecryptedOrder, EncryptedOrder, ZKProof};
use crate::encrypted_mempool::{EncryptedMempool, MempoolOrder, BatchAuctionResult as MempoolBatchAuctionResult};
use crate::ghost_integration::{GhostCloak, BrokerEndpoint, BrokerEvasionStrategy, TimingObfuscation};
use crate::smart_router::{SmartOrderRouter, RouteRequest, RouteResult};
use crate::batch_auction::{FBAMatchingEngine, BatchAuctionConfig, BatchAuctionResult, BatchAuctionStats};
use crate::orderbook::OrderBookManager;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub order: Order,
    pub track: Track,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub order_id: String,
    pub status: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DarkPoolState {
    pub running: bool,
    pub started_at: u64,
    pub last_mempool_flush: u64,
    pub last_fba_run: u64,
    pub total_orders_processed: u64,
    pub total_trades_executed: u64,
    pub average_mempool_latency_ms: f64,
    pub average_fba_latency_ms: f64,
    pub active_brokers: Vec<String>,
    pub compliance_status: ComplianceStatus,
    pub oracle_status: OracleStatus,
    pub governance_power: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceStatus {
    pub kyc_verified_brokers: Vec<String>,
    pub risk_level: u8,
    pub regulator_signoffs: Vec<String>,
    pub last_compliance_check: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OracleStatus {
    pub price_feeds: Vec<String>,
    pub liquidity_sources: Vec<String>,
    pub last_update: u64,
    pub price_deviation: f64,
}

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
                "ghost_protocol".to_string(),
                "oracles".to_string(),
                "governance".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DarkPoolStats {
    pub tps_current: u64,
    pub daily_volume: u64,
    pub avg_order_size: f64,
    pub fee_revenue: u64,
}

impl std::fmt::Display for DarkPoolStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f,
            "TPS: {}  |  Volume: {}  |  Avg Size: {:.2}  |  Fees: {}",
            self.tps_current,
            self.daily_volume,
            self.avg_order_size,
            self.fee_revenue,
        )
    }
}

pub struct SovereignDarkPool {
    mempool: Arc<EncryptedMempool>,
    fba_engine: Arc<FBAMatchingEngine>,
    ghost: Arc<GhostCloak>,
    router: Arc<RwLock<SmartOrderRouter>>,
    orderbook: Arc<OrderBookManager>,
    config: DarkPoolConfig,
    state: Arc<RwLock<DarkPoolState>>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
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
        Self {
            mempool,
            fba_engine,
            ghost,
            router,
            orderbook,
            config,
            state: Arc::new(RwLock::new(DarkPoolState::default())),
            shutdown_tx: tokio::sync::broadcast::channel(1024).0,
        }
    }

    pub fn config(&self) -> DarkPoolConfig {
        self.config.clone()
    }

    pub async fn start(&mut self) -> Result<(), String> {
        let mut state = self.state.write().await;
        state.running = true;
        state.started_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let mempool = self.mempool.clone();
        let fba_engine = self.fba_engine.clone();
        let ghost = self.ghost.clone();
        let router = self.router.clone();
        let orderbook = self.orderbook.clone();
        let state = self.state.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut mempool_interval = tokio::time::interval(Duration::from_millis(100));
            let mut fba_interval = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown received. Stopping orchestration.");
                        break;
                    }
                    _ = mempool_interval.tick() => {
                        let _ = Self::run_mempool_cycle_static(
                            mempool.clone(),
                            ghost.clone(),
                            router.clone(),
                            orderbook.clone(),
                            state.clone(),
                        ).await;
                    }
                    _ = fba_interval.tick() => {
                        let _ = Self::run_fba_cycle_static(
                            fba_engine.clone(),
                            ghost.clone(),
                            router.clone(),
                            orderbook.clone(),
                            state.clone(),
                        ).await;
                    }
                }
            }
            
            info!("Sovereign Dark Pool orchestration stopped.");
        });

        Ok(())
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    pub async fn run_fba_cycle(&self) -> Result<(), String> {
        Ok(())
    }

    pub async fn run_auction(&self) -> Result<BatchAuctionResult, String> {
        let result = self.fba_engine.run_batch_auction().await;
        result.ok_or_else(|| "No auction result".to_string())
    }

    pub async fn get_status(&self) -> DarkPoolState {
        let state = self.state.read().await;
        (*state).clone()
    }

    pub async fn submit_order(&self, request: SubmitOrderRequest) -> Result<SubmitOrderResponse, String> {
        let decrypted = DecryptedOrder {
            order_id: request.order.id.to_string(),
            user_id: request.order.user_id.to_string(),
            pair: request.order.pair.to_string(),
            side: if request.order.side == OrderSide::Buy { "buy".to_string() } else { "sell".to_string() },
            price: (request.order.price * 10000.0) as u64,
            quantity: (request.order.quantity * 10000.0) as u64,
            track: if request.track == Track::Autonomous { 1 } else { 0 },
            nonce: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64,
        };

        let encrypted = self.mempool.encrypt_order(&decrypted);
        self.mempool.submit_encrypted_order(encrypted, 0).await?;
        
        // Also submit to FBA engine
        let order_clone = request.order.clone();
        self.fba_engine.submit_order_fba(order_clone).await?;
        
        Ok(SubmitOrderResponse {
            order_id: request.order.id.to_string(),
            status: "accepted".to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        })
    }

    async fn run_mempool_cycle_static(
        mempool: Arc<EncryptedMempool>,
        ghost: Arc<GhostCloak>,
        router: Arc<RwLock<SmartOrderRouter>>,
        orderbook: Arc<OrderBookManager>,
        state: Arc<RwLock<DarkPoolState>>,
    ) -> Result<(), String> {
        let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u128;
        let mut processed = 0;

        let orders = mempool.get_batch_orders(0).await;

        for mempool_order in orders {
            if let Some(share) = ghost.create_decryption_share(&mempool_order.encrypted_order) {
                if let Some(decrypted) = ghost.combine_decryption_shares(&mempool_order.encrypted_order, &[share]) {
                    let order = Self::decrypted_to_order(&decrypted);
                    let brokers = ghost.list_brokers().await;
                    let route_request = RouteRequest {
                        user_id: decrypted.user_id.clone(),
                        pair: decrypted.pair.clone(),
                        side: if decrypted.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
                        quantity: decrypted.quantity as f64 / 10000.0,
                        price: decrypted.price as f64 / 10000.0,
                        track: if decrypted.track == 1 { Track::Autonomous } else { Track::Compliant },
                        max_slippage_bps: 10,
                        prefer_latency: false,
                    };

                    let route_result = router.write().await.route(&route_request, &brokers);
                    
                    if let Some(_route) = route_result.routes.first() {
                        let cloned_order = order.clone();
                        if let Err(e) = orderbook.place_order(cloned_order) {
                            error!("Failed to place order: {}", e);
                        }
                    } else {
                        warn!("No route found for order");
                    }
                    processed += 1;
                }
            }
        }

        let end_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u128;
        
        let mut state_guard = state.write().await;
        state_guard.total_orders_processed += processed as u64;
        state_guard.last_mempool_flush = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        
        let latency = (end_time - start_time) as f64;
        state_guard.average_mempool_latency_ms = 
            (state_guard.average_mempool_latency_ms * (state_guard.total_orders_processed - processed) as f64 + latency) / state_guard.total_orders_processed as f64;

        Ok(())
    }

    async fn run_fba_cycle_static(
        fba_engine: Arc<FBAMatchingEngine>,
        _ghost: Arc<GhostCloak>,
        _router: Arc<RwLock<SmartOrderRouter>>,
        _orderbook: Arc<OrderBookManager>,
        state: Arc<RwLock<DarkPoolState>>,
    ) -> Result<(), String> {
        let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u128;
        
        let _ = fba_engine.run_batch_auction().await;
        
        let end_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u128;
        
        let mut state_guard = state.write().await;
        state_guard.last_fba_run = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let latency = (end_time - start_time) as f64;
        state_guard.average_fba_latency_ms = 
            (state_guard.average_fba_latency_ms * state_guard.total_trades_executed as f64 + latency) / (state_guard.total_trades_executed + 1) as f64;

        Ok(())
    }

    fn decrypted_to_order(decrypted: &DecryptedOrder) -> Order {
        use crate::types::{Order, OrderSide, Track, OrderStatus, OrderStyle, OrderType};
        use uuid::Uuid;
        use compact_str::CompactString;
        
        Order {
            id: Uuid::parse_str(&decrypted.order_id).unwrap_or_else(|_| Uuid::new_v4()),
            user_id: Uuid::parse_str(&decrypted.user_id).unwrap_or_else(|_| Uuid::new_v4()),
            pair: decrypted.pair.clone().into(),
            side: if decrypted.side == "buy" { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::Limit,
            style: OrderStyle::Standard,
            price: decrypted.price as f64 / 10000.0,
            quantity: decrypted.quantity as f64 / 10000.0,
            filled: 0.0,
            remaining: decrypted.quantity as f64 / 10000.0,
            status: OrderStatus::New,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as i64,
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: false,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: if decrypted.track == 1 { Track::Autonomous } else { Track::Compliant },
            hidden_remaining: 0.0,
            filled_quantity: 0,
            client_order_id: None,
        }
    }

    async fn list_brokers(&self) -> Vec<BrokerEndpoint> {
        Vec::new()
    }
}

pub struct DarkPoolManager {
    dark_pool: Option<SovereignDarkPool>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl DarkPoolManager {
    pub fn new() -> Self {
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1024);
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
        let mut dark_pool = SovereignDarkPool::new(mempool, fba_engine, ghost, router, orderbook, config);
        
        dark_pool.start().await?;

        self.dark_pool = Some(dark_pool);
        
        info!("Dark Pool initialized successfully");
        Ok(())
    }

    pub async fn get_status(&self) -> DarkPoolState {
        match self.dark_pool.as_ref() {
            Some(dp) => dp.get_status().await,
            None => DarkPoolState::default(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threshold_crypto::ThresholdCrypto;
    use crate::encrypted_mempool::EncryptedMempool;
    use crate::batch_auction::{FBAMatchingEngine, BatchAuctionConfig};
    use crate::ghost_integration::GhostCloak;
    use crate::smart_router::SmartOrderRouter;
    use crate::orderbook::OrderBookManager;
    use tokio::sync::RwLock;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_dark_pool_basic() {
        let crypto = Arc::new(ThresholdCrypto::new(2, 3));
        crypto.run_dkg(0);
        crypto.run_dkg(1);

        let mempool = Arc::new(EncryptedMempool::new(crypto, 100, 1000));
        let fba_engine = Arc::new(FBAMatchingEngine::new(
            Arc::new(OrderBookManager::new()),
            BatchAuctionConfig::default()
        ));
        let ghost = Arc::new(GhostCloak::new());
        let router = Arc::new(RwLock::new(SmartOrderRouter::new()));
        let orderbook = Arc::new(OrderBookManager::new());

        let mut manager = DarkPoolManager::new();
        manager.initialize(mempool, fba_engine, ghost, router, orderbook).await.unwrap();

        let status = manager.get_status().await;
        assert!(!status.running);
    }
}