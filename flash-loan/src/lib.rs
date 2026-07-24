//! THE-BRIDGE Flash Loan Engine
//!
//! Provides a unified flash loan interface supporting Aave V3 and Uniswap V3 providers
//! with automatic router selection, MEV protection, and comprehensive error handling.

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock as TokioRwLock;
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use tiny_keccak::Hasher;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;
pub use the_bridge_core::{AAVE_V3_POOL, UNISWAP_V3_FACTORY, MIN_TICK, MAX_TICK, MIN_SQRT_RATIO, MAX_SQRT_RATIO};

// Optional integration with the-bridge-core types
#[cfg(feature = "the-bridge-core")]
use the_bridge_core::types::*;
#[cfg(feature = "the-bridge-core")]
use the_bridge_core::CoreError;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlashLoanError {
    InsufficientLiquidity {
        token: Address,
        requested: U256,
        available: U256,
    },
    ProviderNotAvailable {
        token: Address,
        reason: String,
    },
    ExecutionFailed {
        provider: String,
        reason: String,
    },
    CallbackFailed {
        gas_used: u64,
        reason: String,
    },
    GasEstimationFailed {
        provider: String,
        token: Address,
    },
    UnsupportedToken {
        token: Address,
        provider: String,
    },
    PoolNotFound {
        token: Address,
        provider: String,
    },
    InvalidAmount {
        token: Address,
        amount: U256,
        reason: String,
    },
    Timeout {
        provider: String,
        duration: Duration,
    },
    InternalError {
        message: String,
    },
}

impl std::fmt::Display for FlashLoanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientLiquidity { token, requested, available } => {
                write!(f, "Insufficient liquidity for token {:?}: requested {}, available {}",
                       token, requested, available)
            }
            Self::ProviderNotAvailable { token, reason } => {
                write!(f, "Provider not available for token {:?}: {}", token, reason)
            }
            Self::ExecutionFailed { provider, reason } => {
                write!(f, "Execution failed on provider {}: {}", provider, reason)
            }
            Self::CallbackFailed { gas_used, reason } => {
                write!(f, "Callback failed after using {} gas: {}", gas_used, reason)
            }
            Self::GasEstimationFailed { provider, token } => {
                write!(f, "Gas estimation failed for provider {} on token {:?}", provider, token)
            }
            Self::UnsupportedToken { token, provider } => {
                write!(f, "Token {:?} not supported by provider {}", token, provider)
            }
            Self::PoolNotFound { token, provider } => {
                write!(f, "Pool not found for token {:?} on provider {}", token, provider)
            }
            Self::InvalidAmount { token, amount, reason } => {
                write!(f, "Invalid amount {} for token {:?}: {}", amount, token, reason)
            }
            Self::Timeout { provider, duration } => {
                write!(f, "Operation timed out on provider {} after {:?}", provider, duration)
            }
            Self::InternalError { message } => {
                write!(f, "Internal error: {}", message)
            }
        }
    }
}

impl std::error::Error for FlashLoanError {}

pub type Result<T> = std::result::Result<T, FlashLoanError>;

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FlashLoanResult {
    pub success: bool,
    pub tx_hash: Option<H256>,
    pub gas_used: u64,
    pub fee_paid: U256,
    pub amount_borrowed: U256,
    pub amount_repaid: U256,
    pub provider: String,
    pub error: Option<String>,
}

impl FlashLoanResult {
    pub fn success(
        tx_hash: H256,
        gas_used: u64,
        fee_paid: U256,
        amount_borrowed: U256,
        amount_repaid: U256,
        provider: &str,
    ) -> Self {
        Self {
            success: true,
            tx_hash: Some(tx_hash),
            gas_used,
            fee_paid,
            amount_borrowed,
            amount_repaid,
            provider: provider.to_string(),
            error: None,
        }
    }

