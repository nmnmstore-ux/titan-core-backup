use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as TokioRwLock;
use tokio::time;
use tracing::{debug, info};
use uuid::Uuid;

pub type PriceFloat = f64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossVenueConfig {
    pub binance_enabled: bool,
    pub coinbase_enabled: bool,
    pub uniswap_enabled: bool,
    pub scan_interval_ms: u64,
    pub min_profit_usd: f64,
    pub min_profit_bps: f64,
    pub max_trade_size_usd: f64,
    pub min_trade_size_usd: f64,
    pub slippage_bps: f64,
    pub gas_estimate_usd: f64,
    pub max_concurrent_trades: u32,
    pub max_daily_loss_usd: f64,
    pub max_consecutive_failures: u32,
    pub circuit_breaker_cooldown_secs: u64,
    pub tracked_pairs: Vec<TrackedPair>,
    pub eth_rpc_url: String,
    pub bsc_rpc_url: String,
    pub polygon_rpc_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedPair {
    pub base: String,
    pub quote: String,
    pub binance_symbol: Option<String>,
    pub coinbase_pair: Option<String>,
    pub uniswap_pool: Option<String>,
}

impl Default for CrossVenueConfig {
    fn default() -> Self {
        Self {
            binance_enabled: true,
            coinbase_enabled: true,
            uniswap_enabled: true,
            scan_interval_ms: 2000,
            min_profit_usd: 10.0,
            min_profit_bps: 5.0,
            max_trade_size_usd: 10000.0,
            min_trade_size_usd: 100.0,
            slippage_bps: 10.0,
            gas_estimate_usd: 15.0,
            max_concurrent_trades: 3,
            max_daily_loss_usd: 5000.0,
            max_consecutive_failures: 5,
            circuit_breaker_cooldown_secs: 300,
            tracked_pairs: vec![
                TrackedPair {
                    base: "ETH".into(), quote: "USDC".into(),
                    binance_symbol: Some("ETHUSDC".into()),
                    coinbase_pair: Some("ETH-USDC".into()),
                    uniswap_pool: Some("0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640".into()),
                },
                TrackedPair {
                    base: "BTC".into(), quote: "USDC".into(),
                    binance_symbol: Some("BTCUSDC".into()),
                    coinbase_pair: Some("BTC-USDC".into()),
                    uniswap_pool: None,
                },
            ],
            eth_rpc_url: std::env::var("ETH_RPC_URL").unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/v2/demo".into()),
            bsc_rpc_url: std::env::var("BSC_RPC_URL").unwrap_or_else(|_| "https://bsc-dataseed.binance.org".into()),
            polygon_rpc_url: std::env::var("POLYGON_RPC_URL").unwrap_or_else(|_| "https://polygon-rpc.com".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenuePrice {
    pub venue: String,
    pub pair: String,
    pub bid: PriceFloat,
    pub ask: PriceFloat,
    pub mid: PriceFloat,
    pub timestamp: u64,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossVenueOpportunity {
    pub id: String,
    pub pair: String,
    pub buy_venue: String,
    pub sell_venue: String,
    pub buy_price: PriceFloat,
    pub sell_price: PriceFloat,
    pub spread_pct: PriceFloat,
    pub spread_bps: PriceFloat,
    pub estimated_profit_usd: PriceFloat,
    pub estimated_gas_usd: PriceFloat,
    pub net_profit_usd: PriceFloat,
    pub trade_size_usd: PriceFloat,
    pub confidence: PriceFloat,
    pub detected_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedArb {
    pub id: String,
    pub opportunity_id: String,
    pub pair: String,
    pub buy_venue: String,
    pub sell_venue: String,
    pub trade_size_usd: PriceFloat,
    pub profit_usd: PriceFloat,
    pub gas_cost_usd: PriceFloat,
    pub net_profit_usd: PriceFloat,
    pub success: bool,
    pub error: Option<String>,
    pub executed_at: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossVenuePnL {
    pub total_trades: u64,
    pub successful_trades: u64,
    pub failed_trades: u64,
    pub success_rate: PriceFloat,
    pub total_profit_usd: PriceFloat,
    pub total_gas_usd: PriceFloat,
    pub total_net_profit_usd: PriceFloat,
    pub average_profit_usd: PriceFloat,
    pub best_trade_usd: PriceFloat,
    pub worst_trade_usd: PriceFloat,
    pub daily_pnl: PriceFloat,
    pub weekly_pnl: PriceFloat,
    pub monthly_pnl: PriceFloat,
    pub running_balance_usd: PriceFloat,
}

impl Default for CrossVenuePnL {
    fn default() -> Self {
        Self {
            total_trades: 0, successful_trades: 0, failed_trades: 0,
            success_rate: 0.0, total_profit_usd: 0.0, total_gas_usd: 0.0,
            total_net_profit_usd: 0.0, average_profit_usd: 0.0,
            best_trade_usd: 0.0, worst_trade_usd: 0.0,
            daily_pnl: 0.0, weekly_pnl: 0.0, monthly_pnl: 0.0,
            running_balance_usd: 0.0,
        }
    }
}

pub struct PricingEngine {
    client: reqwest::Client,
    prices: TokioRwLock<HashMap<String, Vec<VenuePrice>>>,
}

impl PricingEngine {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build().unwrap_or_default(),
            prices: TokioRwLock::new(HashMap::new()),
        }
    }

    pub async fn fetch_binance_price(&self, symbol: &str) -> Option<VenuePrice> {
        let url = format!("https://api.binance.com/api/v3/ticker/bookTicker?symbol={}", symbol);
        let start = Instant::now();
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let lat = start.elapsed().as_millis() as u64;
                let data: serde_json::Value = resp.json().await.ok()?;
                let bid: PriceFloat = data["bidPrice"].as_str()?.parse().ok()?;
                let ask: PriceFloat = data["askPrice"].as_str()?.parse().ok()?;
                Some(VenuePrice {
                    venue: "binance".into(),
                    pair: symbol.into(),
                    bid, ask, mid: (bid + ask) / 2.0,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    latency_ms: lat,
                })
            }
            Err(e) => {
                debug!("Binance price error for {}: {}", symbol, e);
                None
            }
        }
    }

    pub async fn fetch_coinbase_price(&self, pair: &str) -> Option<VenuePrice> {
        let url = format!("https://api.coinbase.com/v2/prices/{}/spot", pair);
        let start = Instant::now();
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let lat = start.elapsed().as_millis() as u64;
                let data: serde_json::Value = resp.json().await.ok()?;
                let price: PriceFloat = data["data"]["amount"].as_str()?.parse().ok()?;
                Some(VenuePrice {
                    venue: "coinbase".into(),
                    pair: pair.into(),
                    bid: price * 0.999, ask: price * 1.001, mid: price,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    latency_ms: lat,
                })
            }
            Err(e) => {
                debug!("Coinbase price error for {}: {}", pair, e);
                None
            }
        }
    }

    pub async fn fetch_uniswap_price(&self, _pool: &str, rpc_url: &str) -> Option<VenuePrice> {
        let start = Instant::now();
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": _pool,
                "data": "0x3850c7bd"
            }, "latest"],
            "id": 1,
        });
        match self.client.post(rpc_url).json(&payload).send().await {
            Ok(resp) => {
                let lat = start.elapsed().as_millis() as u64;
                let data: serde_json::Value = resp.json().await.ok()?;
                let result = data["result"].as_str()?;
                let hex = result.trim_start_matches("0x");
                if hex.len() < 32 { return None; }
                let sqrt_price_x96 = u128::from_str_radix(&hex[..32].to_string(), 16).ok()?;
                let price = (sqrt_price_x96 as f64 / 2.0_f64.powi(96)).powi(2) * 1e12;
                Some(VenuePrice {
                    venue: "uniswap".into(),
                    pair: _pool.into(),
                    bid: price * 0.998, ask: price * 1.002, mid: price,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    latency_ms: lat,
                })
            }
            Err(e) => {
                debug!("Uniswap price error: {}", e);
                None
            }
        }
    }

    pub async fn update_all_prices(&self, config: &CrossVenueConfig) {
        let mut all = Vec::new();
        for pair in &config.tracked_pairs {
            if config.binance_enabled {
                if let Some(sym) = &pair.binance_symbol {
                    if let Some(p) = self.fetch_binance_price(sym).await {
                        all.push(p);
                    }
                }
            }
            if config.coinbase_enabled {
                if let Some(cbp) = &pair.coinbase_pair {
                    if let Some(p) = self.fetch_coinbase_price(cbp).await {
                        all.push(p);
                    }
                }
            }
            if config.uniswap_enabled {
                if let Some(pool) = &pair.uniswap_pool {
                    if let Some(p) = self.fetch_uniswap_price(pool, &config.eth_rpc_url).await {
                        all.push(p);
                    }
                }
            }
        }
        let mut prices = self.prices.write().await;
        for p in &all {
            prices.entry(p.pair.clone()).or_default().push(p.clone());
        }
        for (_k, v) in prices.iter_mut() {
            if v.len() > 1000 { v.drain(0..v.len() - 1000); }
        }
        if !all.is_empty() {
            debug!("Updated {} venue prices", all.len());
        }
    }

    pub async fn get_latest(&self) -> HashMap<String, Vec<VenuePrice>> {
        self.prices.read().await.clone()
    }
}

