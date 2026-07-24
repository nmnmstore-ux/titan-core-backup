use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, debug, error};
use std::fmt;

use crate::types::{Order, OrderSide, Track, OrderStatus, OrderType, OrderStyle};
use crate::threshold_crypto::{ThresholdCrypto, DecryptedOrder, EncryptedOrder};
use crate::encrypted_mempool::{EncryptedMempool, BatchAuctionResult as MempoolBatchAuctionResult};
use crate::ghost_integration::{GhostCloak, BrokerEvasionStrategy, BrokerEndpoint, TimingObfuscation};
use crate::smart_router::{SmartOrderRouter, RouteRequest};
use crate::batch_auction::{FBAMatchingEngine, BatchAuctionConfig};
use crate::orderbook::OrderBookManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub mempool_interval_ms: u64,
    pub fba_interval_ms: u64,
    pub threshold_crypto_params: (usize, usize),
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            mempool_interval_ms: 100,
            fba_interval_ms: 100,
            threshold_crypto_params: (3, 5),
        }
    }
}

pub struct SovereignDarkPoolOrchestrator {
    config: OrchestratorConfig,
    mempool: Arc<EncryptedMempool>,
    fba_engine: Arc<FBAMatchingEngine>,
    ghost: Arc<GhostCloak>,
    router: Arc<RwLock<SmartOrderRouter>>,
    orderbook: Arc<OrderBookManager>,
    running: Arc<RwLock<bool>>,
}

