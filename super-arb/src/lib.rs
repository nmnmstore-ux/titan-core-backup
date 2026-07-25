use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as TokioRwLock;
use tokio::time;
use tracing::{info, warn};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

pub type Price = f64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperConfig {
    pub eth_rpc_url: String, pub bsc_rpc_url: String, pub polygon_rpc_url: String,
    pub flashbots_auth_key: String, pub binance_api_key: String, pub coinbase_api_key: String,
    pub scan_interval_ms: u64, pub min_profit_usd: f64, pub max_trade_size_usd: f64,
    pub max_concurrent: u32, pub max_daily_loss_usd: f64, pub max_consecutive_failures: u32,
    pub circuit_breaker_cooldown_secs: u64, pub slippage_bps: f64, pub gas_estimate_usd: f64,
    pub flash_loan_enabled: bool, pub cross_venue_enabled: bool, pub mev_enabled: bool,
    pub jit_liquidity_enabled: bool, pub staking_arb_enabled: bool, pub funding_rate_enabled: bool,
    pub bridge_arb_enabled: bool, pub statistical_arb_enabled: bool,
}

impl Default for SuperConfig {
    fn default() -> Self {
        Self {
            eth_rpc_url: std::env::var("ETH_RPC_URL").unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/v2/demo".into()),
            bsc_rpc_url: std::env::var("BSC_RPC_URL").unwrap_or_else(|_| "https://bsc-dataseed.binance.org".into()),
            polygon_rpc_url: std::env::var("POLYGON_RPC_URL").unwrap_or_else(|_| "https://polygon-rpc.com".into()),
            flashbots_auth_key: std::env::var("FLASHBOTS_AUTH_KEY").unwrap_or_default(),
            binance_api_key: std::env::var("BINANCE_API_KEY").unwrap_or_default(),
            coinbase_api_key: std::env::var("COINBASE_API_KEY").unwrap_or_default(),
            scan_interval_ms: 1500, min_profit_usd: 10.0, max_trade_size_usd: 10000.0,
            max_concurrent: 5, max_daily_loss_usd: 10000.0, max_consecutive_failures: 5,
            circuit_breaker_cooldown_secs: 60, slippage_bps: 5.0, gas_estimate_usd: 20.0,
            flash_loan_enabled: true, cross_venue_enabled: true, mev_enabled: true,
            jit_liquidity_enabled: true, staking_arb_enabled: true, funding_rate_enabled: true,
            bridge_arb_enabled: true, statistical_arb_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenuePrice {
    pub venue: String, pub pair: String, pub bid: Price, pub ask: Price,
    pub mid: Price, pub timestamp: u64, pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: String, pub strategy: String, pub pair: String,
    pub buy_venue: String, pub sell_venue: String,
    pub gross_profit_usd: Price, pub estimated_cost_usd: Price,
    pub net_profit_usd: Price, pub confidence: Price,
    pub trade_size_usd: Price, pub details: String,
    pub detected_at: u64, pub requires_flash_loan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedTrade {
    pub id: String, pub strategy: String, pub pair: String,
    pub profit_usd: Price, pub cost_usd: Price, pub net_profit_usd: Price,
    pub success: bool, pub error: Option<String>, pub executed_at: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperPnL {
    pub total_trades: u64, pub successful_trades: u64, pub failed_trades: u64,
    pub success_rate: Price, pub total_profit_usd: Price, pub total_cost_usd: Price,
    pub total_net_profit_usd: Price, pub daily_pnl: Price, pub weekly_pnl: Price,
    pub monthly_pnl: Price, pub running_balance_usd: Price,
    pub per_strategy: HashMap<String, StrategyPnL>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPnL {
    pub trades: u64, pub successes: u64, pub total_profit: Price,
}

// ═══════════════════════════════════════════════════════════════════════════
// Pricing Hub
// ═══════════════════════════════════════════════════════════════════════════

pub struct PricingHub {
    config: SuperConfig, client: reqwest::Client,
    prices: TokioRwLock<HashMap<String, Vec<VenuePrice>>>,
}

impl PricingHub {
    pub fn new(config: SuperConfig) -> Self {
        Self {
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap_or_default(),
            prices: TokioRwLock::new(HashMap::new()), config,
        }
    }

    pub async fn update_all(&self) {
        let mut tasks = Vec::new();

        // Binance
        for pair in &["ETHUSDC", "BTCUSDC", "BNBUSDC", "MATICUSDC"] {
            let url = format!("https://api.binance.com/api/v3/ticker/bookTicker?symbol={}", pair);
            let c = self.client.clone();
            tasks.push(tokio::spawn(async move {
                let start = Instant::now();
                if let Ok(r) = c.get(&url).send().await {
                    if let Ok(d) = r.json::<serde_json::Value>().await {
                        let bid: Price = d["bidPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let ask: Price = d["askPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        return Some(VenuePrice {
                            venue: "binance".into(), pair: pair.to_string(),
                            bid, ask, mid: (bid + ask) / 2.0,
                            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                            latency_ms: start.elapsed().as_millis() as u64,
                        });
                    }
                }
                None::<VenuePrice>
            }));
        }

        // Coinbase
        for pair in &["ETH-USDC", "BTC-USDC"] {
            let url = format!("https://api.coinbase.com/v2/prices/{}/spot", pair);
            let c = self.client.clone();
            tasks.push(tokio::spawn(async move {
                let start = Instant::now();
                if let Ok(r) = c.get(&url).send().await {
                    if let Ok(d) = r.json::<serde_json::Value>().await {
                        let mid: Price = d["data"]["amount"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        return Some(VenuePrice {
                            venue: "coinbase".into(), pair: pair.to_string(),
                            bid: mid * 0.999, ask: mid * 1.001, mid,
                            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                            latency_ms: start.elapsed().as_millis() as u64,
                        });
                    }
                }
                None::<VenuePrice>
            }));
        }

        // Uniswap V3 price via slot0
        let rpc_url = self.config.eth_rpc_url.clone();
        let pools = [
            ("ETH-USDC", "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640", rpc_url.as_str()),
        ];
        for &(pair, addr, rpc) in &pools {
            let pair = pair.to_string();
            let addr = addr.to_string();
            let rpc = rpc.to_string();
            let c = self.client.clone();
            tasks.push(tokio::spawn(async move {
                let start = Instant::now();
                let payload = serde_json::json!([{
                    "jsonrpc": "2.0", "method": "eth_call",
                    "params": [{"to": &addr, "data": "0x3850c7bd"}, "latest"], "id": 1,
                }]);
                if let Ok(r) = c.post(&rpc).json(&payload).send().await {
                    if let Ok(data) = r.json::<serde_json::Value>().await {
                        if let Some(hex) = data[0]["result"].as_str() {
                            if let Ok(b) = hex::decode(hex.trim_start_matches("0x")) {
                                if b.len() >= 16 {
                                    let mut arr = [0u8; 16]; arr.copy_from_slice(&b[..16]);
                                    let sqrt = u128::from_be_bytes(arr);
                                    let mid = (sqrt as Price / 2.0_f64.powi(96)).powi(2) * 1e12;
                                    return Some(VenuePrice {
                                        venue: "uniswap".into(), pair,
                                        bid: mid * 0.998, ask: mid * 1.002, mid,
                                        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                                        latency_ms: start.elapsed().as_millis() as u64,
                                    });
                                }
                            }
                        }
                    }
                }
                None::<VenuePrice>
            }));
        }

        // Staking rates
        let rpc_url = self.config.eth_rpc_url.clone();
        let pools_staking = [
            ("stETH-ETH", "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84", rpc_url.as_str()),
            ("rETH-ETH", "0xae78736Cd615f374D3085123A210448E74Fc6393", rpc_url.as_str()),
        ];
        for &(pair, addr, rpc) in &pools_staking {
            let pair = pair.to_string();
            let addr = addr.to_string();
            let rpc = rpc.to_string();
            let c = self.client.clone();
            tasks.push(tokio::spawn(async move {
                let start = Instant::now();
                let payload = serde_json::json!([{
                    "jsonrpc": "2.0", "method": "eth_call",
                    "params": [{"to": &addr, "data": "0x18160ddd"}, "latest"], "id": 1,
                }]);
                if let Ok(r) = c.post(&rpc).json(&payload).send().await {
                    if let Ok(data) = r.json::<serde_json::Value>().await {
                        if let Some(hex) = data[0]["result"].as_str() {
                            if let Ok(b) = hex::decode(hex.trim_start_matches("0x")) {
                                if b.len() >= 16 {
                                    let mut arr = [0u8; 16]; arr.copy_from_slice(&b[..16]);
                                    let total = u128::from_be_bytes(arr) as Price;
                                    let rate = total / 1e18;
                                    return Some(VenuePrice {
                                        venue: "staking".into(), pair,
                                        bid: rate * 0.999, ask: rate * 1.001, mid: rate,
                                        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                                        latency_ms: start.elapsed().as_millis() as u64,
                                    });
                                }
                            }
                        }
                    }
                }
                None::<VenuePrice>
            }));
        }

        let results: Vec<VenuePrice> = futures_util::future::join_all(tasks).await
            .into_iter().filter_map(|r| r.ok()).flatten().collect();

        let mut prices = self.prices.write().await;
        for p in &results {
            prices.entry(p.venue.clone() + "/" + &p.pair).or_default().push(p.clone());
        }
    }

    pub async fn get_latest_prices(&self) -> HashMap<String, Vec<VenuePrice>> {
        self.prices.read().await.clone()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Strategy Engine
// ═══════════════════════════════════════════════════════════════════════════

pub struct StrategyEngine {
    config: SuperConfig, prices: Arc<PricingHub>,
    detected: TokioRwLock<VecDeque<Opportunity>>,
    stats: TokioRwLock<EngineStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub total_opportunities: u64, pub profitable: u64,
    pub flash_loan_ops: u64, pub cross_venue_ops: u64,
    pub mev_ops: u64, pub jit_ops: u64, pub staking_ops: u64,
    pub funding_ops: u64, pub bridge_ops: u64, pub statistical_ops: u64,
}

impl StrategyEngine {
    pub fn new(config: SuperConfig, prices: Arc<PricingHub>) -> Self {
        Self { config, prices, detected: TokioRwLock::new(VecDeque::new()), stats: TokioRwLock::new(EngineStats {
            total_opportunities: 0, profitable: 0, flash_loan_ops: 0, cross_venue_ops: 0,
            mev_ops: 0, jit_ops: 0, staking_ops: 0, funding_ops: 0, bridge_ops: 0, statistical_ops: 0,
        })}
    }

    pub async fn scan_all(&self) -> Vec<Opportunity> {
        let prices = self.prices.get_latest_prices().await;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut all = Vec::new();
        let mut stats = self.stats.write().await;

        // 1. Flash Loan + Cross-Venue opportunities
        if self.config.cross_venue_enabled {
            let _venues: Vec<&str> = prices.keys().filter_map(|k| k.split('/').next()).collect();
            let _pairs_seen: std::collections::HashSet<&str> = prices.keys().filter_map(|k| k.split('/').nth(1)).collect();
            for pair in &["ETHUSDC", "ETH-USDC", "BTCUSDC", "BTC-USDC"] {
                let mut pair_prices: Vec<(&str, &VenuePrice)> = prices.iter()
                    .filter(|(k, v)| k.contains(pair) && !v.is_empty())
                    .filter_map(|(k, v)| v.last().map(|p| (k.as_str(), p)))
                    .collect();
                pair_prices.sort_by(|a, b| a.1.mid.partial_cmp(&b.1.mid).unwrap_or(std::cmp::Ordering::Equal));
                if pair_prices.len() >= 2 {
                    let cheapest = pair_prices[0];
                    let most_expensive = pair_prices[pair_prices.len() - 1];
                    let spread_bps = (most_expensive.1.mid - cheapest.1.mid) / cheapest.1.mid * 10000.0;
                    if spread_bps >= 2.0 {
                        let size = self.config.max_trade_size_usd.min(2000.0);
                        let profit = size / cheapest.1.mid * (most_expensive.1.mid - cheapest.1.mid);
                        let cost = self.config.gas_estimate_usd;
                        if profit - cost > self.config.min_profit_usd {
                            all.push(Opportunity {
                                id: Uuid::new_v4().to_string(),
                                strategy: "cross-venue".into(), pair: pair.to_string(),
                                buy_venue: cheapest.1.venue.clone(),
                                sell_venue: most_expensive.1.venue.clone(),
                                gross_profit_usd: profit, estimated_cost_usd: cost,
                                net_profit_usd: profit - cost, confidence: 0.65,
                                trade_size_usd: size, details: format!("spread={:.1}bps", spread_bps),
                                detected_at: now, requires_flash_loan: false,
                            });
                            stats.cross_venue_ops += 1;
                        }
                    }
                }
            }
        }

        // 2. JIT Liquidity
        if self.config.jit_liquidity_enabled {
            for (key, vals) in &prices {
                if let Some(last) = vals.last() {
                    if last.venue == "uniswap" && last.latency_ms > 50 {
                        let profit = self.config.max_trade_size_usd.min(5000.0) * 0.0003;
                        let cost = self.config.gas_estimate_usd * 0.5;
                        if profit - cost > self.config.min_profit_usd * 0.5 {
                            all.push(Opportunity {
                                id: Uuid::new_v4().to_string(),
                                strategy: "jit-liquidity".into(),
                                pair: key.split('/').nth(1).unwrap_or("ETH").to_string(),
                                buy_venue: "uniswap".into(), sell_venue: "uniswap".into(),
                                gross_profit_usd: profit, estimated_cost_usd: cost,
                                net_profit_usd: profit - cost, confidence: 0.7,
                                trade_size_usd: profit * 1000.0, details: "JIT fee collection".into(),
                                detected_at: now, requires_flash_loan: true,
                            });
                            stats.jit_ops += 1;
                        }
                    }
                }
            }
        }

        // 3. Staking Arb (stETH/ETH)
        if self.config.staking_arb_enabled {
            for (key, vals) in &prices {
                if key.contains("stETH-ETH") || key.contains("rETH-ETH") {
                    if let Some(last) = vals.last() {
                        let deviation = (last.mid - 1.0).abs() * 10000.0;
                        if deviation > 5.0 {
                            let profit = self.config.max_trade_size_usd.min(3000.0) * (deviation / 10000.0);
                            let cost = self.config.gas_estimate_usd;
                            if profit - cost > self.config.min_profit_usd {
                                all.push(Opportunity {
                                    id: Uuid::new_v4().to_string(),
                                    strategy: "staking-arb".into(),
                                    pair: key.split('/').nth(1).unwrap_or("ETH").to_string(),
                                    buy_venue: "curve".into(), sell_venue: "lido".into(),
                                    gross_profit_usd: profit, estimated_cost_usd: cost,
                                    net_profit_usd: profit - cost, confidence: 0.72,
                                    trade_size_usd: profit * 10.0,
                                    details: format!("deviation={:.1}bps", deviation),
                                    detected_at: now, requires_flash_loan: true,
                                });
                                stats.staking_ops += 1;
                            }
                        }
                    }
                }
            }
        }

        // 4. Statistical Arb (ETH/BTC correlation)
        if self.config.statistical_arb_enabled {
            let eth_prices: Vec<Price> = prices.values()
                .filter(|v| !v.is_empty())
                .filter_map(|v| v.last())
                .filter(|p| p.pair.contains("ETH"))
                .map(|p| p.mid).collect();
            let btc_prices: Vec<Price> = prices.values()
                .filter(|v| !v.is_empty())
                .filter_map(|v| v.last())
                .filter(|p| p.pair.contains("BTC"))
                .map(|p| p.mid).collect();
            if !eth_prices.is_empty() && !btc_prices.is_empty() {
                let eth_avg: Price = eth_prices.iter().sum::<Price>() / eth_prices.len() as Price;
                let btc_avg: Price = btc_prices.iter().sum::<Price>() / btc_prices.len() as Price;
                let ratio = eth_avg / btc_avg;
                let historical_ratio = 0.0289;
                let deviation = (ratio - historical_ratio) / historical_ratio;
                if deviation.abs() > 0.02 {
                    let size = self.config.max_trade_size_usd.min(2000.0);
                    let expected_return = size * deviation.abs() * 0.3;
                    let cost = self.config.gas_estimate_usd * 2.0;
                    if expected_return - cost > self.config.min_profit_usd {
                        all.push(Opportunity {
                            id: Uuid::new_v4().to_string(),
                            strategy: "statistical-arb".into(),
                            pair: "ETH/BTC".into(),
                            buy_venue: "binance".into(), sell_venue: "coinbase".into(),
                            gross_profit_usd: expected_return, estimated_cost_usd: cost,
                            net_profit_usd: expected_return - cost, confidence: 0.45,
                            trade_size_usd: size, details: format!("deviation={:.1}%", deviation * 100.0),
                            detected_at: now, requires_flash_loan: false,
                        });
                        stats.statistical_ops += 1;
                    }
                }
            }
        }

        all.sort_by(|a, b| b.net_profit_usd.partial_cmp(&a.net_profit_usd).unwrap_or(std::cmp::Ordering::Equal));
        stats.total_opportunities += all.len() as u64;
        let profitable_count = all.iter().filter(|o| o.net_profit_usd >= self.config.min_profit_usd).count() as u64;
        stats.profitable = profitable_count;

        for opp in &all {
            self.detected.write().await.push_back(opp.clone());
        }
        while self.detected.read().await.len() > 500 {
            self.detected.write().await.pop_front();
        }

        all
    }

    pub async fn get_stats(&self) -> EngineStats { self.stats.read().await.clone() }
}

// ═══════════════════════════════════════════════════════════════════════════
// Execution Engine
// ═══════════════════════════════════════════════════════════════════════════

pub struct ExecutionEngine {
    config: SuperConfig,
    trade_history: TokioRwLock<Vec<ExecutedTrade>>,
    consecutive_failures: std::sync::atomic::AtomicU32,
    circuit_breaker_until: std::sync::Mutex<Option<Instant>>,
    daily_loss: std::sync::atomic::AtomicI64,
    last_daily_reset: std::sync::Mutex<u64>,
    pnl: Arc<TokioRwLock<SuperPnL>>,
}

impl ExecutionEngine {
    pub fn new(config: SuperConfig) -> Self {
        let mut per_strategy = HashMap::new();
        for s in &["flash-loan", "cross-venue", "mev", "jit-liquidity", "staking-arb", "funding-rate", "bridge-arb", "statistical-arb"] {
            per_strategy.insert(s.to_string(), StrategyPnL { trades: 0, successes: 0, total_profit: 0.0 });
        }
        Self {
            config,
            trade_history: TokioRwLock::new(Vec::new()),
            consecutive_failures: std::sync::atomic::AtomicU32::new(0),
            circuit_breaker_until: std::sync::Mutex::new(None),
            daily_loss: std::sync::atomic::AtomicI64::new(0),
            last_daily_reset: std::sync::Mutex::new(0),
            pnl: Arc::new(TokioRwLock::new(SuperPnL {
                total_trades: 0, successful_trades: 0, failed_trades: 0,
                success_rate: 0.0, total_profit_usd: 0.0, total_cost_usd: 0.0,
                total_net_profit_usd: 0.0, daily_pnl: 0.0, weekly_pnl: 0.0,
                monthly_pnl: 0.0, running_balance_usd: 0.0, per_strategy,
            })),
        }
    }

    fn is_circuit_breaker_active(&self) -> bool {
        self.circuit_breaker_until.lock().unwrap().map(|u| Instant::now() < u).unwrap_or(false)
    }

    pub async fn execute(&self, opp: &Opportunity) -> ExecutedTrade {
        let id = Uuid::new_v4().to_string();
        let start = Instant::now();
        if self.is_circuit_breaker_active() {
            return ExecutedTrade { id, strategy: opp.strategy.clone(), pair: opp.pair.clone(),
                profit_usd: 0.0, cost_usd: 0.0, net_profit_usd: 0.0,
                success: false, error: Some("Circuit breaker".into()),
                executed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                duration_ms: 0 };
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let day_secs = 86400u64;
        {
            let mut last = self.last_daily_reset.lock().unwrap();
            if now / day_secs != *last / day_secs {
                self.daily_loss.store(0, std::sync::atomic::Ordering::SeqCst);
                *last = now;
            }
        }
        let daily = self.daily_loss.load(std::sync::atomic::Ordering::Relaxed) as f64 / 100.0;
        if daily.abs() >= self.config.max_daily_loss_usd {
            return ExecutedTrade { id, strategy: opp.strategy.clone(), pair: opp.pair.clone(),
                profit_usd: 0.0, cost_usd: 0.0, net_profit_usd: 0.0,
                success: false, error: Some("Max daily loss".into()),
                executed_at: now, duration_ms: 0 };
        }

        // Paper execution
        let elapsed = start.elapsed();
        let executed_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let success = opp.net_profit_usd >= self.config.min_profit_usd;
        let profit = if success { opp.net_profit_usd } else { 0.0 };
        let cost = opp.estimated_cost_usd;

        let trade = ExecutedTrade {
            id, strategy: opp.strategy.clone(), pair: opp.pair.clone(),
            profit_usd: profit, cost_usd: cost, net_profit_usd: profit - cost,
            success, error: if success { None } else { Some("Below min profit".into()) },
            executed_at, duration_ms: elapsed.as_millis() as u64,
        };

        self.trade_history.write().await.push(trade.clone());

        if success {
            self.consecutive_failures.store(0, std::sync::atomic::Ordering::SeqCst);
            let mut pnl = self.pnl.write().await;
            pnl.total_trades += 1; pnl.successful_trades += 1;
            pnl.total_profit_usd += profit; pnl.total_cost_usd += cost;
            pnl.total_net_profit_usd += profit - cost;
            pnl.running_balance_usd += profit - cost;
            let w = 86400u64 * 7;
            let m = 86400u64 * 30;
            if executed_at / 86400u64 == now / 86400u64 { pnl.daily_pnl += profit - cost; }
            if executed_at / w == now / w { pnl.weekly_pnl += profit - cost; }
            if executed_at / m == now / m { pnl.monthly_pnl += profit - cost; }
            pnl.success_rate = pnl.successful_trades as f64 / pnl.total_trades.max(1) as f64 * 100.0;
            if let Some(spnl) = pnl.per_strategy.get_mut(&opp.strategy) {
                spnl.trades += 1; spnl.successes += 1; spnl.total_profit += profit;
            }
            info!("SUPER-ARB profit: ${:.2} | strategy={} | pair={} | net=${:.2}", profit, opp.strategy, opp.pair, profit - cost);
        } else {
            self.consecutive_failures.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let failures = self.consecutive_failures.load(std::sync::atomic::Ordering::SeqCst);
            if failures >= self.config.max_consecutive_failures {
                *self.circuit_breaker_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(self.config.circuit_breaker_cooldown_secs));
                warn!("Circuit breaker: {} failures", failures);
            }
            let mut pnl = self.pnl.write().await;
            pnl.total_trades += 1; pnl.failed_trades += 1;
            warn!("SUPER-ARB failed: strategy={} error={:?}", opp.strategy, trade.error);
        }

        trade
    }

    pub async fn get_pnl(&self) -> SuperPnL { self.pnl.read().await.clone() }
    pub async fn recent_trades(&self, n: usize) -> Vec<ExecutedTrade> {
        self.trade_history.read().await.iter().rev().take(n).cloned().collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SuperArbEngine — Main
// ═══════════════════════════════════════════════════════════════════════════

pub struct SuperArbEngine {
    pub config: SuperConfig,
    pub pricing: Arc<PricingHub>,
    pub strategies: Arc<StrategyEngine>,
    pub executor: Arc<ExecutionEngine>,
    is_running: std::sync::atomic::AtomicBool,
    engine_stats: TokioRwLock<EngineRuntimeStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRuntimeStats {
    pub uptime_seconds: u64, pub total_scans: u64,
    pub opportunities_count: usize, pub trades_count: usize,
    pub last_scan_time: u64, pub circuit_breaker: bool,
}

impl SuperArbEngine {
    pub fn new(config: SuperConfig) -> Self {
        let pricing = Arc::new(PricingHub::new(config.clone()));
        let strategies = Arc::new(StrategyEngine::new(config.clone(), pricing.clone()));
        let executor = Arc::new(ExecutionEngine::new(config.clone()));
        Self {
            config, pricing, strategies, executor,
            is_running: std::sync::atomic::AtomicBool::new(false),
            engine_stats: TokioRwLock::new(EngineRuntimeStats {
                uptime_seconds: 0, total_scans: 0, opportunities_count: 0,
                trades_count: 0, last_scan_time: 0, circuit_breaker: false,
            }),
        }
    }

    pub async fn run(&self) -> Result<(), String> {
        if self.is_running.load(std::sync::atomic::Ordering::SeqCst) { return Err("Already running".into()); }
        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);
        let start_time = Instant::now();
        info!("Super-Arb Engine started — all strategies active");

        let mut interval = time::interval(Duration::from_millis(self.config.scan_interval_ms));

        while self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            interval.tick().await;

            // Phase 1: Update prices
            self.pricing.update_all().await;

            // Phase 2: Scan all strategies
            let opportunities = self.strategies.scan_all().await;

            // Phase 3: Execute best opportunities
            let mut executed = 0u32;
            for opp in &opportunities {
                if executed >= self.config.max_concurrent { break; }
                if opp.net_profit_usd >= self.config.min_profit_usd {
                    let trade = self.executor.execute(opp).await;
                    if trade.success { executed += 1; }
                }
            }

            // Phase 4: Stats
            let total_scans;
            let uptime_seconds;
            {
                let mut stats = self.engine_stats.write().await;
                stats.uptime_seconds = start_time.elapsed().as_secs();
                stats.total_scans += 1;
                stats.opportunities_count = opportunities.len();
                stats.trades_count = self.executor.trade_history.read().await.len();
                stats.last_scan_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                stats.circuit_breaker = {
                    self.executor.circuit_breaker_until.lock().unwrap()
                        .map(|u| Instant::now() < u).unwrap_or(false)
                };
                total_scans = stats.total_scans;
                uptime_seconds = stats.uptime_seconds;
            }

            if total_scans % 15 == 0 {
                let pnl = self.executor.get_pnl().await;
                let s = self.strategies.get_stats().await;
                info!(
                    "SUPER-ARB: uptime={}s scans={} opps={} trades={} profit=${:.2} strategies=[flash={} cv={} mev={} jit={} staking={} stat={}]",
                    uptime_seconds, total_scans, s.total_opportunities,
                    pnl.total_trades, pnl.total_net_profit_usd,
                    s.flash_loan_ops, s.cross_venue_ops, s.mev_ops, s.jit_ops,
                    s.staking_ops, s.statistical_ops,
                );
            }
        }
        Ok(())
    }

    pub fn stop(&self) { self.is_running.store(false, std::sync::atomic::Ordering::SeqCst); }
    pub fn is_running(&self) -> bool { self.is_running.load(std::sync::atomic::Ordering::SeqCst) }
    pub async fn get_stats(&self) -> EngineRuntimeStats { self.engine_stats.read().await.clone() }
    pub async fn get_pnl(&self) -> SuperPnL { self.executor.get_pnl().await }
    pub async fn get_recent_trades(&self, n: usize) -> Vec<ExecutedTrade> { self.executor.recent_trades(n).await }
    pub async fn get_prices(&self) -> HashMap<String, Vec<VenuePrice>> { self.pricing.get_latest_prices().await }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SuperConfig {
        SuperConfig {
            scan_interval_ms: 100, min_profit_usd: 1.0, max_trade_size_usd: 1000.0,
            max_concurrent: 3, max_consecutive_failures: 3,
            circuit_breaker_cooldown_secs: 10, cross_venue_enabled: false,
            mev_enabled: false, flash_loan_enabled: false, jit_liquidity_enabled: false,
            staking_arb_enabled: false, funding_rate_enabled: false,
            bridge_arb_enabled: false, statistical_arb_enabled: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_config_defaults() { let c = SuperConfig::default(); assert!(c.min_profit_usd > 0.0); }

    #[test]
    fn test_opportunity_creation() {
        let o = Opportunity {
            id: "t".into(), strategy: "test".into(), pair: "ETH".into(),
            buy_venue: "a".into(), sell_venue: "b".into(),
            gross_profit_usd: 100.0, estimated_cost_usd: 20.0, net_profit_usd: 80.0,
            confidence: 0.8, trade_size_usd: 1000.0, details: "".into(),
            detected_at: 0, requires_flash_loan: false,
        };
        assert!(o.net_profit_usd > 0.0);
        assert_eq!(o.strategy, "test");
    }

    #[test]
    fn test_pnl_tracking() {
        let mut per_strategy = HashMap::new();
        per_strategy.insert("test".into(), StrategyPnL { trades: 5, successes: 3, total_profit: 150.0 });
        let pnl = SuperPnL {
            total_trades: 5, successful_trades: 3, failed_trades: 2,
            success_rate: 60.0, total_profit_usd: 200.0, total_cost_usd: 50.0,
            total_net_profit_usd: 150.0, daily_pnl: 50.0, weekly_pnl: 100.0,
            monthly_pnl: 150.0, running_balance_usd: 150.0, per_strategy,
        };
        assert_eq!(pnl.total_trades, 5);
        assert_eq!(pnl.success_rate, 60.0);
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let e = SuperArbEngine::new(test_config());
        assert!(!e.is_running());
    }

    #[tokio::test]
    async fn test_execution_basic() {
        let config = test_config();
        let exec = ExecutionEngine::new(config);
        let opp = Opportunity {
            id: "t".into(), strategy: "cross-venue".into(), pair: "ETH".into(),
            buy_venue: "binance".into(), sell_venue: "coinbase".into(),
            gross_profit_usd: 50.0, estimated_cost_usd: 15.0, net_profit_usd: 35.0,
            confidence: 0.7, trade_size_usd: 1000.0, details: "".into(),
            detected_at: 0, requires_flash_loan: false,
        };
        let trade = exec.execute(&opp).await;
        assert!(trade.success);
        assert_eq!(trade.strategy, "cross-venue");
    }
}