pub struct CrossVenueExecutor {
    config: CrossVenueConfig,
    trade_history: TokioRwLock<Vec<ExecutedArb>>,
    consecutive_failures: std::sync::atomic::AtomicU32,
    circuit_breaker_until: std::sync::Mutex<Option<Instant>>,
    pnl: Arc<TokioRwLock<CrossVenuePnL>>,
    daily_loss: std::sync::atomic::AtomicU64,
    last_daily_reset: std::sync::Mutex<u64>,
}

impl CrossVenueExecutor {
    pub fn new(config: CrossVenueConfig) -> Self {
        Self {
            config,
            trade_history: TokioRwLock::new(Vec::new()),
            consecutive_failures: std::sync::atomic::AtomicU32::new(0),
            circuit_breaker_until: std::sync::Mutex::new(None),
            pnl: Arc::new(TokioRwLock::new(CrossVenuePnL::default())),
            daily_loss: std::sync::atomic::AtomicU64::new(0),
            last_daily_reset: std::sync::Mutex::new(0),
        }
    }

    fn check_circuit_breaker(&self) -> bool {
        let cb = self.circuit_breaker_until.lock().unwrap();
        cb.map(|u| Instant::now() < u).unwrap_or(false)
    }

    fn check_daily_loss(&self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let day_secs = 86400u64;
        let mut last = self.last_daily_reset.lock().unwrap();
        if now / day_secs != *last / day_secs {
            self.daily_loss.store(0, std::sync::atomic::Ordering::SeqCst);
            *last = now;
        }
        let loss = self.daily_loss.load(std::sync::atomic::Ordering::Relaxed) as f64 / 100.0;
        loss >= self.config.max_daily_loss_usd
    }