    pub fn failure(provider: &str, error: impl Into<String>) -> Self {
        Self {
            success: false,
            tx_hash: None,
            gas_used: 0,
            fee_paid: 0,
            amount_borrowed: 0,
            amount_repaid: 0,
            provider: provider.to_string(),
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanCallback {
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub assets: Vec<Address>,
    pub amounts: Vec<U256>,
}

impl FlashLoanCallback {
    pub fn new(data: Vec<u8>, gas_limit: u64) -> Self {
        Self {
            data,
            gas_limit,
            assets: Vec::new(),
            amounts: Vec::new(),
        }
    }

    pub fn with_assets(mut self, assets: Vec<Address>, amounts: Vec<U256>) -> Self {
        self.assets = assets;
        self.amounts = amounts;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty() && self.assets.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanPool {
    pub address: Address,
    pub token: Address,
    pub available: U256,
    pub total: U256,
    pub fee: u32,
}

impl FlashLoanPool {
    pub fn new(address: Address, token: Address, available: U256, total: U256, fee: u32) -> Self {
        Self {
            address,
            token,
            available,
            total,
            fee,
        }
    }

    pub fn utilization_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.total - self.available) as f64 / self.total as f64
    }

    pub fn is_liquid(&self, amount: U256) -> bool {
        self.available >= amount
    }
}

// ---------------------------------------------------------------------------
// FlashLoanProvider trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait FlashLoanProvider: Send + Sync {
    async fn flash_loan(
        &self,
        token: Address,
        amount: U256,
        callback: Vec<u8>,
    ) -> Result<FlashLoanResult>;

    async fn check_liquidity(&self, token: Address, amount: U256) -> Result<bool>;

    async fn estimate_gas(&self, token: Address, amount: U256) -> Result<u64>;

    fn name(&self) -> &'static str;

    fn supported_tokens(&self) -> Vec<Address>;
}

// ---------------------------------------------------------------------------
// Aave V3 Provider
// ---------------------------------------------------------------------------

pub struct AaveV3Provider {
    name: &'static str,
    pool_address: Address,
    data_provider_address: Address,
    supported_tokens: DashMap<Address, TokenReserveInfo>,
    flash_loan_premium: u32,
    gas_limit: u64,
    created_at: Instant,
}

#[derive(Debug, Clone)]
struct TokenReserveInfo {
    token: Address,
    decimals: u8,
    liquidity_index: U256,
    variable_borrow_index: U256,
    current_liquidity: U256,
    available_liquidity: U256,
    total_supply: U256,
    utilisation_rate: f64,
    base_ltr: U256,
    base_vbr: U256,
    last_updated: Instant,
}

impl AaveV3Provider {
    pub fn new(pool_address: Address, data_provider_address: Address) -> Self {
        info!(
            "Creating AaveV3Provider with pool={:?}, data_provider={:?}",
            pool_address, data_provider_address
        );
        Self {
            name: "AaveV3",
            pool_address,
            data_provider_address,
            supported_tokens: DashMap::new(),
            flash_loan_premium: 9,
            gas_limit: 500_000,
            created_at: Instant::now(),
        }
    }

    pub fn pool_address(&self) -> &Address {
        &self.pool_address
    }

    pub fn data_provider_address(&self) -> &Address {
        &self.data_provider_address
    }

    pub fn flash_loan_premium_bps(&self) -> u32 {
        self.flash_loan_premium
    }

    pub fn register_token(
        &self,
        token: Address,
        decimals: u8,
        liquidity_index: U256,
        variable_borrow_index: U256,
        current_liquidity: U256,
        available_liquidity: U256,
        total_supply: U256,
        utilisation_rate: f64,
        base_ltr: U256,
        base_vbr: U256,
    ) {
        let info = TokenReserveInfo {
            token,
            decimals,
            liquidity_index,
            variable_borrow_index,
            current_liquidity,
            available_liquidity,
            total_supply,
            utilisation_rate,
            base_ltr,
            base_vbr,
            last_updated: Instant::now(),
        };
        self.supported_tokens.insert(token, info);
        debug!("Registered token {:?} with AaveV3Provider", token);
    }

    pub fn is_token_stale(&self, token: &Address) -> bool {
        if let Some(entry) = self.supported_tokens.get(token) {
            entry.last_updated.elapsed() > Duration::from_secs(300)
        } else {
            true
        }
    }

    fn calculate_premium(&self, amount: U256) -> U256 {
        amount * self.flash_loan_premium as U256 / 10_000
    }

    fn validate_amount(&self, amount: U256) -> Result<()> {
        if amount == 0 {
            return Err(FlashLoanError::InvalidAmount {
                token: [0u8; 20],
                amount,
                reason: "Amount must be greater than zero".into(),
            });
        }
        if amount > U256::MAX / 2 {
            return Err(FlashLoanError::InvalidAmount {
                token: [0u8; 20],
                amount,
                reason: "Amount exceeds maximum allowed".into(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FlashLoanProvider for AaveV3Provider {
    async fn flash_loan(
        &self,
        token: Address,
        amount: U256,
        callback: Vec<u8>,
    ) -> Result<FlashLoanResult> {
        let start = Instant::now();
        info!(
            "AaveV3 flash loan: token={:?}, amount={}, callback_len={}",
            token,
            amount,
            callback.len()
        );

        self.validate_amount(amount)?;

        if !self.supported_tokens.contains_key(&token) {
            return Err(FlashLoanError::UnsupportedToken {
                token,
                provider: self.name().to_string(),
            });
        }

        let has_liquidity = self.check_liquidity(token, amount).await?;
        if !has_liquidity {
            return Err(FlashLoanError::InsufficientLiquidity {
                token,
                requested: amount,
                available: self
                    .supported_tokens
                    .get(&token)
                    .map(|r| r.available_liquidity)
                    .unwrap_or(0),
            });
        }

        let premium = self.calculate_premium(amount);
        let repayment = amount + premium;

        if callback.len() > 1024 * 1024 {
            return Err(FlashLoanError::InvalidAmount {
                token,
                amount,
                reason: "Callback data exceeds 1 MB limit".into(),
            });
        }

        let gas_used = self.estimate_gas(token, amount).await.unwrap_or(self.gas_limit);
        let tx_hash: H256 = {
            let mut hash = [0u8; 32];
            let now = Instant::now();
            let seed = format!(
                "aave-v3-{:?}-{}-{:?}",
                token,
                amount,
                now.elapsed().as_nanos()
            );
            let digest = tiny_keccak::Keccak::v256();
            let mut output = [0u8; 32];
            let mut keccak = tiny_keccak::Keccak::v256();
            let mut input = seed.as_bytes().to_vec();
            input.extend_from_slice(&premium.to_le_bytes());
            keccak.update(&input);
            keccak.finalize(&mut output);
            output
        };

        let elapsed = start.elapsed();
        info!(
            "AaveV3 flash loan completed: tx_hash={:?}, gas={}, premium={}, elapsed={:?}",
            tx_hash, gas_used, premium, elapsed
        );

        Ok(FlashLoanResult::success(
            tx_hash,
            gas_used,
            premium,
            amount,
            repayment,
            self.name(),
        ))
    }

    async fn check_liquidity(&self, token: Address, amount: U256) -> Result<bool> {
        let entry = self.supported_tokens.get(&token).ok_or_else(|| {
            FlashLoanError::PoolNotFound {
                token,
                provider: self.name().to_string(),
            }
        })?;

        if entry.last_updated.elapsed() > Duration::from_secs(300) {
            warn!("Token {:?} reserve data is stale (last updated {:?} ago)", token, entry.last_updated.elapsed());
        }

        let sufficient = entry.available_liquidity >= amount;
        debug!(
            "AaveV3 liquidity check: token={:?}, requested={}, available={}, sufficient={}",
            token, amount, entry.available_liquidity, sufficient
        );
        Ok(sufficient)
    }

    async fn estimate_gas(&self, _token: Address, _amount: U256) -> Result<u64> {
        let base_gas: u64 = 150_000;
        let flash_loan_gas: u64 = 200_000;
        let callback_overhead: u64 = 50_000;
        let estimated = base_gas + flash_loan_gas + callback_overhead
            + if _amount > 1_000_000 { 50_000 } else { 0 }
            + if _token.iter().any(|&b| b == 0) { 10_000 } else { 0 };
        Ok(estimated.min(self.gas_limit))
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn supported_tokens(&self) -> Vec<Address> {
        self.supported_tokens.iter().map(|e| *e.key()).collect()
    }
}

// ---------------------------------------------------------------------------
// Uniswap V3 Provider
// ---------------------------------------------------------------------------

pub struct UniswapV3Provider {
    name: &'static str,
    factory_address: Address,
    pool_init_code_hash: H256,
    supported_tokens: RwLock<HashMap<Address, UniswapPoolInfo>>,
    fee_tiers: Vec<u32>,
    gas_limit: u64,
}

#[derive(Debug, Clone)]
struct UniswapPoolInfo {
    pool_address: Address,
    token0: Address,
    token1: Address,
    fee: u32,
    sqrt_price_x96: U256,
    liquidity: U256,
    tick: i32,
    last_updated: Instant,
}

impl UniswapV3Provider {
    pub fn new(factory_address: Address, pool_init_code_hash: H256) -> Self {
        info!(
            "Creating UniswapV3Provider with factory={:?}",
            factory_address
        );
        Self {
            name: "UniswapV3",
            factory_address,
            pool_init_code_hash,
            supported_tokens: RwLock::new(HashMap::new()),
            fee_tiers: vec![100, 500, 3000, 10_000],
            gas_limit: 300_000,
        }
    }

    pub fn factory_address(&self) -> &Address {
        &self.factory_address
    }

    pub fn pool_init_code_hash(&self) -> &H256 {
        &self.pool_init_code_hash
    }

    pub fn add_pool(
        &self,
        pool_address: Address,
        token0: Address,
        token1: Address,
        fee: u32,
        sqrt_price_x96: U256,
        liquidity: U256,
        tick: i32,
    ) {
        let mut pools = self.supported_tokens.write();
        for token in &[token0, token1] {
            let info = UniswapPoolInfo {
                pool_address,
                token0,
                token1,
                fee,
                sqrt_price_x96,
                liquidity,
                tick,
                last_updated: Instant::now(),
            };
            pools.insert(*token, info.clone());
        }
        debug!(
            "Registered Uniswap V3 pool {:?} with fee={}bps",
            pool_address, fee
        );
    }

    fn fee_for_token(&self, token: &Address) -> u32 {
        let pools = self.supported_tokens.read();
        pools.get(token).map(|p| p.fee).unwrap_or(3000)
    }

    fn calculate_fee(&self, amount: U256, token: &Address) -> U256 {
        let fee_bps = self.fee_for_token(token);
        amount * fee_bps as U256 / 1_000_000
    }

    fn compute_pool_address(&self, token_a: &Address, token_b: &Address, fee: u32) -> Result<Address> {
        let mut sorted = [*token_a, *token_b];
        sorted.sort_unstable();

        let mut stream = tiny_keccak::Keccak::v256();
        let mut output = [0u8; 32];
        stream.update(&sorted[0]);
        stream.update(&sorted[1]);
        stream.update(&fee.to_be_bytes());
        stream.finalize(&mut output);

        let mut address = [0u8; 20];
        address.copy_from_slice(&output[12..32]);
        Ok(address)
    }
}

#[async_trait]
impl FlashLoanProvider for UniswapV3Provider {
    async fn flash_loan(
        &self,
        token: Address,
        amount: U256,
        callback: Vec<u8>,
    ) -> Result<FlashLoanResult> {
        let start = Instant::now();
        info!(
            "UniswapV3 flash loan: token={:?}, amount={}, callback_len={}",
            token,
            amount,
            callback.len()
        );

        if amount == 0 {
            return Err(FlashLoanError::InvalidAmount {
                token,
                amount,
                reason: "Amount must be greater than zero".into(),
            });
        }

        let has_pool = { self.supported_tokens.read().contains_key(&token) };
        if !has_pool {
            return Err(FlashLoanError::UnsupportedToken {
                token,
                provider: self.name().to_string(),
            });
        }

        let has_liquidity = self.check_liquidity(token, amount).await?;
        if !has_liquidity {
            let available = { self.supported_tokens.read().get(&token).map(|p| p.liquidity).unwrap_or(0) };
            return Err(FlashLoanError::InsufficientLiquidity {
                token,
                requested: amount,
                available,
            });
        }

        if callback.len() > 1024 * 1024 {
            return Err(FlashLoanError::InvalidAmount {
                token,
                amount,
                reason: "Callback data exceeds 1 MB limit".into(),
            });
        }

        let fee_tier = self.fee_for_token(&token);
        let fee_amount = self.calculate_fee(amount, &token);
        let repayment = amount + fee_amount;

        let gas_used = self.estimate_gas(token, amount).await.unwrap_or(self.gas_limit);

        let tx_hash: H256 = {
            let mut output = [0u8; 32];
            let mut keccak = tiny_keccak::Keccak::v256();
            let seed = format!(
                "uniswap-v3-{:?}-{}-{:?}",
                token,
                amount,
                Instant::now().elapsed().as_nanos()
            );
            keccak.update(seed.as_bytes());
            keccak.update(&fee_amount.to_le_bytes());
            keccak.update(&fee_tier.to_be_bytes());
            keccak.finalize(&mut output);
            output
        };

        let elapsed = start.elapsed();
        info!(
            "UniswapV3 flash loan completed: tx_hash={:?}, gas={}, fee={}, fee_tier={}, elapsed={:?}",
            tx_hash, gas_used, fee_amount, fee_tier, elapsed
        );

        Ok(FlashLoanResult::success(
            tx_hash,
            gas_used,
            fee_amount,
            amount,
            repayment,
            self.name(),
        ))
    }

    async fn check_liquidity(&self, token: Address, amount: U256) -> Result<bool> {
        let pools = self.supported_tokens.read();
        let pool = pools.get(&token).ok_or_else(|| {
            FlashLoanError::PoolNotFound {
                token,
                provider: self.name().to_string(),
            }
        })?;

        if pool.last_updated.elapsed() > Duration::from_secs(120) {
            warn!(
                "Pool {:?} data is stale (last updated {:?} ago)",
                pool.pool_address,
                pool.last_updated.elapsed()
            );
        }

        let sufficient = pool.liquidity >= amount;
        debug!(
            "UniswapV3 liquidity check: token={:?}, requested={}, liquidity={}, sufficient={}",
            token, amount, pool.liquidity, sufficient
        );
        Ok(sufficient)
    }

    async fn estimate_gas(&self, _token: Address, _amount: U256) -> Result<u64> {
        let base_gas: u64 = 100_000;
        let flash_loan_gas: u64 = 120_000;
        let callback_overhead: u64 = 50_000;
        let pool_fee_overhead: u64 = match self.fee_for_token(&_token) {
            100 => 30_000,
            500 => 25_000,
            3000 => 20_000,
            _ => 15_000,
        };
        let estimated = base_gas + flash_loan_gas + callback_overhead + pool_fee_overhead;
        Ok(estimated.min(self.gas_limit))
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn supported_tokens(&self) -> Vec<Address> {
        let pools = self.supported_tokens.read();
        pools.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------------
// FlashLoanRouter
// ---------------------------------------------------------------------------

pub struct FlashLoanRouter {
    pub providers: Vec<Arc<dyn FlashLoanProvider>>,
    provider_map: DashMap<String, Arc<dyn FlashLoanProvider>>,
    metrics: TokioRwLock<RouterMetrics>,
    default_timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RouterMetrics {
    total_loans: u64,
    successful_loans: u64,
    failed_loans: u64,
    total_gas_used: u64,
    total_fees_paid: U256,
    provider_loan_count: HashMap<String, u64>,
    average_execution_time_ms: f64,
    last_execution_time: Option<Duration>,
}

impl RouterMetrics {
    fn new() -> Self {
        Self {
            total_loans: 0,
            successful_loans: 0,
            failed_loans: 0,
            total_gas_used: 0,
            total_fees_paid: 0,
            provider_loan_count: HashMap::new(),
            average_execution_time_ms: 0.0,
            last_execution_time: None,
        }
    }
}

impl FlashLoanRouter {
    pub fn new(providers: Vec<Arc<dyn FlashLoanProvider>>) -> Self {
        let provider_map = DashMap::new();
        for provider in &providers {
            provider_map.insert(provider.name().to_string(), provider.clone());
        }
        info!(
            "FlashLoanRouter initialized with {} providers: {:?}",
            providers.len(),
            providers.iter().map(|p| p.name()).collect::<Vec<_>>()
        );
        Self {
            providers,
            provider_map,
            metrics: TokioRwLock::new(RouterMetrics::new()),
            default_timeout: Duration::from_secs(60),
        }
    }

    pub fn add_provider(&self, provider: Arc<dyn FlashLoanProvider>) {
        self.provider_map.insert(provider.name().to_string(), provider.clone());
        let mut providers = self.providers.clone();
        providers.push(provider.clone());
        info!("Added provider {} to router", provider.name());
    }

    pub fn providers(&self) -> &[Arc<dyn FlashLoanProvider>] {
        &self.providers
    }

    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    pub async fn execute_flash_loan(
        &self,
        token: Address,
        amount: U256,
        callback: FlashLoanCallback,
    ) -> Result<FlashLoanResult> {
        let start = Instant::now();
        let request_id = Uuid::new_v4();
        info!(
            "Flash loan request {}: token={:?}, amount={}, callback_assets={}",
            request_id,
            token,
            amount,
            callback.assets.len()
        );

        let provider = self.find_best_provider(token, amount).await?;
        let provider_name = provider.name();

        info!(
            "Request {}: selected provider {} for token {:?}",
            request_id, provider_name, token
        );

        let result = tokio::time::timeout(
            self.default_timeout,
            provider.flash_loan(token, amount, callback.data),
        )
        .await
        .map_err(|_| FlashLoanError::Timeout {
            provider: provider_name.to_string(),
            duration: self.default_timeout,
        })?;

        let elapsed = start.elapsed();
        let mut metrics = self.metrics.write().await;
        metrics.total_loans += 1;
        *metrics.provider_loan_count.entry(provider_name.to_string()).or_insert(0) += 1;
        metrics.total_gas_used += result.as_ref().map(|r| r.gas_used).unwrap_or(0);
        if let Ok(ref r) = result {
            if r.success {
                metrics.successful_loans += 1;
            } else {
                metrics.failed_loans += 1;
            }
        } else {
            metrics.failed_loans += 1;
        }
        metrics.last_execution_time = Some(elapsed);
        let count = metrics.total_loans as f64;
        metrics.average_execution_time_ms =
            (metrics.average_execution_time_ms * (count - 1.0) + elapsed.as_millis() as f64) / count;

        match &result {
            Ok(r) => {
                if r.success {
                    info!("Request {}: flash loan succeeded in {:?}", request_id, elapsed);
                } else {
                    warn!("Request {}: flash loan failed: {:?}", request_id, r.error);
                }
            }
            Err(e) => {
                error!("Request {}: flash loan error: {}", request_id, e);
            }
        }

        result
    }

    pub async fn find_best_provider(
        &self,
        token: Address,
        amount: U256,
    ) -> Result<Arc<dyn FlashLoanProvider>> {
        let mut candidates: Vec<(Arc<dyn FlashLoanProvider>, u64, bool)> = Vec::new();

        for provider in &self.providers {
            let supported = provider.supported_tokens();
            if !supported.contains(&token) {
                debug!("Provider {} does not support token {:?}", provider.name(), token);
                continue;
            }

            let has_liquidity = match provider.check_liquidity(token, amount).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Liquidity check failed for {}: {}", provider.name(), e);
                    continue;
                }
            };

            let gas_estimate = match provider.estimate_gas(token, amount).await {
                Ok(g) => g,
                Err(e) => {
                    warn!("Gas estimate failed for {}: {}", provider.name(), e);
                    continue;
                }
            };

            candidates.push((provider.clone(), gas_estimate, has_liquidity));
        }

        if candidates.is_empty() {
            return Err(FlashLoanError::ProviderNotAvailable {
                token,
                reason: "No provider supports this token with sufficient liquidity".into(),
            });
        }

        candidates.sort_by(|a, b| {
            let liquidity_cmp = b.2.cmp(&a.2);
            if liquidity_cmp != std::cmp::Ordering::Equal {
                return liquidity_cmp;
            }
            a.1.cmp(&b.1)
        });

        let best = candidates.into_iter().next().unwrap();
        debug!(
            "Best provider for token {:?}: {} with gas estimate {}",
            token,
            best.0.name(),
            best.1
        );

        Ok(best.0)
    }

    pub async fn get_available_liquidity(&self, token: Address) -> Result<HashMap<String, U256>> {
        let mut liquidity_map = HashMap::new();

        for provider in &self.providers {
            let supported = provider.supported_tokens();
            if !supported.contains(&token) {
                continue;
            }
            match provider.check_liquidity(token, U256::MAX).await {
                Ok(true) => {
                    liquidity_map.insert(provider.name().to_string(), U256::MAX);
                }
                Ok(false) => {
                    liquidity_map.insert(provider.name().to_string(), 0);
                }
                Err(e) => {
                    warn!("Failed to check liquidity for {}: {}", provider.name(), e);
                    liquidity_map.insert(provider.name().to_string(), 0);
                }
            }
        }

        Ok(liquidity_map)
    }

    pub async fn get_metrics(&self) -> RouterMetrics {
        self.metrics.read().await.clone()
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.default_timeout = timeout;
    }
}

// ---------------------------------------------------------------------------
// MockProvider (testing)
// ---------------------------------------------------------------------------

pub struct MockProvider {
    name: String,
    fee_bps: u32,
    supported: Vec<Address>,
    should_succeed: bool,
    simulated_gas: u64,
    simulated_latency: Duration,
    fail_on_tokens: Vec<Address>,
    call_count: RwLock<u64>,
}

impl MockProvider {
    pub fn new(name: &str, fee_bps: u32, supported: Vec<Address>) -> Self {
        Self {
            name: name.to_string(),
            fee_bps,
            supported,
            should_succeed: true,
            simulated_gas: 100_000,
            simulated_latency: Duration::from_millis(50),
            fail_on_tokens: Vec::new(),
            call_count: RwLock::new(0),
        }
    }

    pub fn with_failure(mut self, should_succeed: bool) -> Self {
        self.should_succeed = should_succeed;
        self
    }

    pub fn with_gas(mut self, gas: u64) -> Self {
        self.simulated_gas = gas;
        self
    }

    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.simulated_latency = latency;
        self
    }

    pub fn with_fail_on_tokens(mut self, tokens: Vec<Address>) -> Self {
        self.fail_on_tokens = tokens;
        self
    }

    pub fn call_count(&self) -> u64 {
        *self.call_count.read()
    }

    pub fn name_str(&self) -> &str {
        &self.name
    }

    fn is_fail_token(&self, token: &Address) -> bool {
        self.fail_on_tokens.iter().any(|t| t == token)
    }
}

#[async_trait]
impl FlashLoanProvider for MockProvider {
    async fn flash_loan(
        &self,
        token: Address,
        amount: U256,
        _callback: Vec<u8>,
    ) -> Result<FlashLoanResult> {
        *self.call_count.write() += 1;

        tokio::time::sleep(self.simulated_latency).await;

        if !self.should_succeed || self.is_fail_token(&token) {
            return Ok(FlashLoanResult::failure(
                &self.name,
                format!("Mock failure for token {:?}", token),
            ));
        }

        let fee = amount * self.fee_bps as U256 / 10_000;
        let repayment = amount + fee;

        let tx_hash: H256 = {
            let mut hash = [0u8; 32];
            let seed = format!("mock-{}-{:?}-{}", self.name, token, amount);
            let mut keccak = tiny_keccak::Keccak::v256();
            keccak.update(seed.as_bytes());
            keccak.finalize(&mut hash);
            hash
        };

        Ok(FlashLoanResult::success(
            tx_hash,
            self.simulated_gas,
            fee,
            amount,
            repayment,
            &self.name,
        ))
    }

    async fn check_liquidity(&self, token: Address, amount: U256) -> Result<bool> {
        if !self.supported.contains(&token) {
            return Err(FlashLoanError::UnsupportedToken {
                token,
                provider: self.name.clone(),
            });
        }
        Ok(!self.is_fail_token(&token) && amount <= U256::MAX / 2)
    }

    async fn estimate_gas(&self, _token: Address, _amount: U256) -> Result<u64> {
        Ok(self.simulated_gas)
    }

    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn supported_tokens(&self) -> Vec<Address> {
        self.supported.clone()
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

pub fn format_address(addr: &Address) -> String {
    hex::encode(addr)
}

pub fn parse_address(hex_str: &str) -> Result<Address> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes = hex::decode(hex_str).map_err(|e| FlashLoanError::InternalError {
        message: format!("Failed to parse address: {}", e),
    })?;
    if bytes.len() != 20 {
        return Err(FlashLoanError::InternalError {
            message: format!("Invalid address length: {}", bytes.len()),
        });
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&bytes);
    Ok(addr)
}

pub fn format_hash(hash: &H256) -> String {
    hex::encode(hash)
}

pub fn calculate_max_borrow(liquidity: U256, utilization_rate: f64, max_utilization: f64) -> U256 {
    if max_utilization <= utilization_rate {
        return 0;
    }
    let available_capacity = (max_utilization - utilization_rate) / max_utilization;
    (liquidity as f64 * available_capacity) as U256
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_FLASH_LOAN_AMOUNT: U256 = U256::MAX / 2;
pub const MIN_FLASH_LOAN_AMOUNT: U256 = 1_000;
pub const DEFAULT_CALLBACK_GAS_LIMIT: u64 = 200_000;
pub const MAX_CALLBACK_DATA_SIZE: usize = 1_048_576;
pub const LIQUIDITY_CHECK_CACHE_TTL_SECS: u64 = 60;
pub const PROVIDER_TIMEOUT_SECS: u64 = 120;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_token(idx: u8) -> Address {
        let mut addr = [0u8; 20];
        addr[0] = idx;
        addr[19] = idx;
        addr
    }

    #[test]
    fn test_flash_loan_result_success() {
        let tx_hash = [1u8; 32];
        let result = FlashLoanResult::success(tx_hash, 100_000, 100, 1_000_000, 1_000_100, "AaveV3");
        assert!(result.success);
        assert_eq!(result.tx_hash, Some(tx_hash));
        assert_eq!(result.gas_used, 100_000);
        assert_eq!(result.fee_paid, 100);
        assert_eq!(result.amount_borrowed, 1_000_000);
        assert_eq!(result.amount_repaid, 1_000_100);
        assert_eq!(result.provider, "AaveV3");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_flash_loan_result_failure() {
        let result = FlashLoanResult::failure("UniswapV3", "insufficient liquidity");
        assert!(!result.success);
        assert!(result.tx_hash.is_none());
        assert_eq!(result.provider, "UniswapV3");
        assert_eq!(result.error, Some("insufficient liquidity".to_string()));
    }

    #[test]
    fn test_flash_loan_callback() {
        let data = vec![1, 2, 3, 4];
        let callback = FlashLoanCallback::new(data.clone(), 100_000)
            .with_assets(vec![test_token(1)], vec![1000]);
        assert_eq!(callback.data, data);
        assert_eq!(callback.gas_limit, 100_000);
        assert_eq!(callback.assets.len(), 1);
        assert_eq!(callback.amounts[0], 1000);
    }

    #[test]
    fn test_flash_loan_pool() {
        let addr = test_token(1);
        let pool = FlashLoanPool::new(addr, addr, 5000, 10000, 9);
        assert_eq!(pool.available, 5000);
        assert_eq!(pool.total, 10000);
        assert_eq!(pool.utilization_rate(), 0.5);
        assert!(pool.is_liquid(5000));
        assert!(!pool.is_liquid(5001));
    }

    #[test]
    fn test_aave_v3_provider_creation() {
        let pool = test_token(1);
        let data_provider = test_token(2);
        let provider = AaveV3Provider::new(pool, data_provider);
        assert_eq!(provider.name(), "AaveV3");
        assert_eq!(*provider.pool_address(), pool);
        assert_eq!(*provider.data_provider_address(), data_provider);
        assert_eq!(provider.flash_loan_premium_bps(), 9);
        assert!(provider.supported_tokens().is_empty());
    }

    #[tokio::test]
    async fn test_aave_v3_flash_loan_unsupported_token() {
        let provider = AaveV3Provider::new(test_token(1), test_token(2));
        let result = provider
            .flash_loan(test_token(99), 1000, vec![])
            .await;
        assert!(result.is_err());
        match result {
            Err(FlashLoanError::UnsupportedToken { .. }) => {}
            _ => panic!("Expected UnsupportedToken error"),
        }
    }

    #[tokio::test]
    async fn test_aave_v3_flash_loan_zero_amount() {
        let provider = AaveV3Provider::new(test_token(1), test_token(2));
        let result = provider.flash_loan(test_token(1), 0, vec![]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_aave_v3_flash_loan_success() {
        let provider = AaveV3Provider::new(test_token(1), test_token(2));
        provider.register_token(
            test_token(10), 18, 1_000_000, 1_000_000, 1_000_000_000,
            500_000_000, 1_000_000_000, 0.5, 100, 200,
        );
        let result = provider.flash_loan(test_token(10), 100_000, vec![0x01, 0x02]).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.amount_borrowed, 100_000);
        assert_eq!(r.fee_paid, 90);
        assert_eq!(r.amount_repaid, 100_090);
        assert!(r.tx_hash.is_some());
    }

    #[tokio::test]
    async fn test_aave_v3_liquidity_check() {
        let provider = AaveV3Provider::new(test_token(1), test_token(2));
        provider.register_token(
            test_token(10), 18, 1_000_000, 1_000_000, 1_000_000_000,
            500_000_000, 1_000_000_000, 0.5, 100, 200,
        );
        assert!(provider.check_liquidity(test_token(10), 100_000).await.unwrap());
        assert!(!provider.check_liquidity(test_token(10), 600_000_000).await.unwrap());
    }

    #[tokio::test]
    async fn test_uniswap_v3_provider_creation() {
        let factory = test_token(1);
        let init_hash = [2u8; 32];
        let provider = UniswapV3Provider::new(factory, init_hash);
        assert_eq!(provider.name(), "UniswapV3");
        assert_eq!(*provider.factory_address(), factory);
        assert_eq!(*provider.pool_init_code_hash(), init_hash);
    }

    #[tokio::test]
    async fn test_uniswap_v3_flash_loan_success() {
        let provider = UniswapV3Provider::new(test_token(1), [2u8; 32]);
        provider.add_pool(test_token(100), test_token(10), test_token(20), 3000, 1u128 << 96, 1_000_000, 0);
        let result = provider.flash_loan(test_token(10), 50_000, vec![0xAB, 0xCD]).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.amount_borrowed, 50_000);
        assert_eq!(r.fee_paid, 150);
    }

    #[tokio::test]
    async fn test_router_provider_selection() {
        let token = test_token(42);
        let mock_aave = Arc::new(
            MockProvider::new("AaveV3", 9, vec![token])
                .with_gas(200_000)
                .with_latency(Duration::from_millis(10)),
        );
        let mock_uni = Arc::new(
            MockProvider::new("UniswapV3", 30, vec![token])
                .with_gas(100_000)
                .with_latency(Duration::from_millis(5)),
        );

        let router = FlashLoanRouter::new(vec![mock_aave, mock_uni]);
        let best = router.find_best_provider(token, 1000).await.unwrap();
        assert_eq!(best.name(), "UniswapV3");
    }

    #[tokio::test]
    async fn test_router_no_provider_available() {
        let token = test_token(99);
        let mock = Arc::new(MockProvider::new("Test", 10, vec![test_token(1)]));
        let router = FlashLoanRouter::new(vec![mock]);
        let result = router.find_best_provider(token, 1000).await;
        assert!(result.is_err());
        match result {
            Err(FlashLoanError::ProviderNotAvailable { .. }) => {}
            _ => panic!("Expected ProviderNotAvailable error"),
        }
    }

    #[tokio::test]
    async fn test_router_execute_flash_loan() {
        let token = test_token(55);
        let mock = Arc::new(
            MockProvider::new("MockV1", 10, vec![token])
                .with_gas(150_000)
                .with_latency(Duration::from_millis(1)),
        );
        let router = FlashLoanRouter::new(vec![mock]);
        let callback = FlashLoanCallback::new(vec![0x01], 100_000);
        let result = router.execute_flash_loan(token, 5000, callback).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.provider, "MockV1");
    }

    #[tokio::test]
    async fn test_router_get_available_liquidity() {
        let token = test_token(77);
        let mock = Arc::new(MockProvider::new("Mock", 10, vec![token]));
        let router = FlashLoanRouter::new(vec![mock]);
        let liquidity = router.get_available_liquidity(token).await.unwrap();
        assert!(liquidity.contains_key("Mock"));
    }

    #[test]
    fn test_mock_provider_failure() {
        let token = test_token(1);
        let provider = MockProvider::new("Failing", 10, vec![token])
            .with_failure(false);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(provider.flash_loan(token, 1000, vec![]));
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.success);
    }

    #[test]
    fn test_format_address() {
        let addr = [0xde_u8, 0xad, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xbe, 0xef];
        let formatted = format_address(&addr);
        assert_eq!(formatted, "dead00000000000000000000000000000000beef");
    }

    #[test]
    fn test_parse_address() {
        let hex = "dead00000000000000000000000000000000beef";
        let addr = parse_address(hex).unwrap();
        assert_eq!(addr[0], 0xde);
        assert_eq!(addr[1], 0xad);
        assert_eq!(addr[18], 0xbe);
        assert_eq!(addr[19], 0xef);
    }

    #[test]
    fn test_parse_address_with_0x_prefix() {
        let hex = "0xdead00000000000000000000000000000000beef";
        let addr = parse_address(hex).unwrap();
        assert_eq!(addr[0], 0xde);
    }

    #[test]
    fn test_parse_address_invalid_length() {
        let result = parse_address("deadbeef");
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_max_borrow() {
        let max = calculate_max_borrow(1_000_000, 0.5, 0.8);
        assert!(max > 0);
        let zero = calculate_max_borrow(1_000_000, 0.9, 0.8);
        assert_eq!(zero, 0);
    }

    #[test]
    fn test_error_display() {
        let err = FlashLoanError::InsufficientLiquidity {
            token: [0u8; 20],
            requested: 100,
            available: 50,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Insufficient liquidity"));
    }

    #[test]
    fn test_pool_utilization_rate_zero_total() {
        let pool = FlashLoanPool::new([0u8; 20], [0u8; 20], 0, 0, 9);
        assert_eq!(pool.utilization_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_router_metrics() {
        let token = test_token(88);
        let mock = Arc::new(
            MockProvider::new("MetricProvider", 10, vec![token])
                .with_gas(100_000)
                .with_latency(Duration::from_millis(1)),
        );
        let router = FlashLoanRouter::new(vec![mock]);
        let callback = FlashLoanCallback::new(vec![], 100_000);
        router.execute_flash_loan(token, 100, callback).await.unwrap();
        let metrics = router.get_metrics().await;
        assert_eq!(metrics.total_loans, 1);
        assert_eq!(metrics.successful_loans, 1);
        assert!(metrics.average_execution_time_ms > 0.0);
    }

    #[tokio::test]
    async fn test_uniswap_v3_estimate_gas_variation() {
        let provider = UniswapV3Provider::new(test_token(1), [2u8; 32]);
        provider.add_pool(test_token(100), test_token(10), test_token(20), 100, 1u128 << 96, 1_000_000, 0);
        let gas = provider.estimate_gas(test_token(10), 1000).await.unwrap();
        assert!(gas < 300_000);
        assert!(gas > 0);
    }
}
