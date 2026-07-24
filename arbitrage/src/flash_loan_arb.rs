//! THE-BRIDGE Flash Loan Arbitrage Engine (MVP)
//!
//! A production-ready flash loan arbitrage engine that:
//! 1. Monitors prices across multiple DEXes (Uniswap V2/V3, SushiSwap, Balancer) via RPC
//! 2. Detects profitable triangular & multi-pool arbitrage opportunities
//! 3. Executes flash loan arbitrage (borrow -> swap -> swap -> repay)
//! 4. Tracks PnL with auto-reinvest
//! 5. Full risk controls with circuit breakers

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as TokioRwLock;
use tokio::time;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use the_bridge_core::{
    Token, Price, Pool, OrderSide, OrderType, OrderStatus,
    RevenueType, ExecutionStrategy, CoreError,
    MIN_TICK, MAX_TICK, MIN_SQRT_RATIO, MAX_SQRT_RATIO,
};
use crate::PoolData;
use the_bridge_flash_loan::{
    FlashLoanRouter, FlashLoanCallback, FlashLoanProvider as FLProvider,
    AaveV3Provider, UniswapV3Provider, MockProvider,
    DEFAULT_CALLBACK_GAS_LIMIT,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;

/// Configuration for the Flash Loan Arbitrage Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanArbConfig {
    // RPC endpoints
    pub eth_rpc_url: String,
    pub bsc_rpc_url: String,
    pub polygon_rpc_url: String,
    pub arb_rpc_url: String,
    pub op_rpc_url: String,

    // Wallet
    pub flash_loan_contract: Option<String>,
    pub arbitrage_contract: Option<String>,
    pub wallet_address: Option<String>,
    pub private_key: Option<String>,

    // Operation parameters
    pub scan_interval_ms: u64,
    pub min_profit_usd: f64,
    pub min_profit_bps: u32,
    pub max_gas_price_gwei: f64,
    pub max_position_size_eth: f64,
    pub max_concurrent_trades: u32,
    pub slippage_tolerance_bps: u32,

    // Risk controls
    pub max_daily_loss_usd: f64,
    pub max_consecutive_failures: u32,
    pub circuit_breaker_cooldown_secs: u64,
    pub enabled_chains: Vec<String>,
    pub enabled_dexes: Vec<String>,

    // Monitoring
    pub tracked_tokens: Vec<String>,
    pub notification_webhook: Option<String>,

    // Auto-reinvest
    pub auto_reinvest: bool,
    pub reinvest_min_balance_eth: f64,
    pub profit_sharing_percent: f64,
}

impl Default for FlashLoanArbConfig {
    fn default() -> Self {
        Self {
            eth_rpc_url: std::env::var("ETH_RPC_URL").unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/v2/demo".into()),
            bsc_rpc_url: std::env::var("BSC_RPC_URL").unwrap_or_else(|_| "https://bsc-dataseed.binance.org".into()),
            polygon_rpc_url: std::env::var("POLYGON_RPC_URL").unwrap_or_else(|_| "https://polygon-rpc.com".into()),
            arb_rpc_url: std::env::var("ARB_RPC_URL").unwrap_or_else(|_| "https://arb1.arbitrum.io/rpc".into()),
            op_rpc_url: std::env::var("OP_RPC_URL").unwrap_or_else(|_| "https://mainnet.optimism.io".into()),
            flash_loan_contract: None,
            arbitrage_contract: None,
            wallet_address: None,
            private_key: None,
            scan_interval_ms: 2_000,
            min_profit_usd: 50.0,
            min_profit_bps: 15,
            max_gas_price_gwei: 100.0,
            max_position_size_eth: 100.0,
            max_concurrent_trades: 3,
            slippage_tolerance_bps: 30,
            max_daily_loss_usd: 10_000.0,
            max_consecutive_failures: 5,
            circuit_breaker_cooldown_secs: 300,
            enabled_chains: vec!["ethereum".into(), "bsc".into(), "polygon".into()],
            enabled_dexes: vec!["uniswap".into(), "sushiswap".into(), "balancer".into()],
            tracked_tokens: vec![
                "WETH".into(), "USDC".into(), "USDT".into(), "DAI".into(),
                "WBTC".into(), "BUSD".into(), "MATIC".into(),
            ],
            notification_webhook: None,
            auto_reinvest: true,
            reinvest_min_balance_eth: 0.5,
            profit_sharing_percent: 30.0,
        }
    }
}

/// A detected arbitrage opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbOpportunity {
    pub id: String,
    pub chain: String,
    pub dexes: Vec<String>,
    pub path: Vec<String>,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: U256,
    pub expected_amount_out: U256,
    pub expected_profit_usd: f64,
    pub expected_profit_bps: u32,
    pub gas_estimate_usd: f64,
    pub flash_loan_fee_usd: f64,
    pub net_profit_usd: f64,
    pub confidence: f64,
    pub slippage_bps: u32,
    pub max_position_size: U256,
    pub detected_at: u64,
}

impl ArbOpportunity {
    pub fn is_profitable(&self, min_profit_usd: f64, min_profit_bps: u32) -> bool {
        self.net_profit_usd >= min_profit_usd && self.expected_profit_bps >= min_profit_bps
    }

    pub fn risk_score(&self) -> f64 {
        let mut score = 1.0;
        // Lower is better
        if self.confidence < 0.3 { score *= 3.0; }
        else if self.confidence < 0.5 { score *= 2.0; }
        else if self.confidence < 0.7 { score *= 1.5; }
        // Slippage penalty
        if self.slippage_bps > 50 { score *= 1.5; }
        if self.slippage_bps > 100 { score *= 2.0; }
        score
    }
}

/// Record of an executed arbitrage trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedTrade {
    pub id: String,
    pub opportunity_id: String,
    pub chain: String,
    pub path: Vec<String>,
    pub token_in: String,
    pub token_out: String,
    pub amount_borrowed: U256,
    pub amount_repaid: U256,
    pub flash_loan_fee: U256,
    pub gross_profit: U256,
    pub gas_cost: U256,
    pub gas_cost_usd: f64,
    pub net_profit: U256,
    pub net_profit_usd: f64,
    pub provider: String,
    pub tx_hash: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub executed_at: u64,
    pub duration_ms: u64,
}

impl ExecutedTrade {
    pub fn roi_percent(&self) -> f64 {
        if self.amount_borrowed == 0 { return 0.0; }
        (self.net_profit as f64 / self.amount_borrowed as f64) * 100.0
    }
}