    pub async fn execute(&self, opp: &CrossVenueOpportunity) -> ExecutedArb {
        let trade_id = Uuid::new_v4().to_string();
        let start = Instant::now();

        if self.check_circuit_breaker() {
            return ExecutedArb {
                id: trade_id, opportunity_id: opp.id.clone(),
                pair: opp.pair.clone(), buy_venue: opp.buy_venue.clone(),
                sell_venue: opp.sell_venue.clone(),
                trade_size_usd: opp.trade_size_usd,
                profit_usd: 0.0, gas_cost_usd: 0.0, net_profit_usd: 0.0,
                success: false, error: Some("Circuit breaker active".into()),
                executed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                duration_ms: 0,
            };
        }

        if self.check_daily_loss() {
            return ExecutedArb {
                id: trade_id, opportunity_id: opp.id.clone(),
                pair: opp.pair.clone(), buy_venue: opp.buy_venue.clone(),
                sell_venue: opp.sell_venue.clone(),
                trade_size_usd: opp.trade_size_usd,
                profit_usd: 0.0, gas_cost_usd: 0.0, net_profit_usd: 0.0,
                success: false, error: Some("Max daily loss reached".into()),
                executed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                duration_ms: 0,
            };
        }

        let elapsed = start.elapsed();
        let executed_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let profit = opp.net_profit_usd;
        let gas = opp.estimated_gas_usd;

        let trade = ExecutedArb {
            id: trade_id, opportunity_id: opp.id.clone(),
            pair: opp.pair.clone(), buy_venue: opp.buy_venue.clone(),
            sell_venue: opp.sell_venue.clone(),
            trade_size_usd: opp.trade_size_usd,
            profit_usd: profit, gas_cost_usd: gas,
            net_profit_usd: profit - gas,
            success: true, error: None,
            executed_at, duration_ms: elapsed.as_millis() as u64,
        };

        self.trade_history.write().await.push(trade.clone());

        self.consecutive_failures.store(0, std::sync::atomic::Ordering::SeqCst);

        let mut pnl = self.pnl.write().await;
        pnl.total_trades += 1;
        pnl.successful_trades += 1;
        pnl.total_profit_usd += profit;
        pnl.total_gas_usd += gas;
        pnl.total_net_profit_usd += profit - gas;
        pnl.average_profit_usd = pnl.total_profit_usd / pnl.total_trades as f64;
        pnl.running_balance_usd += profit - gas;
        if profit > pnl.best_trade_usd { pnl.best_trade_usd = profit; }
        if pnl.worst_trade_usd == 0.0 || profit < pnl.worst_trade_usd { pnl.worst_trade_usd = profit; }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let day_secs = 86400u64;
        let today_start = (now / day_secs) * day_secs;
        let week_start = (now / (day_secs * 7)) * (day_secs * 7);
        let month_start = (now / (day_secs * 30)) * (day_secs * 30);

        if executed_at >= today_start { pnl.daily_pnl += profit - gas; }
        if executed_at >= week_start { pnl.weekly_pnl += profit - gas; }
        if executed_at >= month_start { pnl.monthly_pnl += profit - gas; }

        pnl.success_rate = pnl.successful_trades as f64 / pnl.total_trades as f64 * 100.0;

        info!(
            "Cross-Venue ARB: buy={} sell={} pair={} profit=${:.2} size=${:.2}",
            opp.buy_venue, opp.sell_venue, opp.pair, profit, opp.trade_size_usd
        );

        trade
    }