impl SovereignDarkPoolOrchestrator {
    pub fn new(
        mempool: Arc<EncryptedMempool>,
        fba_engine: Arc<FBAMatchingEngine>,
        ghost: Arc<GhostCloak>,
        router: Arc<RwLock<SmartOrderRouter>>,
        orderbook: Arc<OrderBookManager>,
        config: OrchestratorConfig,
    ) -> Self {
        Self {
            config,
            mempool,
            fba_engine,
            ghost,
            router,
            orderbook,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        *self.running.write().await = true;
        info!("Sovereign Dark Pool Orchestrator started");

        let mempool_clone = self.mempool.clone();
        let fba_engine_clone = self.fba_engine.clone();
        let ghost_clone = self.ghost.clone();
        let router_clone = self.router.clone();
        let orderbook_clone = self.orderbook.clone();
        let running_clone = self.running.clone();

        let mempool_interval = Duration::from_millis(self.config.mempool_interval_ms);
        let fba_interval = Duration::from_millis(self.config.fba_interval_ms);

        tokio::spawn(async move {
            let mut mempool_tick = tokio::time::interval(mempool_interval);
            let mut fba_tick = tokio::time::interval(fba_interval);

            loop {
                if !*running_clone.read().await {
                    break;
                }

                tokio::select! {
                    _ = mempool_tick.tick() => {
                        match mempool_clone.get_pending_count().await {
                            count if count > 0 => {
                                debug!("Processing {} pending encrypted orders", count);
                                Self::process_mempool_orders(
                                    &mempool_clone,
                                    &ghost_clone,
                                    &router_clone,
                                    &orderbook_clone,
                                ).await;
                            }
                            _ => {}
                        }
                    }
                    _ = fba_tick.tick() => {
                        match Self::run_fba_cycle(&fba_engine_clone, &ghost_clone, &router_clone, &orderbook_clone).await {
                            Ok(_) => {}
                            Err(e) => error!("FBA cycle failed: {}", e),
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Sovereign Dark Pool Orchestrator stopped");
    }

    async fn process_mempool_orders(
        mempool: &EncryptedMempool,
        ghost: &GhostCloak,
        router: &Arc<RwLock<SmartOrderRouter>>,
        orderbook: &OrderBookManager,
    ) {
        let current_batch = 0;
        let orders = mempool.get_batch_orders(current_batch).await;

        if orders.is_empty() {
            return;
        }

        for mempool_order in orders {
            let share = match ghost.create_decryption_share(&mempool_order.encrypted_order) {
                Some(s) => s,
                None => {
                    error!("Failed to create decryption share");
                    continue;
                }
            };

            let decrypted = match ghost.combine_decryption_shares(&mempool_order.encrypted_order, &[share]) {
                Some(result) => result,
                None => continue,
            };

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

            let mut router_lock = router.write().await;
            let route_result = router_lock.route(&route_request, &brokers);

            if let Some(route) = route_result.routes.first() {
                let cloned_order = order.clone();
                if let Err(e) = orderbook.place_order(cloned_order) {
                    error!("Failed to place order: {:?}", e);
                    router_lock.record_failure(&route.broker_id);
                } else {
                    router_lock.record_success(&route.broker_id, route.estimated_latency_us as u64);
                    ghost.mark_fragment_settled(&route.broker_id, "tx").await;
                }
            }
        }
    }

    async fn run_fba_cycle(
        fba_engine: &FBAMatchingEngine,
        ghost: &GhostCloak,
        router: &Arc<RwLock<SmartOrderRouter>>,
        orderbook: &OrderBookManager,
    ) -> Result<(), String> {
        let result = match fba_engine.run_batch_auction().await {
            Some(r) => r,
            None => return Err("No batch to process".to_string()),
        };

        info!("FBA cycle completed: {} trades, clearing_price={}",
            result.matched_trades.len(), result.clearing_price);

        for trade in &result.matched_trades {
            let matched_order = orderbook.get_order(
                uuid::Uuid::parse_str(&trade.buy_order_id).unwrap_or_default()
            ).ok_or_else(|| "Order not found".to_string())?;

            ghost.mark_fragment_settled(&matched_order.id.to_string(), &trade.buy_order_id).await;
        }

        let fba_stats = fba_engine.get_fba_stats().await;
        info!("FBA stats: {} batches, {} volume", fba_stats.total_batches, fba_stats.total_volume_all_time);

        Ok(())
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
            filled_quantity: 0,
            client_order_id: None,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
        }
    }

    pub async fn get_system_status(&self) -> SystemStatus {
        let ghost_stats = self.ghost.snapshot().await;
        let router_stats = self.router.read().await.snapshot();
        let orderbook_stats = Self::get_orderbook_stats(self.orderbook.clone()).await;
        let fba_stats = self.fba_engine.get_fba_stats().await;

        SystemStatus {
            ghost: ghost_stats,
            router: router_stats,
            mempool_pending: self.mempool.get_pending_count().await,
            orderbook_stats,
            fba_stats,
            timestamp: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64,
        }
    }

    async fn get_orderbook_stats(book: Arc<OrderBookManager>) -> OrderBookStats {
        OrderBookStats {
            total_orders: book.total_orders(),
            total_trades: book.total_trades(),
            tps_current: book.tps_current(),
            tps_peak: book.tps_peak(),
            active_pairs: book.active_pairs() as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub ghost: serde_json::Value,
    pub router: serde_json::Value,
    pub mempool_pending: usize,
    pub orderbook_stats: OrderBookStats,
    pub fba_stats: crate::batch_auction::BatchAuctionStats,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookStats {
    pub total_orders: u64,
    pub total_trades: u64,
    pub tps_current: u64,
    pub tps_peak: u64,
    pub active_pairs: u64,
}

pub struct DarkPoolOrchestratorBuilder {
    mempool: Option<Arc<EncryptedMempool>>,
    fba_engine: Option<Arc<FBAMatchingEngine>>,
    ghost: Option<Arc<GhostCloak>>,
    router: Option<Arc<RwLock<SmartOrderRouter>>>,
    orderbook: Option<Arc<OrderBookManager>>,
    config: OrchestratorConfig,
}

impl DarkPoolOrchestratorBuilder {
    pub fn new() -> Self {
        Self {
            mempool: None,
            fba_engine: None,
            ghost: None,
            router: None,
            orderbook: None,
            config: OrchestratorConfig::default(),
        }
    }

    pub fn with_mempool(mut self, mempool: Arc<EncryptedMempool>) -> Self {
        self.mempool = Some(mempool);
        self
    }

    pub fn with_fba_engine(mut self, fba_engine: Arc<FBAMatchingEngine>) -> Self {
        self.fba_engine = Some(fba_engine);
        self
    }

    pub fn with_ghost(mut self, ghost: Arc<GhostCloak>) -> Self {
        self.ghost = Some(ghost);
        self
    }

    pub fn with_router(mut self, router: Arc<RwLock<SmartOrderRouter>>) -> Self {
        self.router = Some(router);
        self
    }

    pub fn with_orderbook(mut self, orderbook: Arc<OrderBookManager>) -> Self {
        self.orderbook = Some(orderbook);
        self
    }

    pub fn build(self) -> Result<SovereignDarkPoolOrchestrator, String> {
        let mempool = self.mempool.ok_or("mempool not set")?;
        let fba_engine = self.fba_engine.ok_or("fba_engine not set")?;
        let ghost = self.ghost.ok_or("ghost not set")?;
        let router = self.router.ok_or("router not set")?;
        let orderbook = self.orderbook.ok_or("orderbook not set")?;

        Ok(SovereignDarkPoolOrchestrator::new(
            mempool,
            fba_engine,
            ghost,
            router,
            orderbook,
            self.config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_orchestrator_integration() {
        let crypto = Arc::new(ThresholdCrypto::new(3, 5));
        crypto.run_dkg(0);
        crypto.run_dkg(1);

        let mempool = Arc::new(EncryptedMempool::new(crypto.clone(), 100, 1000));
        let fba_engine = Arc::new(FBAMatchingEngine::new(
            Arc::new(OrderBookManager::new()),
            BatchAuctionConfig::default()
        ));
        let ghost = Arc::new(GhostCloak::new());
        let router = Arc::new(RwLock::new(SmartOrderRouter::new()));
        let orderbook = Arc::new(OrderBookManager::new());

        let orchestrator = SovereignDarkPoolOrchestrator::new(
            mempool.clone(),
            fba_engine.clone(),
            ghost.clone(),
            router.clone(),
            orderbook.clone(),
            OrchestratorConfig::default(),
        );

        let status = orchestrator.get_system_status().await;
        assert_eq!(status.mempool_pending, 0);

        let order = Order {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            pair: "USD/EGP".into(),
            order_type: OrderType::Limit,
            side: OrderSide::Buy,
            price: 30.50,
            quantity: 1000.0,
            filled: 0.0,
            remaining: 1000.0,
            status: OrderStatus::New,
            timestamp: chrono::Utc::now().timestamp_millis(),
            ttl_ms: None,
            is_swap: false,
            swap_target_currency: None,
            tee_signed: false,
            dot_verified: false,
            stealth: true,
            trailing_offset: None,
            trigger_price: None,
            hard_floor: None,
            track: Track::Autonomous,
            style: OrderStyle::Standard,
            hidden_remaining: 0.0,
        };

        let decrypted = DecryptedOrder {
            order_id: "test_order_1".into(),
            user_id: "test_user_1".into(),
            pair: "USD/EGP".into(),
            side: "buy".into(),
            price: 305000,
            quantity: 100000,
            track: 1,
            nonce: 12345,
        };

        let decrypted_sell = DecryptedOrder {
            order_id: "test_order_2".into(),
            user_id: "test_user_2".into(),
            pair: "USD/EGP".into(),
            side: "sell".into(),
            price: 300000,
            quantity: 100000,
            track: 1,
            nonce: 12346,
        };

        let encrypted = crypto.encrypt_order(&decrypted);
        let encrypted_sell = crypto.encrypt_order(&decrypted_sell);
        mempool.submit_encrypted_order(encrypted, 1000).await.unwrap();
        mempool.submit_encrypted_order(encrypted_sell, 1000).await.unwrap();

        let status = orchestrator.get_system_status().await;
        assert_eq!(status.mempool_pending, 1);

        let fba_result = fba_engine.run_batch_auction().await.unwrap();
        assert!(!fba_result.matched_trades.is_empty());
    }
}

impl fmt::Display for SystemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "System Status: Ghost Protocol={}, Router={}, Pending Orders={}, Timestamp={}",
            self.ghost["cloaked_orders"].as_u64().unwrap_or(0),
            self.router["history"].as_object().unwrap_or(&serde_json::Map::new()).len(),
            self.mempool_pending,
            self.timestamp,
        )
    }
}