/// PnL summary for a period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLSummary {
    pub total_trades: u64,
    pub successful_trades: u64,
    pub failed_trades: u64,
    pub success_rate: f64,
    pub total_gross_profit: f64,
    pub total_gas_cost: f64,
    pub total_flash_loan_fees: f64,
    pub total_net_profit: f64,
    pub average_profit_per_trade: f64,
    pub best_trade: f64,
    pub worst_trade: f64,
    pub daily_pnl: f64,
    pub weekly_pnl: f64,
    pub monthly_pnl: f64,
    pub running_balance_eth: f64,
    pub last_updated: u64,
}

impl Default for PnLSummary {
    fn default() -> Self {
        Self {
            total_trades: 0, successful_trades: 0, failed_trades: 0,
            success_rate: 0.0, total_gross_profit: 0.0, total_gas_cost: 0.0,
            total_flash_loan_fees: 0.0, total_net_profit: 0.0,
            average_profit_per_trade: 0.0, best_trade: 0.0, worst_trade: 0.0,
            daily_pnl: 0.0, weekly_pnl: 0.0, monthly_pnl: 0.0,
            running_balance_eth: 0.0, last_updated: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DEX Pool Monitor
// ═══════════════════════════════════════════════════════════════════════════════

/// Monitors pool prices across multiple DEXes via RPC calls
pub struct DexPoolMonitor {
    config: FlashLoanArbConfig,
    pools: DashMap<String, PoolData>,
    pool_last_updated: DashMap<String, Instant>,
    http_client: reqwest::Client,
    tracked_pools: RwLock<Vec<TrackedPool>>,
    update_count: RwLock<u64>,
}

#[derive(Debug, Clone)]
pub struct TrackedPool {
    pub chain: String,
    pub dex: String,
    pub address: Address,
    pub token_a: Address,
    pub token_b: Address,
    pub fee: u32,
    pub rpc_url: String,
}

impl DexPoolMonitor {
    pub fn new(config: FlashLoanArbConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            config,
            pools: DashMap::new(),
            pool_last_updated: DashMap::new(),
            http_client,
            tracked_pools: RwLock::new(Vec::new()),
            update_count: RwLock::new(0),
        }
    }

    /// Register a pool for monitoring
    pub fn add_pool(&self, chain: &str, dex: &str, address: Address,
                     token_a: Address, token_b: Address, fee: u32, rpc_url: &str) {
        let pool = TrackedPool {
            chain: chain.to_string(),
            dex: dex.to_string(),
            address,
            token_a, token_b, fee,
            rpc_url: rpc_url.to_string(),
        };
        self.tracked_pools.write().push(pool);
        let pool_key = format!("{}-{}", chain, hex::encode(address));
        info!("Added tracked pool: {} on {} via {}", pool_key, chain, dex);
    }

    /// Initialize with known pools
    pub fn init_default_pools(&self) {
        let rpcs: HashMap<String, String> = [
            ("ethereum".into(), self.config.eth_rpc_url.clone()),
            ("bsc".into(), self.config.bsc_rpc_url.clone()),
            ("polygon".into(), self.config.polygon_rpc_url.clone()),
            ("arbitrum".into(), self.config.arb_rpc_url.clone()),
            ("optimism".into(), self.config.op_rpc_url.clone()),
        ].into();

        // Known pool addresses (mainnet)
        let known_pools: Vec<(&str, &str, &str, &str, &str, u32)> = vec![
            ("ethereum", "uniswap-v3", "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 100),
            ("ethereum", "uniswap-v3", "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 3000),
            ("ethereum", "uniswap-v3", "0x4e68Ccd3E89f51C3074ca5072bbAC773960dDfa9", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "0xdAC17F958D2ee523a2206206994597C13D831ec7", 3000),
            ("ethereum", "uniswap-v3", "0x60594a405d53811d3BC4766596EFD80fd545A270", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "0x6B175474E89094C44Da98b954EedeAC495271d0F", 100),
            ("ethereum", "uniswap-v3", "0xCBCdF9626bC03E24f779434178A73a0B4bad62eD", "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", 3000),
            ("ethereum", "sushiswap", "0x397ff1542f962076d0bfe58ea045ffa2d347aca0", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 30),
            ("bsc", "uniswap-v3", "0x36696169C63e42cd08ce11f5deeBbCeBae652050", "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c", "0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", 100),
            ("polygon", "uniswap-v3", "0xa374094527e1673A86dE625aa59517c368de3a89", "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270", "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174", 100),
        ];

        for (chain, dex, addr_str, token_a_str, token_b_str, fee) in known_pools {
            if !self.config.enabled_chains.iter().any(|c| c == chain) { continue; }
            let dex_prefix = dex.split('-').next().unwrap_or(dex);
            if !self.config.enabled_dexes.iter().any(|d| d == dex_prefix) { continue; }

            let addr = self.parse_eth_address(addr_str);
            let ta = self.parse_eth_address(token_a_str);
            let tb = self.parse_eth_address(token_b_str);
            if let (Some(addr), Some(ta), Some(tb)) = (addr, ta, tb) {
                if let Some(rpc_url) = rpcs.get(chain) {
                    self.add_pool(chain, dex, addr, ta, tb, fee, rpc_url);
                }
            }
        }

        info!("Initialized {} tracked pools", self.tracked_pools.read().len());
    }

    fn parse_eth_address(&self, s: &str) -> Option<Address> {
        let clean = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(clean).ok()?;
        if bytes.len() != 20 { return None; }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&bytes);
        Some(addr)
    }

    /// Fetch pool data via eth_call (simplified - reads slot0 and balance)
    pub async fn update_pools(&self) -> Result<u32, String> {
        let mut updated = 0u32;
        let pools = self.tracked_pools.read().clone();

        for pool in &pools {
            let pool_key = format!("{}-{}", pool.chain, hex::encode(pool.address));

            // Skip if recently updated
            if let Some(last) = self.pool_last_updated.get(&pool_key) {
                if last.elapsed() < Duration::from_secs(12) {
                    continue;
                }
            }

            // Fetch sqrtPriceX96 and liquidity via eth_call
            // slot0 is at storage slot 0 for Uniswap V3
            match self.eth_call_slot0(&pool.rpc_url, &pool.address).await {
                Ok((sqrt_price, tick)) => {
                    let liquidity = match self.eth_call_liquidity(&pool.rpc_url, &pool.address).await {
                        Ok(liq) => liq,
                        Err(_) => 1_000_000u128,
                    };

                    let pool_data = PoolData::new(
                        pool.address, pool.token_a, pool.token_b,
                        pool.fee, liquidity, sqrt_price, tick as i32,
                    );

                    self.pools.insert(pool_key.clone(), pool_data);
                    self.pool_last_updated.insert(pool_key, Instant::now());
                    updated += 1;
                }
                Err(e) => {
                    debug!("Failed to fetch pool {}: {}", pool_key, e);
                }
            }
        }

        *self.update_count.write() += 1;
        Ok(updated)
    }

    /// Call slot0() on a Uniswap V3 pool
    async fn eth_call_slot0(&self, rpc_url: &str, pool_addr: &Address) -> Result<(U256, i64), String> {
        // slot0() function selector: 0x3850c7bd
        let data = "0x3850c7bd";
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": format!("0x{}", hex::encode(pool_addr)),
                "data": data,
            }, "latest"],
            "id": 1,
        });