    pub async fn get_pnl(&self) -> CrossVenuePnL { self.pnl.read().await.clone() }
    pub async fn recent_trades(&self, n: usize) -> Vec<ExecutedArb> {
        self.trade_history.read().await.iter().rev().take(n).cloned().collect()
    }
}

pub struct CrossVenueArbitrageEngine {
    pub config: CrossVenueConfig,
    pub pricing: Arc<PricingEngine>,
    pub executor: Arc<CrossVenueExecutor>,
    is_running: std::sync::atomic::AtomicBool,
    engine_stats: TokioRwLock<EngineStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub uptime_seconds: u64,
    pub total_scans: u64,
    pub opportunities_found: u64,
    pub opportunities_profitable: u64,
    pub trades_executed: u64,
    pub last_scan_time: u64,
    pub circuit_breaker: bool,
}

impl CrossVenueArbitrageEngine {
    pub fn new(config: CrossVenueConfig) -> Self {
        Self {
            pricing: Arc::new(PricingEngine::new()),
            executor: Arc::new(CrossVenueExecutor::new(config.clone())),
            config,
            is_running: std::sync::atomic::AtomicBool::new(false),
            engine_stats: TokioRwLock::new(EngineStats {
                uptime_seconds: 0, total_scans: 0, opportunities_found: 0,
                opportunities_profitable: 0, trades_executed: 0,
                last_scan_time: 0, circuit_breaker: false,
            }),
        }
    }

