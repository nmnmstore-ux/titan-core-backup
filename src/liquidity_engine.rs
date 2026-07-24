use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityConfig {
    pub enable_cross_venue: bool,
    pub enable_synthetic: bool,
    pub enable_internalization: bool,
    pub min_liquidity_score: f64,
    pub max_slippage_bps: u32,
    pub liquidity_incentive_bps: u32,
    pub market_maker_rebate_bps: u32,
    pub depth_refresh_ms: u64,
    pub venues: Vec<VenueConfig>,
    pub synthetic_pools: Vec<SyntheticPoolConfig>,
}

impl Default for LiquidityConfig {
    fn default() -> Self {
        Self {
            enable_cross_venue: true,
            enable_synthetic: true,
            enable_internalization: true,
            min_liquidity_score: 0.7,
            max_slippage_bps: 10,
            liquidity_incentive_bps: 3,
            market_maker_rebate_bps: 2,
            depth_refresh_ms: 100,
            venues: vec![
                VenueConfig {
                    venue_id: "BINANCE".to_string(),
                    venue_type: VenueType::CEX,
                    api_endpoint: "wss://stream.binance.com:9443".to_string(),
                    symbols: vec!["BTC/USDT".to_string(), "ETH/USDT".to_string()],
                    fee_bps: 10,
                    latency_ms: 50,
                    enabled: true,
                    weight: 1.0,
                },
                VenueConfig {
                    venue_id: "COINBASE".to_string(),
                    venue_type: VenueType::CEX,
                    api_endpoint: "wss://ws-feed.pro.coinbase.com".to_string(),
                    symbols: vec!["BTC/USD".to_string(), "ETH/USD".to_string()],
                    fee_bps: 15,
                    latency_ms: 80,
                    enabled: true,
                    weight: 0.8,
                },
                VenueConfig {
                    venue_id: "UNISWAP_V3".to_string(),
                    venue_type: VenueType::DEX,
                    api_endpoint: "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3".to_string(),
                    symbols: vec!["WETH/USDC".to_string(), "WBTC/USDC".to_string()],
                    fee_bps: 30,
                    latency_ms: 500,
                    enabled: true,
                    weight: 0.6,
                },
                VenueConfig {
                    venue_id: "CURVE".to_string(),
                    venue_type: VenueType::DEX,
                    api_endpoint: "https://api.curve.fi".to_string(),
                    symbols: vec!["USDC/USDT".to_string(), "DAI/USDC".to_string()],
                    fee_bps: 4,
                    latency_ms: 300,
                    enabled: true,
                    weight: 0.7,
                },
            ],
            synthetic_pools: vec![
                SyntheticPoolConfig {
                    pool_id: "SYNTH_FX_MAJORS".to_string(),
                    base_assets: vec!["EUR/USD".to_string(), "GBP/USD".to_string(), "USD/JPY".to_string()],
                    quote_asset: "USD".to_string(),
                    correlation_threshold: 0.85,
                    min_participants: 3,
                    max_slippage_bps: 5,
                },
                SyntheticPoolConfig {
                    pool_id: "SYNTH_CRYPTO_MAJORS".to_string(),
                    base_assets: vec!["BTC/USD".to_string(), "ETH/USD".to_string()],
                    quote_asset: "USD".to_string(),
                    correlation_threshold: 0.75,
                    min_participants: 2,
                    max_slippage_bps: 8,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueConfig {
    pub venue_id: String,
    pub venue_type: VenueType,
    pub api_endpoint: String,
    pub symbols: Vec<String>,
    pub fee_bps: u32,
    pub latency_ms: u32,
    pub enabled: bool,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VenueType {
    CEX,
    DEX,
    OTC,
    Internal,
    Synthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticPoolConfig {
    pub pool_id: String,
    pub base_assets: Vec<String>,
    pub quote_asset: String,
    pub correlation_threshold: f64,
    pub min_participants: u32,
    pub max_slippage_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquiditySnapshot {
    pub symbol: String,
    pub timestamp: u64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub total_bid_volume: f64,
    pub total_ask_volume: f64,
    pub spread_bps: f64,
    pub mid_price: f64,
    pub liquidity_score: f64,
    pub venue_contributions: HashMap<String, VenueContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub volume: f64,
    pub venue: String,
    pub order_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueContribution {
    pub venue_id: String,
    pub bid_volume: f64,
    pub ask_volume: f64,
    pub bid_depth_usd: f64,
    pub ask_depth_usd: f64,
    pub weighted_spread_bps: f64,
    pub latency_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedOrderBook {
    pub symbol: String,
    pub timestamp: u64,
    pub bids: Vec<AggregatedLevel>,
    pub asks: Vec<AggregatedLevel>,
    pub best_bid: f64,
    pub best_ask: f64,
    pub spread_bps: f64,
    pub depth_10_bps_usd: f64,
    pub depth_50_bps_usd: f64,
    pub depth_100_bps_usd: f64,
    pub liquidity_score: f64,
    pub venue_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedLevel {
    pub price: f64,
    pub total_volume: f64,
    pub venues: Vec<VenueLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueLevel {
    pub venue_id: String,
    pub volume: f64,
    pub order_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityIncentive {
    pub participant_id: String,
    pub symbol: String,
    pub incentive_bps: u32,
    pub volume_target_usd: f64,
    pub achieved_volume_usd: f64,
    pub reward_earned_usd: f64,
    pub period_start: u64,
    pub period_end: u64,
    pub status: IncentiveStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IncentiveStatus {
    Active,
    Completed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketMakerProfile {
    pub participant_id: String,
    pub symbols: Vec<String>,
    pub min_spread_bps: u32,
    pub max_position_usd: f64,
    pub quote_size_usd: f64,
    pub uptime_requirement_pct: f64,
    pub rebate_tier: RebateTier,
    pub performance_score: f64,
    pub total_volume_usd: f64,
    pub total_rebates_usd: f64,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RebateTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
}

impl RebateTier {
    pub fn rebate_bps(&self) -> u32 {
        match self {
            RebateTier::Bronze => 1,
            RebateTier::Silver => 2,
            RebateTier::Gold => 3,
            RebateTier::Platinum => 4,
            RebateTier::Diamond => 5,
        }
    }
}

pub struct LiquidityEngine {
    config: LiquidityConfig,
    snapshots: Arc<RwLock<HashMap<String, LiquiditySnapshot>>>,
    aggregated_books: Arc<RwLock<HashMap<String, AggregatedOrderBook>>>,
    market_makers: Arc<RwLock<HashMap<String, MarketMakerProfile>>>,
    incentives: Arc<RwLock<HashMap<String, LiquidityIncentive>>>,
    venue_connections: Arc<RwLock<HashMap<String, VenueConnection>>>,
    synthetic_pools: Arc<RwLock<HashMap<String, SyntheticPool>>>,
    metrics: Arc<RwLock<LiquidityMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueConnection {
    pub venue_id: String,
    pub connected: bool,
    pub last_ping: u64,
    pub messages_received: u64,
    pub errors: u32,
    pub subscriptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticPool {
    pub pool_id: String,
    pub config: SyntheticPoolConfig,
    pub participants: Vec<String>,
    pub composite_price: f64,
    pub composite_volume: f64,
    pub correlation_matrix: HashMap<String, HashMap<String, f64>>,
    pub last_updated: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiquidityMetrics {
    pub total_symbols_tracked: u32,
    pub active_venues: u32,
    pub total_bid_depth_usd: f64,
    pub total_ask_depth_usd: f64,
    pub avg_spread_bps: f64,
    pub avg_liquidity_score: f64,
    pub synthetic_pools_active: u32,
    pub market_makers_active: u32,
    pub incentives_active: u32,
    pub total_incentive_volume_usd: f64,
    pub cross_venue_volume_usd: f64,
    pub internalization_rate_pct: f64,
}

impl LiquidityEngine {
    pub fn new(config: LiquidityConfig) -> Self {
        let mut synthetic_pools = HashMap::new();
        for pool_config in &config.synthetic_pools {
            synthetic_pools.insert(pool_config.pool_id.clone(), SyntheticPool {
                pool_id: pool_config.pool_id.clone(),
                config: pool_config.clone(),
                participants: vec![],
                composite_price: 0.0,
                composite_volume: 0.0,
                correlation_matrix: HashMap::new(),
                last_updated: 0,
                active: false,
            });
        }

        Self {
            config,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            aggregated_books: Arc::new(RwLock::new(HashMap::new())),
            market_makers: Arc::new(RwLock::new(HashMap::new())),
            incentives: Arc::new(RwLock::new(HashMap::new())),
            venue_connections: Arc::new(RwLock::new(HashMap::new())),
            synthetic_pools: Arc::new(RwLock::new(synthetic_pools)),
            metrics: Arc::new(RwLock::new(LiquidityMetrics::default())),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        if self.config.enable_cross_venue {
            self.connect_venues().await?;
        }
        if self.config.enable_synthetic {
            self.initialize_synthetic_pools().await?;
        }
        self.start_aggregation_loop().await;
        self.start_incentive_loop().await;
        self.start_metrics_loop().await;
        info!("Liquidity engine started");
        Ok(())
    }

    async fn connect_venues(&self) -> Result<(), String> {
        for venue in &self.config.venues {
            if venue.enabled {
                let conn = VenueConnection {
                    venue_id: venue.venue_id.clone(),
                    connected: true,
                    last_ping: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                    messages_received: 0,
                    errors: 0,
                    subscriptions: venue.symbols.clone(),
                };
                self.venue_connections.write().await.insert(venue.venue_id.clone(), conn);
                info!("Connected to venue: {}", venue.venue_id);
            }
        }
        Ok(())
    }

    async fn initialize_synthetic_pools(&self) -> Result<(), String> {
        let mut pools = self.synthetic_pools.write().await;
        for pool in pools.values_mut() {
            pool.active = pool.config.base_assets.len() >= pool.config.min_participants as usize;
            pool.last_updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            info!("Initialized synthetic pool: {} (active: {})", pool.pool_id, pool.active);
        }
        Ok(())
    }

    async fn start_aggregation_loop(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(engine.config.depth_refresh_ms));
            loop {
                interval.tick().await;
                if let Err(e) = engine.aggregate_orderbooks().await {
                    warn!("Orderbook aggregation error: {}", e);
                }
                if let Err(e) = engine.update_synthetic_pools().await {
                    warn!("Synthetic pool update error: {}", e);
                }
            }
        });
    }

    async fn start_incentive_loop(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            loop {
                interval.tick().await;
                engine.calculate_incentives().await;
            }
        });
    }

    async fn start_metrics_loop(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                engine.update_metrics().await;
            }
        });
    }

    pub async fn update_venue_snapshot(&self, venue_id: &str, symbol: &str, bids: Vec<PriceLevel>, asks: Vec<PriceLevel>) {
        let mut snapshots = self.snapshots.write().await;
        let snapshot = LiquiditySnapshot {
            symbol: symbol.to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            total_bid_volume: bids.iter().map(|b| b.volume).sum(),
            total_ask_volume: asks.iter().map(|a| a.volume).sum(),
            spread_bps: if !bids.is_empty() && !asks.is_empty() {
                (asks[0].price - bids[0].price) / bids[0].price * 10000.0
            } else { 0.0 },
            mid_price: if !bids.is_empty() && !asks.is_empty() {
                (bids[0].price + asks[0].price) / 2.0
            } else { 0.0 },
            liquidity_score: self.calculate_liquidity_score(&bids, &asks),
            venue_contributions: HashMap::new(),
            bids,
            asks,
        };
        snapshots.insert(format!("{}:{}", venue_id, symbol), snapshot);
    }

    fn calculate_liquidity_score(&self, bids: &[PriceLevel], asks: &[PriceLevel]) -> f64 {
        if bids.is_empty() || asks.is_empty() { return 0.0; }
        
        let bid_depth: f64 = bids.iter().take(10).map(|b| b.volume * b.price).sum();
        let ask_depth: f64 = asks.iter().take(10).map(|a| a.volume * a.price).sum();
        let spread = (asks[0].price - bids[0].price) / bids[0].price;
        
        let depth_score = (bid_depth + ask_depth).ln().max(0.0) / 20.0;
        let spread_score = (1.0 - spread * 1000.0).max(0.0).min(1.0);
        let balance_score = 1.0 - (bid_depth - ask_depth).abs() / (bid_depth + ask_depth).max(1.0);
        
        (depth_score * 0.4 + spread_score * 0.4 + balance_score * 0.2).min(1.0)
    }

    async fn aggregate_orderbooks(&self) -> Result<(), String> {
        let snapshots = self.snapshots.read().await;
        let mut aggregated = HashMap::new();
        
        for snapshot in snapshots.values() {
            let symbol = &snapshot.symbol;
            
            let mut book = aggregated.entry(symbol.clone()).or_insert(AggregatedOrderBook {
                symbol: symbol.clone(),
                timestamp: snapshot.timestamp,
                bids: vec![],
                asks: vec![],
                best_bid: 0.0,
                best_ask: 0.0,
                spread_bps: 0.0,
                depth_10_bps_usd: 0.0,
                depth_50_bps_usd: 0.0,
                depth_100_bps_usd: 0.0,
                liquidity_score: 0.0,
                venue_count: 0,
            });
            
            book.venue_count += 1;
            book.liquidity_score = (book.liquidity_score + snapshot.liquidity_score) / 2.0;
            
            for bid in &snapshot.bids {
                if let Some(level) = book.bids.iter_mut().find(|l| (l.price - bid.price).abs() < 0.0001) {
                    level.total_volume += bid.volume;
                    level.venues.push(VenueLevel {
                        venue_id: bid.venue.clone(),
                        volume: bid.volume,
                        order_count: bid.order_count,
                    });
                } else {
                    book.bids.push(AggregatedLevel {
                        price: bid.price,
                        total_volume: bid.volume,
                        venues: vec![VenueLevel {
                            venue_id: bid.venue.clone(),
                            volume: bid.volume,
                            order_count: bid.order_count,
                        }],
                    });
                }
            }
            
            for ask in &snapshot.asks {
                if let Some(level) = book.asks.iter_mut().find(|l| (l.price - ask.price).abs() < 0.0001) {
                    level.total_volume += ask.volume;
                    level.venues.push(VenueLevel {
                        venue_id: ask.venue.clone(),
                        volume: ask.volume,
                        order_count: ask.order_count,
                    });
                } else {
                    book.asks.push(AggregatedLevel {
                        price: ask.price,
                        total_volume: ask.volume,
                        venues: vec![VenueLevel {
                            venue_id: ask.venue.clone(),
                            volume: ask.volume,
                            order_count: ask.order_count,
                        }],
                    });
                }
            }
        }
        
        for book in aggregated.values_mut() {
            book.bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
            book.asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
            
            book.best_bid = book.bids.first().map(|l| l.price).unwrap_or(0.0);
            book.best_ask = book.asks.first().map(|l| l.price).unwrap_or(0.0);
            book.spread_bps = if book.best_bid > 0.0 {
                (book.best_ask - book.best_bid) / book.best_bid * 10000.0
            } else { 0.0 };
            
            let mid = (book.best_bid + book.best_ask) / 2.0;
            book.depth_10_bps_usd = book.bids.iter()
                .filter(|l| (mid - l.price) / mid <= 0.001)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>()
                + book.asks.iter()
                .filter(|l| (l.price - mid) / mid <= 0.001)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>();
            
            book.depth_50_bps_usd = book.bids.iter()
                .filter(|l| (mid - l.price) / mid <= 0.005)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>()
                + book.asks.iter()
                .filter(|l| (l.price - mid) / mid <= 0.005)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>();
            
            book.depth_100_bps_usd = book.bids.iter()
                .filter(|l| (mid - l.price) / mid <= 0.01)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>()
                + book.asks.iter()
                .filter(|l| (l.price - mid) / mid <= 0.01)
                .map(|l| l.total_volume * l.price)
                .sum::<f64>();
        }
        
        self.aggregated_books.write().await.extend(aggregated);
        Ok(())
    }

    async fn update_synthetic_pools(&self) -> Result<(), String> {
        let mut pools = self.synthetic_pools.write().await;
        let books = self.aggregated_books.read().await;
        
        for pool in pools.values_mut() {
            if !pool.active { continue; }
            
            let mut prices = Vec::new();
            let mut volumes = Vec::new();
            
            for asset in &pool.config.base_assets {
                if let Some(book) = books.get(asset) {
                    prices.push(book.mid_price());
                    volumes.push(book.depth_10_bps_usd);
                }
            }
            
            if prices.len() >= pool.config.min_participants as usize {
                let total_vol: f64 = volumes.iter().sum();
                pool.composite_price = prices.iter().zip(volumes.iter())
                    .map(|(p, v)| p * v)
                    .sum::<f64>() / total_vol.max(1.0);
                pool.composite_volume = total_vol;
                pool.last_updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                
                self.update_correlation_matrix(&mut pool.correlation_matrix, &prices).await;
            }
        }
        Ok(())
    }

    async fn update_correlation_matrix(&self, matrix: &mut HashMap<String, HashMap<String, f64>>, prices: &[f64]) {
        for i in 0..prices.len() {
            for j in i+1..prices.len() {
                let key_i = format!("asset_{}", i);
                let key_j = format!("asset_{}", j);
                let corr = self.calculate_correlation(&prices[i..=i], &prices[j..=j]);
                matrix.entry(key_i.clone()).or_default().insert(key_j.clone(), corr);
                matrix.entry(key_j).or_default().insert(key_i, corr);
            }
        }
    }

    fn calculate_correlation(&self, x: &[f64], y: &[f64]) -> f64 {
        if x.len() != y.len() || x.is_empty() { return 0.0; }
        let n = x.len() as f64;
        let sum_x: f64 = x.iter().sum();
        let sum_y: f64 = y.iter().sum();
        let sum_xy: f64 = x.iter().zip(y.iter()).map(|(a, b)| a * b).sum();
        let sum_x2: f64 = x.iter().map(|a| a * a).sum();
        let sum_y2: f64 = y.iter().map(|b| b * b).sum();
        
        let numerator = n * sum_xy - sum_x * sum_y;
        let denominator = ((n * sum_x2 - sum_x * sum_x) * (n * sum_y2 - sum_y * sum_y)).sqrt();
        if denominator == 0.0 { 0.0 } else { numerator / denominator }
    }

    async fn calculate_incentives(&self) {
        let mut incentives = self.incentives.write().await;
        let makers = self.market_makers.read().await;
        
        for (id, maker) in makers.iter() {
            if !maker.is_active { continue; }
            
            let incentive_key = format!("{}_{}", id, maker.symbols.join("_"));
            let incentive = incentives.entry(incentive_key.clone()).or_insert(LiquidityIncentive {
                participant_id: id.clone(),
                symbol: maker.symbols.first().cloned().unwrap_or_default(),
                incentive_bps: self.config.liquidity_incentive_bps,
                volume_target_usd: maker.quote_size_usd * 100.0,
                achieved_volume_usd: maker.total_volume_usd,
                reward_earned_usd: 0.0,
                period_start: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 86400,
                period_end: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 86400,
                status: IncentiveStatus::Active,
            });
            
            if incentive.achieved_volume_usd >= incentive.volume_target_usd {
                incentive.reward_earned_usd = incentive.volume_target_usd * incentive.incentive_bps as f64 / 10000.0;
                incentive.status = IncentiveStatus::Completed;
            }
        }
    }

    async fn update_metrics(&self) {
        let books = self.aggregated_books.read().await;
        let pools = self.synthetic_pools.read().await;
        let makers = self.market_makers.read().await;
        let incentives = self.incentives.read().await;
        let venues = self.venue_connections.read().await;
        
        let mut metrics = self.metrics.write().await;
        metrics.total_symbols_tracked = books.len() as u32;
        metrics.active_venues = venues.values().filter(|v| v.connected).count() as u32;
        
        let total_bid: f64 = books.values().map(|b| b.bids.iter().map(|l| l.total_volume * l.price).sum::<f64>()).sum();
        let total_ask: f64 = books.values().map(|b| b.asks.iter().map(|l| l.total_volume * l.price).sum::<f64>()).sum();
        metrics.total_bid_depth_usd = total_bid;
        metrics.total_ask_depth_usd = total_ask;
        
        metrics.avg_spread_bps = if !books.is_empty() {
            books.values().map(|b| b.spread_bps).sum::<f64>() / books.len() as f64
        } else { 0.0 };
        
        metrics.avg_liquidity_score = if !books.is_empty() {
            books.values().map(|b| b.liquidity_score).sum::<f64>() / books.len() as f64
        } else { 0.0 };
        
        metrics.synthetic_pools_active = pools.values().filter(|p| p.active).count() as u32;
        metrics.market_makers_active = makers.values().filter(|m| m.is_active).count() as u32;
        metrics.incentives_active = incentives.values().filter(|i| i.status == IncentiveStatus::Active).count() as u32;
        metrics.total_incentive_volume_usd = incentives.values().map(|i| i.achieved_volume_usd).sum();
    }

    pub async fn register_market_maker(&self, profile: MarketMakerProfile) -> Result<(), String> {
        let id = profile.participant_id.clone();
        self.market_makers.write().await.insert(id.clone(), profile);
        info!("Registered market maker: {}", id);
        Ok(())
    }

    pub async fn get_aggregated_book(&self, symbol: &str) -> Option<AggregatedOrderBook> {
        self.aggregated_books.read().await.get(symbol).cloned()
    }

    pub async fn get_best_execution(&self, symbol: &str, side: &str, size_usd: f64) -> Option<ExecutionPlan> {
        let books = self.aggregated_books.read().await;
        let book = books.get(symbol)?;
        
        let levels = if side == "buy" { &book.asks } else { &book.bids };
        let mut remaining = size_usd;
        let mut plan = ExecutionPlan {
            symbol: symbol.to_string(),
            side: side.to_string(),
            total_size_usd: size_usd,
            legs: vec![],
            weighted_avg_price: 0.0,
            expected_slippage_bps: 0.0,
            venues_used: vec![],
        };
        
        let mut total_cost = 0.0;
        let mut total_filled = 0.0;
        
        for level in levels {
            if remaining <= 0.0 { break; }
            let available = level.total_volume * level.price;
            let fill = remaining.min(available);
            
            let venue_id = level.venues.first().map(|v| v.venue_id.clone()).unwrap_or("UNKNOWN".to_string());
            
            plan.legs.push(ExecutionLeg {
                venue: venue_id.clone(),
                price: level.price,
                size_usd: fill,
                fee_bps: self.config.venues.iter().find(|v| v.venue_id == venue_id).map(|v| v.fee_bps).unwrap_or(10),
            });
            
            plan.venues_used.push(venue_id);
            total_cost += fill * level.price;
            total_filled += fill;
            remaining -= fill;
        }
        
        if total_filled > 0.0 {
            plan.weighted_avg_price = total_cost / total_filled;
            plan.expected_slippage_bps = (plan.weighted_avg_price - book.mid_price()) / book.mid_price() * 10000.0;
        }
        
        if total_filled >= size_usd * 0.99 {
            Some(plan)
        } else {
            None
        }
    }

    pub async fn get_metrics(&self) -> LiquidityMetrics {
        self.metrics.read().await.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub symbol: String,
    pub side: String,
    pub total_size_usd: f64,
    pub legs: Vec<ExecutionLeg>,
    pub weighted_avg_price: f64,
    pub expected_slippage_bps: f64,
    pub venues_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLeg {
    pub venue: String,
    pub price: f64,
    pub size_usd: f64,
    pub fee_bps: u32,
}

impl AggregatedOrderBook {
    pub fn mid_price(&self) -> f64 {
        (self.best_bid + self.best_ask) / 2.0
    }
}

impl Clone for LiquidityEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            snapshots: self.snapshots.clone(),
            aggregated_books: self.aggregated_books.clone(),
            market_makers: self.market_makers.clone(),
            incentives: self.incentives.clone(),
            venue_connections: self.venue_connections.clone(),
            synthetic_pools: self.synthetic_pools.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_liquidity_engine_basic() {
        let engine = LiquidityEngine::new(LiquidityConfig::default());
        engine.start().await.unwrap();
        
        let bids = vec![PriceLevel { price: 100.0, volume: 10.0, venue: "TEST".to_string(), order_count: 5 }];
        let asks = vec![PriceLevel { price: 100.1, volume: 10.0, venue: "TEST".to_string(), order_count: 5 }];
        
        engine.update_venue_snapshot("TEST", "BTC/USD", bids, asks).await;
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let book = engine.get_aggregated_book("BTC/USD").await;
        assert!(book.is_some());
        assert!(book.unwrap().liquidity_score > 0.0);
    }
}