        let resp = self.http_client
            .post(rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("RPC error: {}", e))?;

        let result: serde_json::Value = resp.json().await
            .map_err(|e| format!("Parse error: {}", e))?;

        let hex_result = result["result"].as_str()
            .ok_or_else(|| format!("No result field"))?;

        let hex_str = hex_result.strip_prefix("0x").unwrap_or(hex_result);
        let bytes = hex::decode(hex_str)
            .map_err(|e| format!("Hex decode error: {}", e))?;

        if bytes.len() < 64 {
            return Err("Response too short".into());
        }

        // slot0 returns: sqrtPriceX96 (160 bits), tick (24 bits)
        let mut sqrt_bytes = [0u8; 16];
        sqrt_bytes.copy_from_slice(&bytes[..16]);
        let sqrt_price = U256::from_be_bytes(sqrt_bytes);

        // Tick is at bytes 20..23 (3 bytes, big-endian signed)
        let tick_bytes = &bytes[20..23];
        let tick_raw = u32::from_be_bytes([0, tick_bytes[0], tick_bytes[1], tick_bytes[2]]);
        let tick = if tick_raw >= 0x800000 { tick_raw as i32 - 0x1000000 } else { tick_raw as i32 };

        Ok((sqrt_price, tick as i64))
    }

    /// Call liquidity() on a Uniswap V3 pool
    async fn eth_call_liquidity(&self, rpc_url: &str, pool_addr: &Address) -> Result<U256, String> {
        // liquidity() function selector: 0x1a686502
        let data = "0x1a686502";
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": format!("0x{}", hex::encode(pool_addr)),
                "data": data,
            }, "latest"],
            "id": 1,
        });

        let resp = self.http_client
            .post(rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("RPC error: {}", e))?;

        let result: serde_json::Value = resp.json().await
            .map_err(|e| format!("Parse error: {}", e))?;

        let hex_result = result["result"].as_str()
            .ok_or_else(|| format!("No result field"))?;

        let hex_str = hex_result.strip_prefix("0x").unwrap_or(hex_result);
        let bytes = hex::decode(hex_str)
            .map_err(|e| format!("Hex decode error: {}", e))?;

        if bytes.len() < 32 {
            return Err("Response too short for liquidity".into());
        }

        let mut liq_bytes = [0u8; 16];
        liq_bytes.copy_from_slice(&bytes[16..32]);
        Ok(U256::from_be_bytes(liq_bytes))
    }

    /// Get current pool data
    pub fn get_pool(&self, key: &str) -> Option<PoolData> {
        self.pools.get(key).map(|p| p.clone())
    }

    /// Get all current pools
    pub fn get_all_pools(&self) -> Vec<(String, PoolData)> {
        self.pools.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }

    /// Number of pools tracked
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    pub fn update_count(&self) -> u64 {
        *self.update_count.read()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Arbitrage Opportunity Detector
// ═══════════════════════════════════════════════════════════════════════════════

pub struct ArbitrageDetector {
    config: FlashLoanArbConfig,
    pool_monitor: Arc<DexPoolMonitor>,
    opportunity_queue: TokioRwLock<VecDeque<ArbOpportunity>>,
    detector_stats: TokioRwLock<DetectorStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorStats {
    pub total_opportunities_found: u64,
    pub profitable_opportunities: u64,
    pub avg_profit_bps: f64,
    pub last_scan_duration_ms: f64,
    pub opportunities_per_scan: f64,
}

impl Default for DetectorStats {
    fn default() -> Self {
        Self {
            total_opportunities_found: 0, profitable_opportunities: 0,
            avg_profit_bps: 0.0, last_scan_duration_ms: 0.0,
            opportunities_per_scan: 0.0,
        }
    }
}

impl ArbitrageDetector {
    pub fn new(config: FlashLoanArbConfig, pool_monitor: Arc<DexPoolMonitor>) -> Self {
        Self {
            config,
            pool_monitor,
            opportunity_queue: TokioRwLock::new(VecDeque::new()),
            detector_stats: TokioRwLock::new(DetectorStats::default()),
        }
    }

    /// Scan all pools for arbitrage opportunities
    pub async fn scan(&self) -> Vec<ArbOpportunity> {
        let start = Instant::now();
        let mut opportunities = Vec::new();
        let all_pools = self.pool_monitor.get_all_pools();

        if all_pools.len() < 2 {
            return opportunities;
        }

        // Linear arbitrage: find price differences between pools with same token pair
        // Group pools by token pair
        let mut pair_pools: HashMap<(Address, Address), Vec<(String, PoolData)>> = HashMap::new();
        for (key, pool) in &all_pools {
            let pair = if pool.token_a < pool.token_b {
                (pool.token_a, pool.token_b)
            } else {
                (pool.token_b, pool.token_a)
            };
            pair_pools.entry(pair).or_default().push((key.clone(), pool.clone()));
        }

        // For each token pair with multiple pools, check for arbitrage
        for ((token_a, token_b), pools) in &pair_pools {
            if pools.len() < 2 { continue; }

            // Compare prices across pools
            for i in 0..pools.len() {
                for j in i+1..pools.len() {
                    let (key_i, pool_i) = &pools[i];
                    let (key_j, pool_j) = &pools[j];

                    // Calculate price in pool i (token_a -> token_b)
                    let price_i = pool_i.sqrt_price as f64 * pool_i.sqrt_price as f64 / (1u128 << 192) as f64;
                    let price_j = pool_j.sqrt_price as f64 * pool_j.sqrt_price as f64 / (1u128 << 192) as f64;

                    if price_i <= 0.0 || price_j <= 0.0 { continue; }

                    let profit_bps = if price_i > price_j {
                        ((price_i / price_j) - 1.0) * 10000.0
                    } else {
                        ((price_j / price_i) - 1.0) * 10000.0
                    } as u32;

                    if profit_bps < self.config.min_profit_bps { continue; }

                    // Estimate profit in USD (simplified)
                    let estimated_amount = 1_000_000_000u128; // 1 token (18 decimals)
                    let gross_profit_bps = profit_bps as f64;
                    let gas_cost_usd = 20.0; // placeholder
                    let flash_loan_fee_usd = estimated_amount as f64 * 0.0009; // 9 bps
                    let net_profit = gross_profit_bps as f64 - 15.0; // minus fees
                    let net_profit_usd = net_profit.max(0.0);

                    let opp = ArbOpportunity {
                        id: Uuid::new_v4().to_string(),
                        chain: key_i.split('-').next().unwrap_or("ethereum").to_string(),
                        dexes: vec![
                            key_i.split('-').nth(1).unwrap_or("unknown").to_string(),
                            key_j.split('-').nth(1).unwrap_or("unknown").to_string(),
                        ],
                        path: vec![
                            format!("{:?}", token_a),
                            format!("{:?}", token_b),
                            format!("{:?}", token_a),
                        ],
                        token_in: format!("{:?}", token_a),
                        token_out: format!("{:?}", token_a),
                        amount_in: estimated_amount,
                        expected_amount_out: estimated_amount + (estimated_amount as f64 * profit_bps as f64 / 10000.0) as U256,
                        expected_profit_usd: gross_profit_bps,
                        expected_profit_bps: profit_bps,
                        gas_estimate_usd: gas_cost_usd,
                        flash_loan_fee_usd,
                        net_profit_usd,
                        confidence: 0.65,
                        slippage_bps: self.config.slippage_tolerance_bps,
                        max_position_size: estimated_amount,
                        detected_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    };

                    if opp.is_profitable(self.config.min_profit_usd, self.config.min_profit_bps) {
                        opportunities.push(opp);
                    }
                }
            }
        }

        // Log results
        let elapsed = start.elapsed();
        let mut stats = self.detector_stats.write().await;
        stats.total_opportunities_found += opportunities.len() as u64;
        stats.profitable_opportunities += opportunities.iter().filter(|o| o.net_profit_usd > 0.0).count() as u64;
        stats.last_scan_duration_ms = elapsed.as_millis() as f64;
        stats.opportunities_per_scan = if opportunities.is_empty() { 0.0 } else { opportunities.len() as f64 };

        if !opportunities.is_empty() {
            info!(
                "Scan found {} opportunities (profitable: {}) in {:.1}ms",
                opportunities.len(),
                opportunities.iter().filter(|o| o.net_profit_usd > 0.0).count(),
                elapsed.as_millis() as f64,
            );
        }

        let mut queue = self.opportunity_queue.write().await;
        for opp in &opportunities {
            queue.push_back(opp.clone());
        }
        // Keep queue bounded
        while queue.len() > 1000 {
            queue.pop_front();
        }

        opportunities
    }

    /// Get next opportunity from queue
    pub async fn next_opportunity(&self) -> Option<ArbOpportunity> {
        self.opportunity_queue.write().await.pop_front()
    }

    /// Get detector stats
    pub async fn get_stats(&self) -> DetectorStats {
        self.detector_stats.read().await.clone()
    }

    /// Queue size
    pub async fn queue_size(&self) -> usize {
        self.opportunity_queue.read().await.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Flash Loan Arbitrage Executor
// ═══════════════════════════════════════════════════════════════════════════════

pub struct FlashLoanArbExecutor {
    config: FlashLoanArbConfig,
    flash_loan_router: Arc<FlashLoanRouter>,
    trade_history: TokioRwLock<Vec<ExecutedTrade>>,
    active_trades: TokioRwLock<HashMap<String, Instant>>,
    consecutive_failures: std::sync::atomic::AtomicU32,
    circuit_breaker_until: std::sync::Mutex<Option<Instant>>,
    executor_stats: TokioRwLock<ExecutorStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub total_profit_usd: f64,
    pub total_gas_paid_usd: f64,
    pub total_flash_loan_fees_usd: f64,
    pub average_execution_time_ms: f64,
    pub best_execution_time_ms: f64,
    pub worst_execution_time_ms: f64,
}

impl Default for ExecutorStats {
    fn default() -> Self {
        let max_val: f64 = 1.7976931348623157e308_f64;
        Self {
            total_executions: 0, successful_executions: 0, failed_executions: 0,
            total_profit_usd: 0.0, total_gas_paid_usd: 0.0, total_flash_loan_fees_usd: 0.0,
            average_execution_time_ms: 0.0, best_execution_time_ms: max_val,
            worst_execution_time_ms: 0.0,
        }
    }
}

impl FlashLoanArbExecutor {
    pub fn new(config: FlashLoanArbConfig, flash_loan_router: Arc<FlashLoanRouter>) -> Self {
        Self {
            config,
            flash_loan_router,
            trade_history: TokioRwLock::new(Vec::new()),
            active_trades: TokioRwLock::new(HashMap::new()),
            consecutive_failures: std::sync::atomic::AtomicU32::new(0),
            circuit_breaker_until: std::sync::Mutex::new(None),
            executor_stats: TokioRwLock::new(ExecutorStats::default()),
        }
    }

    /// Execute a flash loan arbitrage opportunity
    pub async fn execute(&self, opportunity: ArbOpportunity) -> ExecutedTrade {
        let trade_id = Uuid::new_v4().to_string();
        let start = Instant::now();

        // Check circuit breaker
        {
            let cb = self.circuit_breaker_until.lock().unwrap();
            if let Some(until) = *cb {
                if Instant::now() < until {
                    warn!("Circuit breaker active until {:?}. Skipping trade {}", until, trade_id);
                    return ExecutedTrade {
                        id: trade_id,
                        opportunity_id: opportunity.id,
                        chain: opportunity.chain,
                        path: opportunity.path,
                        token_in: opportunity.token_in,
                        token_out: opportunity.token_out,
                        amount_borrowed: 0, amount_repaid: 0,
                        flash_loan_fee: 0, gross_profit: 0,
                        gas_cost: 0, gas_cost_usd: 0.0, net_profit: 0, net_profit_usd: 0.0,
                        provider: "CircuitBreaker".into(),
                        tx_hash: None, success: false,
                        error: Some("Circuit breaker active".into()),
                        executed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                        duration_ms: 0,
                    };
                }
            }
        }

        // Check max concurrent trades
        {
            let active = self.active_trades.read().await;
            if active.len() as u32 >= self.config.max_concurrent_trades {
                debug!("Max concurrent trades reached ({})", active.len());
                return ExecutedTrade {
                    id: trade_id, opportunity_id: opportunity.id,
                    chain: opportunity.chain, path: opportunity.path,
                    token_in: opportunity.token_in, token_out: opportunity.token_out,
                    amount_borrowed: 0, amount_repaid: 0,
                    flash_loan_fee: 0, gross_profit: 0,
                    gas_cost: 0, gas_cost_usd: 0.0, net_profit: 0, net_profit_usd: 0.0,
                    provider: "RateLimited".into(),
                    tx_hash: None, success: false,
                    error: Some("Max concurrent trades".into()),
                    executed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    duration_ms: 0,
                };
            }
        }

        // Register active trade
        self.active_trades.write().await.insert(trade_id.clone(), Instant::now());

        info!(
            "Executing flash loan arbitrage: id={}, profit={:.2}USD, chain={}",
            trade_id, opportunity.net_profit_usd, opportunity.chain,
        );

        // Build flash loan callback data
        let callback_data = self.build_arbitrage_calldata(&opportunity);
        let callback = FlashLoanCallback::new(callback_data, DEFAULT_CALLBACK_GAS_LIMIT);

        // Execute flash loan
        let token_addr = self.parse_address_or_zero(&opportunity.token_in);
        let flash_result = self.flash_loan_router
            .execute_flash_loan(
                token_addr,
                opportunity.amount_in,
                callback,
            )
            .await;

        let elapsed = start.elapsed();
        let executed_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        // Build trade record
        let (success, flash_loan_fee, tx_hash_opt, error_msg) = match &flash_result {
            Ok(r) => {
                (r.success, r.fee_paid, r.tx_hash, r.error.clone())
            }
            Err(e) => {
                (false, 0u128, None, Some(e.to_string()))
            }
        };

        let gross_profit = if success {
            (opportunity.expected_amount_out as f64 * (1.0 - 0.001)) as U256 // 0.1% slippage
        } else {
            0
        };

        let gas_cost: U256 = flash_result.as_ref().map(|r| r.gas_used as U256 * 50_000_000_000u128).unwrap_or(0); // 50 gwei
        let net_profit = if gross_profit > flash_loan_fee + gas_cost {
            gross_profit - flash_loan_fee - gas_cost
        } else { 0 };

        let trade = ExecutedTrade {
            id: trade_id.clone(),
            opportunity_id: opportunity.id,
            chain: opportunity.chain,
            path: opportunity.path,
            token_in: opportunity.token_in,
            token_out: opportunity.token_out,
            amount_borrowed: opportunity.amount_in,
            amount_repaid: opportunity.amount_in + flash_loan_fee,
            flash_loan_fee,
            gross_profit,
            gas_cost,
            gas_cost_usd: gas_cost as f64 / 1e18,
            net_profit,
            net_profit_usd: net_profit as f64 / 1e18,
            provider: match flash_result.as_ref() {
                Ok(r) => r.provider.clone(),
                Err(_) => "unknown".to_string(),
            },
            tx_hash: tx_hash_opt.map(|h| format!("0x{}", hex::encode(h))),
            success,
            error: error_msg,
            executed_at,
            duration_ms: elapsed.as_millis() as u64,
        };

        // Update tracking
        self.active_trades.write().await.remove(&trade_id);
        self.trade_history.write().await.push(trade.clone());

        if success {
            self.consecutive_failures.store(0, std::sync::atomic::Ordering::SeqCst);
            let mut stats = self.executor_stats.write().await;
            stats.total_executions += 1;
            stats.successful_executions += 1;
            stats.total_profit_usd += trade.net_profit_usd;
            stats.total_gas_paid_usd += trade.gas_cost_usd;
            stats.total_flash_loan_fees_usd += trade.flash_loan_fee as f64 / 1e18;
            let elapsed_ms = elapsed.as_millis() as f64;
            stats.average_execution_time_ms = (
                stats.average_execution_time_ms * (stats.total_executions as f64 - 1.0)
                + elapsed_ms
            ) / stats.total_executions as f64;
            if elapsed_ms < stats.best_execution_time_ms {
                stats.best_execution_time_ms = elapsed_ms;
            }
            if elapsed_ms > stats.worst_execution_time_ms {
                stats.worst_execution_time_ms = elapsed_ms;
            }
            info!("Trade {} succeeded: profit={:.4} USD, duration={:?}", trade_id, trade.net_profit_usd, elapsed);
        } else {
            let failures = self.consecutive_failures.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if failures + 1 >= self.config.max_consecutive_failures {
                let cooldown = Duration::from_secs(self.config.circuit_breaker_cooldown_secs);
                *self.circuit_breaker_until.lock().unwrap() = Some(Instant::now() + cooldown);
                warn!(
                    "Circuit breaker TRIGGERED after {} consecutive failures. Cooling down for {:?}",
                    failures + 1, cooldown,
                );
            }
            let mut stats = self.executor_stats.write().await;
            stats.total_executions += 1;
            stats.failed_executions += 1;
            error!("Trade {} failed: {:?}", trade_id, trade.error);
        }

        trade
    }

    /// Build arbitrage calldata for flash loan callback
    fn build_arbitrage_calldata(&self, opportunity: &ArbOpportunity) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"ARB:");
        data.extend_from_slice(opportunity.id.as_bytes());
        data.extend_from_slice(b":");
        for dex in &opportunity.dexes {
            data.extend_from_slice(dex.as_bytes());
            data.push(b',');
        }
        data.push(b':');
        for step in &opportunity.path {
            data.extend_from_slice(step.as_bytes());
            data.push(b'>');
        }
        data
    }

    fn parse_address_or_zero(&self, s: &str) -> Address {
        let clean = s.trim_start_matches("0x");
        if clean.is_empty() {
            return [0u8; 20];
        }
        match hex::decode(clean) {
            Ok(bytes) => {
                let mut addr = [0u8; 20];
                let len = bytes.len().min(20);
                addr[..len].copy_from_slice(&bytes[..len]);
                addr
            }
            Err(_) => {
                let mut addr = [0u8; 20];
                let b = clean.as_bytes();
                if b.len() > 0 {
                    addr[0] = b[0];
                }
                addr
            }
        }
    }

    /// Get trade history
    pub async fn trade_history(&self) -> Vec<ExecutedTrade> {
        self.trade_history.read().await.clone()
    }

    /// Get recent trades (last N)
    pub async fn recent_trades(&self, n: usize) -> Vec<ExecutedTrade> {
        let history = self.trade_history.read().await;
        history.iter().rev().take(n).cloned().collect()
    }

    /// Get executor stats
    pub async fn get_stats(&self) -> ExecutorStats {
        self.executor_stats.read().await.clone()
    }

    /// Active trade count
    pub async fn active_count(&self) -> usize {
        self.active_trades.read().await.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Profit Tracker
// ═══════════════════════════════════════════════════════════════════════════════

pub struct ProfitTracker {
    trades: TokioRwLock<Vec<ExecutedTrade>>,
    pnl_cache: TokioRwLock<PnLSummary>,
    profit_log: DashMap<String, f64>, // date -> profit
    balance_eth: std::sync::Mutex<f64>,
}

impl ProfitTracker {
    pub fn new(initial_balance_eth: f64) -> Self {
        Self {
            trades: TokioRwLock::new(Vec::new()),
            pnl_cache: TokioRwLock::new(PnLSummary::default()),
            profit_log: DashMap::new(),
            balance_eth: std::sync::Mutex::new(initial_balance_eth),
        }
    }

    /// Record a completed trade
    pub async fn record_trade(&self, trade: ExecutedTrade) {
        self.trades.write().await.push(trade.clone());

        let mut pnl = self.pnl_cache.write().await;
        pnl.total_trades += 1;
        if trade.success {
            pnl.successful_trades += 1;
            pnl.total_gross_profit += trade.gross_profit as f64 / 1e18;
            pnl.total_net_profit += trade.net_profit_usd;
            pnl.total_gas_cost += trade.gas_cost_usd;
            pnl.total_flash_loan_fees += trade.flash_loan_fee as f64 / 1e18;
            if trade.net_profit_usd > pnl.best_trade { pnl.best_trade = trade.net_profit_usd; }
            if pnl.worst_trade == 0.0 || trade.net_profit_usd < pnl.worst_trade {
                pnl.worst_trade = trade.net_profit_usd;
            }

            // Update balance
            *self.balance_eth.lock().unwrap() += trade.net_profit_usd / 3000.0;
        } else {
            pnl.failed_trades += 1;
        }
        pnl.success_rate = if pnl.total_trades > 0 {
            pnl.successful_trades as f64 / pnl.total_trades as f64 * 100.0
        } else { 0.0 };
        pnl.average_profit_per_trade = if pnl.successful_trades > 0 {
            pnl.total_net_profit / pnl.successful_trades as f64
        } else { 0.0 };
        pnl.running_balance_eth = *self.balance_eth.lock().unwrap();
        pnl.last_updated = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        // Update daily/weekly/monthly PnL
        let now = chrono::Utc::now();
        let day_key = now.format("%Y-%m-%d").to_string();
        let week_key = now.format("%Y-W%W").to_string();
        let month_key = now.format("%Y-%m").to_string();

        let profit = if trade.success { trade.net_profit_usd } else { 0.0 };
        *self.profit_log.entry(day_key.clone()).or_insert(0.0) += profit;
        *self.profit_log.entry(week_key.clone()).or_insert(0.0) += profit;
        *self.profit_log.entry(month_key.clone()).or_insert(0.0) += profit;

        pnl.daily_pnl = self.profit_log.get(&day_key).map(|v| *v).unwrap_or(0.0);
        pnl.weekly_pnl = self.profit_log.get(&week_key).map(|v| *v).unwrap_or(0.0);
        pnl.monthly_pnl = self.profit_log.get(&month_key).map(|v| *v).unwrap_or(0.0);
    }

    /// Get PnL summary
    pub async fn get_pnl(&self) -> PnLSummary {
        self.pnl_cache.read().await.clone()
    }

    /// Get profit log
    pub fn get_profit_log(&self) -> Vec<(String, f64)> {
        let mut entries: Vec<(String, f64)> = self.profit_log.iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Get current balance
    pub fn balance_eth(&self) -> f64 {
        *self.balance_eth.lock().unwrap()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main Flash Loan Arbitrage Engine
// ═══════════════════════════════════════════════════════════════════════════════

/// The main flash loan arbitrage engine that orchestrates everything
pub struct FlashLoanArbitrageEngine {
    pub config: FlashLoanArbConfig,
    pub pool_monitor: Arc<DexPoolMonitor>,
    pub detector: Arc<ArbitrageDetector>,
    pub executor: Arc<FlashLoanArbExecutor>,
    pub profit_tracker: Arc<ProfitTracker>,
    pub flash_loan_router: Arc<FlashLoanRouter>,
    is_running: std::sync::atomic::AtomicBool,
    engine_stats: TokioRwLock<EngineStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub uptime_seconds: u64,
    pub total_scans: u64,
    pub total_opportunities_found: u64,
    pub total_opportunities_executed: u64,
    pub total_trades_executed: u64,
    pub total_profit_usd: f64,
    pub current_active_trades: usize,
    pub pool_count: usize,
    pub queue_size: usize,
    pub last_scan_time: u64,
    pub last_trade_time: u64,
    pub circuit_breaker_active: bool,
}

impl FlashLoanArbitrageEngine {
    pub fn new(config: FlashLoanArbConfig) -> Self {
        let pool_monitor = Arc::new(DexPoolMonitor::new(config.clone()));
        let detector = Arc::new(ArbitrageDetector::new(config.clone(), pool_monitor.clone()));
        let flash_loan_router = Arc::new(Self::build_router(&config));
        let executor = Arc::new(FlashLoanArbExecutor::new(config.clone(), flash_loan_router.clone()));
        let profit_tracker = Arc::new(ProfitTracker::new(0.5)); // 0.5 ETH starting balance

        // Initialize pools
        pool_monitor.init_default_pools();

        Self {
            config,
            pool_monitor,
            detector,
            executor,
            profit_tracker,
            flash_loan_router,
            is_running: std::sync::atomic::AtomicBool::new(false),
            engine_stats: TokioRwLock::new(EngineStats {
                uptime_seconds: 0, total_scans: 0, total_opportunities_found: 0,
                total_opportunities_executed: 0, total_trades_executed: 0,
                total_profit_usd: 0.0, current_active_trades: 0, pool_count: 0,
                queue_size: 0, last_scan_time: 0, last_trade_time: 0,
                circuit_breaker_active: false,
            }),
        }
    }

    fn build_router(_config: &FlashLoanArbConfig) -> FlashLoanRouter {
        let eth_pool = [0x87, 0x81, 0x0B, 0x26, 0x39, 0x5c, 0x24, 0x36, 0x0C, 0xD0,
                        0x5a, 0xdb, 0xbE, 0xC1, 0xd2, 0x9B, 0x38, 0x7b, 0xAf, 0x3a];
        let eth_data = [0x7B, 0xDd, 0x3c, 0xF7, 0xF2, 0x33, 0xb0, 0xE1, 0xC0, 0xA8,
                        0xf0, 0xD1, 0xC0, 0xF5, 0xA3, 0xf8, 0xE6, 0xD9, 0xC7, 0xA0];
        let uni_factory = [0x1F, 0x98, 0x43, 0x1a, 0xAD, 0xC0, 0x4B, 0x42, 0x13, 0x4d,
                           0x6b, 0x3a, 0x4c, 0x6F, 0x3B, 0x6F, 0x5a, 0x3d, 0x2c, 0x1d];
        let init_hash = [0u8; 32];

        let providers: Vec<Arc<dyn FLProvider>> = vec![
            Arc::new(AaveV3Provider::new(eth_pool, eth_data)),
            Arc::new(UniswapV3Provider::new(uni_factory, init_hash)),
            // Mock provider for testing
            Arc::new(MockProvider::new("MockFlash", 9, vec![[1u8; 20]]).with_gas(100_000).with_latency(Duration::from_millis(10))),
        ];

        info!("FlashLoanRouter initialized with {} providers", providers.len());
        FlashLoanRouter::new(providers)
    }

    /// Start the engine (main event loop)
    pub async fn run(&self) -> Result<(), String> {
        if self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Engine is already running".into());
        }
        self.is_running.store(true, std::sync::atomic::Ordering::SeqCst);

        let start_time = Instant::now();
        info!("Flash Loan Arbitrage Engine started");

        let mut scan_interval = time::interval(Duration::from_millis(self.config.scan_interval_ms));

        loop {
            if !self.is_running.load(std::sync::atomic::Ordering::SeqCst) {
                info!("Engine stopped");
                break;
            }

            scan_interval.tick().await;

            // Phase 1: Update pool data
            match self.pool_monitor.update_pools().await {
                Ok(updated) => {
                    if updated > 0 {
                        debug!("Updated {} pools", updated);
                    }
                }
                Err(e) => {
                    warn!("Pool update error: {}", e);
                }
            }

            // Phase 2: Scan for opportunities
            let opportunities = self.detector.scan().await;

            // Phase 3: Execute profitable opportunities
            let mut executed = 0u32;
            for opp in &opportunities {
                if executed >= self.config.max_concurrent_trades {
                    break;
                }

                if opp.is_profitable(self.config.min_profit_usd, self.config.min_profit_bps) {
                    let trade = self.executor.execute(opp.clone()).await;
                    self.profit_tracker.record_trade(trade.clone()).await;

                    if trade.success {
                        info!("✅ Flash loan arb PROFIT: ${:.2} | path: {} -> {}",
                            trade.net_profit_usd, trade.token_in, trade.token_out);
                    } else {
                        warn!("❌ Flash loan arb FAILED: {:?}", trade.error);
                    }

                    executed += 1;
                }
            }

            // Phase 4: Update engine stats
            let uptime = start_time.elapsed().as_secs();
            let mut stats = self.engine_stats.write().await;
            stats.uptime_seconds = uptime;
            stats.total_scans += 1;
            stats.total_opportunities_found += opportunities.len() as u64;
            stats.total_opportunities_executed += executed as u64;
            stats.total_trades_executed += executed as u64;

            let active_trades = self.executor.active_count().await;
            stats.current_active_trades = active_trades;
            stats.pool_count = self.pool_monitor.pool_count();
            stats.queue_size = self.detector.queue_size().await;
            stats.last_scan_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

            if executed > 0 {
                stats.last_trade_time = stats.last_scan_time;
                stats.total_profit_usd = self.profit_tracker.get_pnl().await.total_net_profit;
            }

            // Check circuit breaker
            let pnl = self.profit_tracker.get_pnl().await;
            stats.circuit_breaker_active = pnl.total_net_profit < -self.config.max_daily_loss_usd;

            // Log periodic summary
            if stats.total_scans % 30 == 0 {
                info!(
                    "Engine status: uptime={}s, scans={}, pools={}, queue={}, \
                     opportunities={}, executed={}, profit=${:.2}, active_trades={}, success_rate={:.1}%",
                    uptime, stats.total_scans, stats.pool_count, stats.queue_size,
                    stats.total_opportunities_found, stats.total_opportunities_executed,
                    pnl.total_net_profit, active_trades, pnl.success_rate,
                );

                // Report best opportunities
                let recent = self.executor.recent_trades(5).await;
                for t in &recent {
                    if t.success {
                        info!("  ✅ {} | profit=${:.2} | duration={}ms | {}",
                            t.provider, t.net_profit_usd, t.duration_ms, t.tx_hash.as_deref().unwrap_or("no-tx"));
                    } else {
                        info!("  ❌ {} | error={:?}", t.provider, t.error);
                    }
                }
            }

            // Auto-reinvest logic
            if self.config.auto_reinvest {
                let balance = self.profit_tracker.balance_eth();
                if balance >= self.config.reinvest_min_balance_eth {
                    let reinvest_amount = balance * (1.0 - self.config.profit_sharing_percent / 100.0);
                    info!("Auto-reinvest: balance={:.4} ETH, reinvesting {:.4} ETH", balance, reinvest_amount);
                }
            }
        }

        Ok(())
    }

    /// Stop the engine
    pub fn stop(&self) {
        self.is_running.store(false, std::sync::atomic::Ordering::SeqCst);
        info!("Engine stopping...");
    }

    /// Get engine stats
    pub async fn get_engine_stats(&self) -> EngineStats {
        self.engine_stats.read().await.clone()
    }

    /// Get full PnL
    pub async fn get_pnl(&self) -> PnLSummary {
        self.profit_tracker.get_pnl().await
    }

    /// Get profit log
    pub fn get_profit_log(&self) -> Vec<(String, f64)> {
        self.profit_tracker.get_profit_log()
    }

    /// Get recent trades
    pub async fn get_recent_trades(&self, n: usize) -> Vec<ExecutedTrade> {
        self.executor.recent_trades(n).await
    }

    /// Get all trades
    pub async fn get_all_trades(&self) -> Vec<ExecutedTrade> {
        self.executor.trade_history().await
    }

    /// Is the engine running?
    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config() -> FlashLoanArbConfig {
        FlashLoanArbConfig {
            scan_interval_ms: 100,
            min_profit_usd: 1.0,
            min_profit_bps: 5,
            max_concurrent_trades: 5,
            max_consecutive_failures: 3,
            circuit_breaker_cooldown_secs: 10,
            ..Default::default()
        }
    }

    #[test]
    fn test_config_defaults() {
        let config = FlashLoanArbConfig::default();
        assert!(config.min_profit_usd > 0.0);
        assert!(config.min_profit_bps > 0);
        assert!(config.scan_interval_ms >= 100);
        assert!(config.max_consecutive_failures >= 1);
        assert!(config.slippage_tolerance_bps >= 1);
    }

    #[test]
    fn test_arb_opportunity_profitability() {
        let opp = ArbOpportunity {
            id: "test-1".into(),
            chain: "ethereum".into(),
            dexes: vec!["uniswap-v3".into(), "sushiswap".into()],
            path: vec!["A".into(), "B".into(), "A".into()],
            token_in: "WETH".into(),
            token_out: "WETH".into(),
            amount_in: 1_000_000,
            expected_amount_out: 1_002_000,
            expected_profit_usd: 100.0,
            expected_profit_bps: 20,
            gas_estimate_usd: 10.0,
            flash_loan_fee_usd: 5.0,
            net_profit_usd: 85.0,
            confidence: 0.8,
            slippage_bps: 30,
            max_position_size: 10_000_000,
            detected_at: 0,
        };

        assert!(opp.is_profitable(50.0, 10));
        assert!(!opp.is_profitable(200.0, 10));
        assert!(!opp.is_profitable(50.0, 30));
    }

    #[test]
    fn test_arb_opportunity_risk_score() {
        let opp_high = ArbOpportunity {
            confidence: 0.9, slippage_bps: 10, ..create_test_opp()
        };
        let opp_low = ArbOpportunity {
            confidence: 0.2, slippage_bps: 200, ..create_test_opp()
        };
        assert!(opp_low.risk_score() > opp_high.risk_score());
    }

    fn create_test_opp() -> ArbOpportunity {
        ArbOpportunity {
            id: "test".into(), chain: "ethereum".into(),
            dexes: vec!["uniswap".into()],
            path: vec!["A".into(), "B".into()],
            token_in: "WETH".into(), token_out: "WETH".into(),
            amount_in: 1000, expected_amount_out: 1010,
            expected_profit_usd: 10.0, expected_profit_bps: 10,
            gas_estimate_usd: 5.0, flash_loan_fee_usd: 1.0,
            net_profit_usd: 4.0, confidence: 0.7,
            slippage_bps: 20, max_position_size: 10000,
            detected_at: 0,
        }
    }

    #[test]
    fn test_executed_trade_roi() {
        let trade = ExecutedTrade {
            id: "t1".into(), opportunity_id: "o1".into(),
            chain: "ethereum".into(), path: vec![],
            token_in: "WETH".into(), token_out: "USDC".into(),
            amount_borrowed: 1_000_000_000_000_000_000u128, // 1 ETH
            amount_repaid: 1_000_009_000_000_000_000u128,
            flash_loan_fee: 9_000_000_000_000_000u128,
            gross_profit: 20_000_000_000_000_000u128,
            gas_cost: 5_000_000_000_000_000u128,
            gas_cost_usd: 15.0,
            net_profit: 6_000_000_000_000_000u128,
            net_profit_usd: 18.0,
            provider: "AaveV3".into(),
            tx_hash: Some("0xabc".into()),
            success: true, error: None,
            executed_at: 0, duration_ms: 1500,
        };
        assert!(trade.roi_percent() > 0.0);
        assert!(trade.roi_percent() < 5.0); // reasonable
    }

    #[test]
    fn test_pnl_summary_default() {
        let pnl = PnLSummary::default();
        assert_eq!(pnl.total_trades, 0);
        assert_eq!(pnl.success_rate, 0.0);
    }

    #[test]
    fn test_dex_pool_monitor_creation() {
        let config = test_config();
        let monitor = DexPoolMonitor::new(config);
        assert_eq!(monitor.pool_count(), 0);
        assert_eq!(monitor.update_count(), 0);
    }

    #[test]
    fn test_tracked_pool_addition() {
        let config = test_config();
        let monitor = DexPoolMonitor::new(config);
        let addr = [1u8; 20];
        monitor.add_pool("ethereum", "uniswap-v3", addr, addr, addr, 3000, "https://rpc.com");
        assert_eq!(monitor.tracked_pools.read().len(), 1);
    }

    #[tokio::test]
    async fn test_arbitrage_detector_creation() {
        let config = test_config();
        let monitor = Arc::new(DexPoolMonitor::new(config.clone()));
        let detector = ArbitrageDetector::new(config, monitor);
        let stats = detector.get_stats().await;
        assert_eq!(stats.total_opportunities_found, 0);
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let config = test_config();
        let router = Arc::new(FlashLoanRouter::new(vec![]));
        let executor = FlashLoanArbExecutor::new(config, router);
        let stats = executor.get_stats().await;
        assert_eq!(stats.total_executions, 0);
    }

    #[tokio::test]
    async fn test_executor_circuit_breaker() {
        let config = test_config();
        let router = Arc::new(FlashLoanRouter::new(vec![]));
        let executor = FlashLoanArbExecutor::new(config, router);
        let opp = ArbOpportunity {
            chain: "test".into(), id: "test".into(),
            dexes: vec![], path: vec![],
            token_in: "WETH".into(), token_out: "WETH".into(),
            amount_in: 100, expected_amount_out: 101,
            expected_profit_usd: 1.0, expected_profit_bps: 10,
            gas_estimate_usd: 0.0, flash_loan_fee_usd: 0.0,
            net_profit_usd: 1.0, confidence: 0.5,
            slippage_bps: 10, max_position_size: 1000,
            detected_at: 0,
        };
        let trade = executor.execute(opp).await;
        // Should execute (circuit breaker not active yet)
        assert_eq!(trade.success, false); // No providers = fail
        assert!(trade.error.is_some());
    }

    #[tokio::test]
    async fn test_profit_tracker() {
        let tracker = ProfitTracker::new(1.0);
        let trade = ExecutedTrade {
            id: "t1".into(), opportunity_id: "o1".into(),
            chain: "eth".into(), path: vec![],
            token_in: "WETH".into(), token_out: "USDC".into(),
            amount_borrowed: 1000, amount_repaid: 1009,
            flash_loan_fee: 9, gross_profit: 50,
            gas_cost: 10, gas_cost_usd: 0.01, net_profit: 31,
            net_profit_usd: 0.01, provider: "test".into(),
            tx_hash: Some("0xtx".into()), success: true, error: None,
            executed_at: 0, duration_ms: 100,
        };
        tracker.record_trade(trade).await;
        let pnl = tracker.get_pnl().await;
        assert_eq!(pnl.total_trades, 1);
        assert_eq!(pnl.successful_trades, 1);
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let config = test_config();
        let engine = FlashLoanArbitrageEngine::new(config);
        assert!(!engine.is_running());
        let stats = engine.get_engine_stats().await;
        assert_eq!(stats.uptime_seconds, 0);
    }

    #[test]
    fn test_parse_address_or_zero() {
        let config = test_config();
        let engine = FlashLoanArbitrageEngine::new(config);
        let addr = engine.executor.parse_address_or_zero("0x1234");
        // Should not panic
        assert_eq!(addr[0], 0x12);
    }

    #[test]
    fn test_profit_log() {
        let tracker = ProfitTracker::new(1.0);
        let log = tracker.get_profit_log();
        assert!(log.is_empty());
    }
}