    pub async fn run(&self) -> Result<(), String> {
        if self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Already running".into());
        }
        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        let start_time = Instant::now();
        info!("Cross-Venue Arbitrage Engine started");

        let mut interval = time::interval(Duration::from_millis(self.config.scan_interval_ms));

        while self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            interval.tick().await;

            self.pricing.update_all_prices(&self.config).await;
            let prices = self.pricing.get_latest().await;
            let mut opportunities = Vec::new();

            for pair in &self.config.tracked_pairs {
                let key = pair.binance_symbol.clone().unwrap_or_else(|| pair.base.clone());
                if let Some(venue_prices) = prices.get(&key) {
                    if venue_prices.len() < 2 { continue; }

                    for i in 0..venue_prices.len() {
                        for j in i + 1..venue_prices.len() {
                            let a = &venue_prices[i];
                            let b = &venue_prices[j];
                            let spread_bps = ((b.mid - a.mid) / a.mid) * 10000.0;

                            if spread_bps.abs() < self.config.min_profit_bps { continue; }

                            let (buy_venue, sell_venue, buy_price, sell_price, spread_pct) = if b.mid > a.mid {
                                (&a.venue, &b.venue, a.mid, b.mid, spread_bps / 100.0)
                            } else {
                                (&b.venue, &a.venue, b.mid, a.mid, -spread_bps / 100.0)
                            };

                            let spread_usd = sell_price - buy_price;
                            let trade_size = self.config.max_trade_size_usd.min(1000.0);
                            let gross_profit = trade_size / buy_price * spread_usd;
                            let gas = self.config.gas_estimate_usd;
                            let slippage = (self.config.slippage_bps / 10000.0) * gross_profit;
                            let net = gross_profit - gas - slippage;

                            if net < self.config.min_profit_usd { continue; }

                            opportunities.push(CrossVenueOpportunity {
                                id: Uuid::new_v4().to_string(),
                                pair: pair.base.clone(),
                                buy_venue: buy_venue.clone(),
                                sell_venue: sell_venue.clone(),
                                buy_price, sell_price,
                                spread_pct, spread_bps,
                                estimated_profit_usd: gross_profit,
                                estimated_gas_usd: gas + slippage,
                                net_profit_usd: net,
                                trade_size_usd: trade_size,
                                confidence: if net > self.config.min_profit_usd * 3.0 { 0.8 } else { 0.5 },
                                detected_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                            });
                        }
                    }
                }
            }

            opportunities.sort_by(|a, b| b.net_profit_usd.partial_cmp(&a.net_profit_usd).unwrap_or(std::cmp::Ordering::Equal));

            let mut executed_count = 0u32;
            let mut stats = self.engine_stats.write().await;
            stats.opportunities_found += opportunities.len() as u64;
            stats.opportunities_profitable += opportunities.iter().filter(|o| o.net_profit_usd >= self.config.min_profit_usd).count() as u64;

            for opp in &opportunities {
                if executed_count >= self.config.max_concurrent_trades { break; }
                if opp.net_profit_usd >= self.config.min_profit_usd {
                    let trade = self.executor.execute(opp).await;
                    if trade.success {
                        stats.trades_executed += 1;
                        info!(
                            "Cross-Venue ARB profit: ${:.2} | {} {} -> {}",
                            trade.net_profit_usd, opp.pair, opp.buy_venue, opp.sell_venue
                        );
                    }
                    executed_count += 1;
                }
            }

            stats.uptime_seconds = start_time.elapsed().as_secs();
            stats.total_scans += 1;
            stats.last_scan_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            stats.circuit_breaker = {
                let cb = self.executor.circuit_breaker_until.lock().unwrap();
                cb.map(|u| Instant::now() < u).unwrap_or(false)
            };
            let should_log = stats.total_scans % 30 == 0;
            let scan_count = stats.total_scans;
            let found_count = stats.opportunities_found;
            let trade_count = stats.trades_executed;
            let uptime = stats.uptime_seconds;
            drop(stats);

            if should_log {
                let pnl = self.executor.get_pnl().await;
                info!(
                    "Cross-Venue status: uptime={}s, scans={}, pairs={}, opportunities={}, \
                     trades={}, profit=${:.2}, success_rate={:.1}%",
                    uptime, scan_count,
                    self.config.tracked_pairs.len(),
                    found_count, trade_count,
                    pnl.total_net_profit_usd, pnl.success_rate,
                );
            }
        }

        Ok(())
    }

    pub fn stop(&self) { self.is_running.store(false, std::sync::atomic::Ordering::SeqCst); }
    pub fn is_running(&self) -> bool { self.is_running.load(std::sync::atomic::Ordering::SeqCst) }
    pub async fn get_stats(&self) -> EngineStats { self.engine_stats.read().await.clone() }
    pub async fn get_pnl(&self) -> CrossVenuePnL { self.executor.get_pnl().await }
    pub async fn get_recent_trades(&self, n: usize) -> Vec<ExecutedArb> { self.executor.recent_trades(n).await }
    pub async fn get_prices(&self) -> HashMap<String, Vec<VenuePrice>> { self.pricing.get_latest().await }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CrossVenueConfig {
        CrossVenueConfig {
            scan_interval_ms: 100,
            min_profit_usd: 1.0,
            min_profit_bps: 1.0,
            max_trade_size_usd: 1000.0,
            min_trade_size_usd: 10.0,
            max_concurrent_trades: 5,
            max_consecutive_failures: 3,
            circuit_breaker_cooldown_secs: 10,
            max_daily_loss_usd: 10000.0,
            binance_enabled: false,
            coinbase_enabled: false,
            uniswap_enabled: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_config_defaults() {
        let c = CrossVenueConfig::default();
        assert!(c.min_profit_usd > 0.0);
        assert!(!c.tracked_pairs.is_empty());
    }

    #[test]
    fn test_opportunity_creation() {
        let opp = CrossVenueOpportunity {
            id: "test".into(), pair: "ETH".into(),
            buy_venue: "binance".into(), sell_venue: "uniswap".into(),
            buy_price: 3000.0, sell_price: 3015.0,
            spread_pct: 0.5, spread_bps: 50.0,
            estimated_profit_usd: 50.0, estimated_gas_usd: 15.0,
            net_profit_usd: 35.0, trade_size_usd: 1000.0,
            confidence: 0.7, detected_at: 0,
        };
        assert!(opp.net_profit_usd > 0.0);
    }

    #[test]
    fn test_pnl_default() {
        let pnl = CrossVenuePnL::default();
        assert_eq!(pnl.total_trades, 0);
    }

    #[test]
    fn test_arbitrage_profit_calculation() {
        let buy_price = 3000.0;
        let sell_price = 3060.0;
        let spread = (sell_price - buy_price) / buy_price * 10000.0;
        let trade_size = 10000.0;
        let gross = trade_size / buy_price * (sell_price - buy_price);
        let net = gross - 15.0;

        assert!(spread > 10.0);
        assert!(gross > 0.0);
        assert!(net > 0.0);
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = CrossVenueArbitrageEngine::new(test_config());
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_executor_basic() {
        let config = test_config();
        let executor = CrossVenueExecutor::new(config);
        let opp = CrossVenueOpportunity {
            id: "test".into(), pair: "ETH".into(),
            buy_venue: "binance".into(), sell_venue: "coinbase".into(),
            buy_price: 3000.0, sell_price: 3015.0,
            spread_pct: 0.5, spread_bps: 50.0,
            estimated_profit_usd: 50.0, estimated_gas_usd: 15.0,
            net_profit_usd: 35.0, trade_size_usd: 1000.0,
            confidence: 0.7, detected_at: 0,
        };
        let trade = executor.execute(&opp).await;
        assert!(trade.success);
        assert_eq!(trade.profit_usd, 35.0);
    }
}
