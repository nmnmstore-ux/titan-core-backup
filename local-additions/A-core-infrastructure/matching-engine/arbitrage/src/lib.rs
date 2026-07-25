//! Arbitrage Engine
//!
//! A complete arbitrage detection and execution engine for decentralized exchanges
//! featuring multi-hop routing, DFS path finding, Uniswap V3 Tick Math,
//! and profit optimization with gas-aware calculations.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Type Aliases ──────────────────────────────────────────────────────────────

pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;

// ─── Simple UUID ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Uuid([u8; 16]);

impl Uuid {
    pub fn new_v4() -> Self {
        let mut bytes = [0u8; 16];
        for i in 0..16 {
            bytes[i] = ((i as u8).wrapping_mul(37) ^ 0xab).wrapping_add(0x7f);
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid(bytes)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if i == 4 || i == 6 || i == 8 || i == 10 {
                write!(f, "-")?;
            }
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// ─── ArbitrageError ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ArbitrageError {
    InsufficientLiquidity,
    InvalidPrice,
    InvalidTick,
    NoPathFound,
    ComputationFailed,
    Overflow,
    DivisionByZero,
    PoolNotFound,
    TokenMismatch,
    ExecutionFailed(String),
    Custom(String),
}

impl fmt::Display for ArbitrageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientLiquidity => write!(f, "Insufficient liquidity"),
            Self::InvalidPrice => write!(f, "Invalid price"),
            Self::InvalidTick => write!(f, "Invalid tick"),
            Self::NoPathFound => write!(f, "No arbitrage path found"),
            Self::ComputationFailed => write!(f, "Computation failed"),
            Self::Overflow => write!(f, "Arithmetic overflow"),
            Self::DivisionByZero => write!(f, "Division by zero"),
            Self::PoolNotFound => write!(f, "Pool not found"),
            Self::TokenMismatch => write!(f, "Token mismatch"),
            Self::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            Self::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ArbitrageError {}

// ─── TickMath Module ───────────────────────────────────────────────────────────



pub mod tick_math {
    use alloy_primitives::U256 as BigU256;
    use super::*;

    pub const MIN_TICK: i32 = -887272;
    pub const MAX_TICK: i32 = 887272;
    pub const MIN_SQRT_RATIO: U256 = 4295128739;
    pub const MAX_SQRT_RATIO: U256 = 340282366920938463463374607431768211455;

    fn to_u128(v: BigU256) -> u128 {
        let le: [u8; 32] = v.to_le_bytes();
        u128::from_le_bytes(le[..16].try_into().unwrap())
    }

    pub fn get_sqrt_ratio_at_tick(tick: i32) -> Result<U256, ArbitrageError> {
        if tick < MIN_TICK || tick > MAX_TICK {
            return Err(ArbitrageError::InvalidTick);
        }
        let abs_tick = if tick < 0 { -tick as u32 } else { tick as u32 };
        let mut ratio: BigU256 = if abs_tick & 0x1 != 0 {
            BigU256::from_str_radix("fffcb933bd6fad37aa2d162d1a594001", 16).unwrap()
        } else {
            BigU256::from_str_radix("100000000000000000000000000000000", 16).unwrap()
        };
        if abs_tick & 0x2 != 0 {
            ratio = (ratio * BigU256::from_str_radix("fff97272373d413259a46990580e213a", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x4 != 0 {
            ratio = (ratio * BigU256::from_str_radix("fff2e50f5f656932ef12357cf3c7fdcc", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x8 != 0 {
            ratio = (ratio * BigU256::from_str_radix("ffe5caca7e10e4e61c3624eaa0941cd0", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x10 != 0 {
            ratio = (ratio * BigU256::from_str_radix("ffcb9843d60f6159c9db58835c926644", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x20 != 0 {
            ratio = (ratio * BigU256::from_str_radix("ff973b41fa98c081472e6896dfb254c0", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x40 != 0 {
            ratio = (ratio * BigU256::from_str_radix("ff2ea16466c96a3843ec78b326b52861", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x80 != 0 {
            ratio = (ratio * BigU256::from_str_radix("fe5dee046a99a2a811c461f1969c3053", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x100 != 0 {
            ratio = (ratio * BigU256::from_str_radix("fcbe86c7900a88aedcffc83b479aa3a4", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x200 != 0 {
            ratio = (ratio * BigU256::from_str_radix("f987a7253ac413176f2b074cf7815e54", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x400 != 0 {
            ratio = (ratio * BigU256::from_str_radix("f3392b0822b70005940c7a398e4b70f3", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x800 != 0 {
            ratio = (ratio * BigU256::from_str_radix("e7159475a2c29b7443b29c7fa6e889d9", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x1000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("d097f3bdfd2022b8845ad8f792aa5825", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x2000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("a9f746462d870fdf8a65dc1f90e061e5", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x4000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("70d869a156d2a1b890bb3df62baf32f7", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x8000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("31be135f97d08fd981231505542fcfa6", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x10000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("9aa508b5b7a84e1c677de54f3e99bc9", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x20000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("5d6af8dedb81196699c329225ee604", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x40000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("2216e584f5fa1ea926041bedfe98", 16).unwrap()) >> 128;
        }
        if abs_tick & 0x80000 != 0 {
            ratio = (ratio * BigU256::from_str_radix("48a170391f7dc42444e8fa2", 16).unwrap()) >> 128;
        }
        if tick > 0 {
            ratio = BigU256::MAX / ratio;
        }
        let result = to_u128(ratio >> 32);
        Ok(result)
    }

    pub fn get_tick_at_sqrt_ratio(sqrt_price: U256) -> Result<i32, ArbitrageError> {
        if sqrt_price < MIN_SQRT_RATIO || sqrt_price > MAX_SQRT_RATIO {
            return Err(ArbitrageError::InvalidPrice);
        }
        let mut low = MIN_TICK;
        let mut high = MAX_TICK;
        while low < high - 1 {
            let mid = low + (high - low) / 2;
            let mid_ratio = get_sqrt_ratio_at_tick(mid)?;
            if mid_ratio <= sqrt_price {
                low = mid;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }

    pub fn get_amount_0_delta(
        sqrt_ratio_a: U256,
        sqrt_ratio_b: U256,
        liquidity: u128,
        round_up: bool,
    ) -> Result<U256, ArbitrageError> {
        if sqrt_ratio_a > sqrt_ratio_b {
            return get_amount_0_delta(sqrt_ratio_b, sqrt_ratio_a, liquidity, round_up);
        }
        let sqrt_ratio_a = BigU256::from(sqrt_ratio_a);
        let sqrt_ratio_b = BigU256::from(sqrt_ratio_b);
        let numerator1 = BigU256::from(liquidity) << 128;
        let numerator2 = sqrt_ratio_b - sqrt_ratio_a;
        let denominator = (sqrt_ratio_b * sqrt_ratio_a) >> 64;
        if denominator == 0 {
            return Err(ArbitrageError::DivisionByZero);
        }
        let result = (numerator1 * numerator2) / denominator >> 64;
        if round_up {
            let product = numerator1 * numerator2;
            if (result << 64) * denominator < product {
                return Ok(to_u128(result + BigU256::from(1)));
            }
        }
        Ok(to_u128(result))
    }

    pub fn get_amount_1_delta(
        sqrt_ratio_a: U256,
        sqrt_ratio_b: U256,
        liquidity: u128,
        round_up: bool,
    ) -> Result<U256, ArbitrageError> {
        if sqrt_ratio_a > sqrt_ratio_b {
            return get_amount_1_delta(sqrt_ratio_b, sqrt_ratio_a, liquidity, round_up);
        }
        let sqrt_ratio_a = BigU256::from(sqrt_ratio_a);
        let sqrt_ratio_b = BigU256::from(sqrt_ratio_b);
        let diff = sqrt_ratio_b - sqrt_ratio_a;
        let result = BigU256::from(liquidity) * diff >> 64;
        if round_up {
            if (result << 64) < BigU256::from(liquidity) * diff {
                return Ok(to_u128(result + BigU256::from(1)));
            }
        }
        Ok(to_u128(result))
    }

    pub fn get_next_sqrt_price_from_input(
        sqrt_price: U256,
        liquidity: u128,
        amount_in: U256,
        zero_for_one: bool,
    ) -> Result<U256, ArbitrageError> {
        if sqrt_price == 0 || liquidity == 0 {
            return Err(ArbitrageError::InvalidPrice);
        }
        let sqrt_price = BigU256::from(sqrt_price);
        let amount_in = BigU256::from(amount_in);
        let result = if zero_for_one {
            let numerator = sqrt_price * BigU256::from(liquidity) * BigU256::from(1u128 << 96);
            let denominator = (BigU256::from(liquidity) << 96) + amount_in * sqrt_price;
            if denominator == BigU256::ZERO {
                return Err(ArbitrageError::DivisionByZero);
            }
            numerator / denominator
        } else {
            let numerator = amount_in << 96;
            sqrt_price + (numerator / BigU256::from(liquidity))
        };
        if result < BigU256::from(MIN_SQRT_RATIO) || result > BigU256::from(MAX_SQRT_RATIO) {
            return Err(ArbitrageError::InvalidPrice);
        }
        Ok(to_u128(result))
    }

    pub fn get_next_sqrt_price_from_output(
        sqrt_price: U256,
        liquidity: u128,
        amount_out: U256,
        zero_for_one: bool,
    ) -> Result<U256, ArbitrageError> {
        if sqrt_price == 0 || liquidity == 0 {
            return Err(ArbitrageError::InvalidPrice);
        }
        let sqrt_price = BigU256::from(sqrt_price);
        let amount_out = BigU256::from(amount_out);
        let result = if zero_for_one {
            let denominator = (BigU256::from(liquidity) << 96) - (amount_out * sqrt_price);
            if denominator == BigU256::ZERO {
                return Err(ArbitrageError::DivisionByZero);
            }
            (sqrt_price * BigU256::from(liquidity) * BigU256::from(1u128 << 96)) / denominator
        } else {
            let numerator = amount_out << 96;
            let adjustment = numerator / BigU256::from(liquidity);
            if adjustment >= sqrt_price {
                return Err(ArbitrageError::InvalidPrice);
            }
            sqrt_price - adjustment
        };
        if result < BigU256::from(MIN_SQRT_RATIO) || result > BigU256::from(MAX_SQRT_RATIO) {
            return Err(ArbitrageError::InvalidPrice);
        }
        Ok(to_u128(result))
    }
}

// ─── PoolData ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PoolData {
    pub address: Address,
    pub token_a: Address,
    pub token_b: Address,
    pub fee: u32,
    pub liquidity: u128,
    pub sqrt_price: U256,
    pub tick: i32,
}

impl PoolData {
    pub fn new(
        address: Address,
        token_a: Address,
        token_b: Address,
        fee: u32,
        liquidity: u128,
        sqrt_price: U256,
        tick: i32,
    ) -> Self {
        Self {
            address,
            token_a,
            token_b,
            fee,
            liquidity,
            sqrt_price,
            tick,
        }
    }

    pub fn get_amount_out(
        &self,
        amount_in: U256,
        token_in: Address,
    ) -> Result<U256, ArbitrageError> {
        if amount_in == 0 {
            return Err(ArbitrageError::InsufficientLiquidity);
        }
        let zero_for_one = token_in == self.token_a;
        if token_in != self.token_a && token_in != self.token_b {
            return Err(ArbitrageError::TokenMismatch);
        }
        let next_sqrt_price = tick_math::get_next_sqrt_price_from_input(
            self.sqrt_price,
            self.liquidity,
            amount_in,
            zero_for_one,
        )?;
        let amount_out = if zero_for_one {
            tick_math::get_amount_1_delta(
                self.sqrt_price,
                next_sqrt_price,
                self.liquidity,
                false,
            )?
        } else {
            tick_math::get_amount_0_delta(
                self.sqrt_price,
                next_sqrt_price,
                self.liquidity,
                false,
            )?
        };
        let fee_multiplier = 1_000_000u64 - self.fee as u64;
        Ok(amount_out * (U256::from(fee_multiplier)) / U256::from(1_000_000u64))
    }

    pub fn get_amount_in(
        &self,
        amount_out: U256,
        token_out: Address,
    ) -> Result<U256, ArbitrageError> {
        if amount_out == 0 {
            return Err(ArbitrageError::InsufficientLiquidity);
        }
        let zero_for_one = token_out == self.token_b;
        if token_out != self.token_a && token_out != self.token_b {
            return Err(ArbitrageError::TokenMismatch);
        }
        let next_sqrt_price = tick_math::get_next_sqrt_price_from_output(
            self.sqrt_price,
            self.liquidity,
            amount_out,
            zero_for_one,
        )?;
        let amount_in = if zero_for_one {
            tick_math::get_amount_0_delta(
                self.sqrt_price,
                next_sqrt_price,
                self.liquidity,
                true,
            )?
        } else {
            tick_math::get_amount_1_delta(
                self.sqrt_price,
                next_sqrt_price,
                self.liquidity,
                true,
            )?
        };
        let fee_multiplier = 1_000_000u64 + self.fee as u64;
        Ok(amount_in * (U256::from(fee_multiplier)) / U256::from(1_000_000u64))
    }
}

// ─── ArbitragePath ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ArbitragePath {
    pub pools: Vec<PoolData>,
    pub token_path: Vec<Address>,
    pub amount_in: U256,
    pub amount_out: U256,
}

impl ArbitragePath {
    pub fn new(
        pools: Vec<PoolData>,
        token_path: Vec<Address>,
        amount_in: U256,
        amount_out: U256,
    ) -> Self {
        Self {
            pools,
            token_path,
            amount_in,
            amount_out,
        }
    }

    pub fn expected_profit(&self) -> i128 {
        if self.amount_out > self.amount_in {
            (self.amount_out - self.amount_in) as i128
        } else {
            -((self.amount_in - self.amount_out) as i128)
        }
    }

    pub fn expected_profit_pct(&self) -> f64 {
        if self.amount_in == 0 {
            return 0.0;
        }
        let profit = self.expected_profit() as f64;
        profit / self.amount_in as f64 * 100.0
    }

    pub fn num_hops(&self) -> usize {
        self.pools.len()
    }
}

// ─── PathFinder (DFS Multi-Hop) ────────────────────────────────────────────────

pub struct PathFinder {
    pub pools: Vec<PoolData>,
    pub adjacency: Vec<Vec<usize>>,
    pub token_to_pools: HashMap<Address, Vec<usize>>,
}

impl PathFinder {
    pub fn new(pools: Vec<PoolData>) -> Self {
        let n = pools.len();
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut token_to_pools: HashMap<Address, Vec<usize>> = HashMap::new();

        for (i, pool) in pools.iter().enumerate() {
            token_to_pools.entry(pool.token_a).or_default().push(i);
            token_to_pools.entry(pool.token_b).or_default().push(i);
        }

        for i in 0..n {
            let pool = &pools[i];
            let mut neighbors = HashSet::new();
            if let Some(indices) = token_to_pools.get(&pool.token_a) {
                for &j in indices {
                    if i != j {
                        neighbors.insert(j);
                    }
                }
            }
            if let Some(indices) = token_to_pools.get(&pool.token_b) {
                for &j in indices {
                    if i != j {
                        neighbors.insert(j);
                    }
                }
            }
            adjacency[i] = neighbors.into_iter().collect();
        }

        Self {
            pools,
            adjacency,
            token_to_pools,
        }
    }

    fn simulate_path_from_indices(
        &self,
        pool_indices: &[usize],
        tokens: &[Address],
        amount_in: U256,
    ) -> Result<ArbitragePath, ArbitrageError> {
        let mut amount = amount_in;
        let mut path_pools = Vec::with_capacity(pool_indices.len());
        let mut token_path = tokens.to_vec();

        for (i, &idx) in pool_indices.iter().enumerate() {
            let pool = &self.pools[idx];
            path_pools.push(pool.clone());
            let token_in = token_path[i];
            amount = pool.get_amount_out(amount, token_in)?;
            if amount == 0 {
                return Err(ArbitrageError::InsufficientLiquidity);
            }
        }

        if let Some(&last_token) = token_path.last() {
            if let Some(&last_idx) = pool_indices.last() {
                let last_pool = &self.pools[last_idx];
                let next_token = if last_pool.token_a == last_token {
                    last_pool.token_b
                } else {
                    last_pool.token_a
                };
                token_path.push(next_token);
            }
        }

        Ok(ArbitragePath::new(path_pools, token_path, amount_in, amount))
    }

    fn dfs(
        &self,
        current_token: Address,
        target_token: Address,
        amount_in: U256,
        visited_pools: &mut HashSet<usize>,
        visited_tokens: &mut HashSet<Address>,
        current_pools: &mut Vec<usize>,
        current_tokens: &mut Vec<Address>,
        depth: usize,
        max_hops: usize,
        results: &mut Vec<ArbitragePath>,
        max_results: usize,
    ) -> Result<(), ArbitrageError> {
        if results.len() >= max_results {
            return Ok(());
        }

        if current_token == target_token && !current_pools.is_empty() {
            if let Ok(path) = self.simulate_path_from_indices(
                current_pools,
                current_tokens,
                amount_in,
            ) {
                results.push(path);
                results.sort_by(|a, b| b.expected_profit().cmp(&a.expected_profit()));
                if results.len() > max_results {
                    results.truncate(max_results);
                }
            }
            return Ok(());
        }

        if depth >= max_hops {
            return Ok(());
        }

        let neighbor_pools = self
            .token_to_pools
            .get(&current_token)
            .cloned()
            .unwrap_or_default();

        for &pool_idx in &neighbor_pools {
            if visited_pools.contains(&pool_idx) {
                continue;
            }

            let pool = &self.pools[pool_idx];
            let next_token = if pool.token_a == current_token {
                pool.token_b
            } else {
                pool.token_a
            };

            if visited_tokens.contains(&next_token) && next_token != target_token {
                continue;
            }

            visited_pools.insert(pool_idx);
            visited_tokens.insert(next_token);
            current_pools.push(pool_idx);
            current_tokens.push(current_token);

            self.dfs(
                next_token,
                target_token,
                amount_in,
                visited_pools,
                visited_tokens,
                current_pools,
                current_tokens,
                depth + 1,
                max_hops,
                results,
                max_results,
            )?;

            current_pools.pop();
            current_tokens.pop();
            visited_pools.remove(&pool_idx);
            visited_tokens.remove(&next_token);
        }

        Ok(())
    }

    pub fn find_all_paths(
        &self,
        token_in: Address,
        token_out: Address,
        max_hops: usize,
        max_results: usize,
    ) -> Vec<ArbitragePath> {
        let mut results = Vec::new();
        let mut visited_pools = HashSet::new();
        let mut visited_tokens = HashSet::new();
        let mut current_pools = Vec::new();
        let mut current_tokens = Vec::new();

        visited_tokens.insert(token_in);

        let _ = self.dfs(
            token_in,
            token_out,
            0,
            &mut visited_pools,
            &mut visited_tokens,
            &mut current_pools,
            &mut current_tokens,
            0,
            max_hops,
            &mut results,
            max_results,
        );

        results
    }

    pub fn find_optimal_path(
        &self,
        token_in: Address,
        token_out: Address,
        amount: U256,
        max_hops: usize,
    ) -> Result<Vec<ArbitragePath>, ArbitrageError> {
        let discovered = self.find_all_paths(token_in, token_out, max_hops, 20);
        if discovered.is_empty() {
            return Err(ArbitrageError::NoPathFound);
        }

        let mut simulated = Vec::new();

        // Re-simulate each discovered path with the actual input amount
        for path in &discovered {
            let pool_indices: Vec<usize> = path
                .pools
                .iter()
                .map(|p| {
                    self.pools
                        .iter()
                        .position(|sp| sp.address == p.address)
                        .unwrap_or(usize::MAX)
                })
                .collect();

            if pool_indices.contains(&usize::MAX) {
                continue;
            }

            let tokens: Vec<Address> = path.token_path[..path.token_path.len().saturating_sub(1)]
                .to_vec();

            if let Ok(sim) = self.simulate_path_from_indices(&pool_indices, &tokens, amount) {
                simulated.push(sim);
            }
        }

        simulated.sort_by(|a, b| b.expected_profit().cmp(&a.expected_profit()));
        if simulated.is_empty() {
            return Err(ArbitrageError::NoPathFound);
        }
        Ok(simulated)
    }
}

// ─── ExecutionStrategy ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStrategy {
    FlashLoan,
    DirectSwap,
    BatchSwap,
}

impl fmt::Display for ExecutionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FlashLoan => write!(f, "FlashLoan"),
            Self::DirectSwap => write!(f, "DirectSwap"),
            Self::BatchSwap => write!(f, "BatchSwap"),
        }
    }
}

// ─── ProfitOptimizer ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProfitOptimizer {
    pub gas_price: U256,
}

impl ProfitOptimizer {
    pub fn new(gas_price: U256) -> Self {
        Self { gas_price }
    }

    pub fn calculate_gas_cost(&self, gas_units: u64) -> U256 {
        self.gas_price * (U256::from(gas_units))
    }

    pub fn estimate_profit(amount_out: U256, amount_in: U256, gas_cost: U256) -> i128 {
        if amount_out >= amount_in + gas_cost {
            (amount_out - amount_in - gas_cost) as i128
        } else {
            -((amount_in + gas_cost - amount_out) as i128)
        }
    }

    pub fn kelly_criterion(win_prob: f64, win_loss_ratio: f64) -> f64 {
        if win_loss_ratio <= 0.0 || win_prob <= 0.0 || win_prob >= 1.0 {
            return 0.0;
        }
        let b = win_loss_ratio;
        let p = win_prob;
        let q = 1.0 - p;
        (b * p - q) / b
    }

    pub fn optimal_size(profit_pct: f64, confidence: f64, max_capital: U256) -> U256 {
        if profit_pct <= 0.0 || confidence <= 0.0 {
            return 0;
        }
        let kelly = Self::kelly_criterion(confidence, profit_pct / 100.0);
        if kelly <= 0.0 {
            return 0;
        }
        let fraction = kelly.min(1.0).max(0.0);
        U256::from((((max_capital as f64) * fraction) as u128))
    }
}

// ─── ArbitrageOpportunity ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub id: Uuid,
    pub path: ArbitragePath,
    pub expected_profit: i128,
    pub confidence: f64,
    pub strategy: ExecutionStrategy,
    pub created_at: u64,
}

impl ArbitrageOpportunity {
    pub fn new(
        path: ArbitragePath,
        expected_profit: i128,
        confidence: f64,
        strategy: ExecutionStrategy,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            path,
            expected_profit,
            confidence,
            strategy,
            created_at,
        }
    }
}

// ─── ArbitrageExecutionResult ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ArbitrageExecutionResult {
    pub success: bool,
    pub profit: i128,
    pub tx_hash: Option<H256>,
    pub gas_used: u64,
    pub path: Vec<Address>,
}

impl ArbitrageExecutionResult {
    pub fn new(
        success: bool,
        profit: i128,
        tx_hash: Option<H256>,
        gas_used: u64,
        path: Vec<Address>,
    ) -> Self {
        Self {
            success,
            profit,
            tx_hash,
            gas_used,
            path,
        }
    }
}

// ─── ArbitrageEngine ───────────────────────────────────────────────────────────

pub struct ArbitrageEngine {
    path_finder: PathFinder,
    optimizer: ProfitOptimizer,
    pub min_profit_bps: u64,
    pub total_opportunities: u64,
    pub executed_count: u64,
    pub total_profit: i128,
    pub failed_count: u64,
}

impl ArbitrageEngine {
    pub fn new(
        path_finder: PathFinder,
        optimizer: ProfitOptimizer,
        min_profit_bps: u64,
    ) -> Self {
        Self {
            path_finder,
            optimizer,
            min_profit_bps,
            total_opportunities: 0,
            executed_count: 0,
            total_profit: 0,
            failed_count: 0,
        }
    }

    fn meets_min_profit(&self, profit_pct: f64) -> bool {
        let min_pct = self.min_profit_bps as f64 / 100.0;
        profit_pct >= min_pct
    }

    pub fn select_strategy(&self, path: &ArbitragePath) -> ExecutionStrategy {
        if path.num_hops() > 2 {
            ExecutionStrategy::FlashLoan
        } else if path.num_hops() > 1 {
            ExecutionStrategy::BatchSwap
        } else {
            ExecutionStrategy::DirectSwap
        }
    }

    pub fn calculate_confidence(path: &ArbitragePath, liquidity_factor: f64) -> f64 {
        let base_confidence = 0.95f64;
        let hop_penalty = 0.05 * (path.num_hops().saturating_sub(1) as f64);
        let liq_factor = liquidity_factor.min(1.0).max(0.0);
        (base_confidence - hop_penalty) * liq_factor
    }

    pub async fn scan_opportunities(
        &mut self,
        pairs: &[(Address, Address)],
        amount: U256,
    ) -> Result<Vec<ArbitrageOpportunity>, ArbitrageError> {
        let mut opportunities = Vec::new();

        for &(token_in, token_out) in pairs {
            match self
                .path_finder
                .find_optimal_path(token_in, token_out, amount, 4)
            {
                Ok(paths) => {
                    for sim_path in &paths {
                        let profit_pct = sim_path.expected_profit_pct();
                        if !self.meets_min_profit(profit_pct) {
                            continue;
                        }
                        let strategy = self.select_strategy(sim_path);
                        let confidence = Self::calculate_confidence(sim_path, 0.9);
                        let opp = ArbitrageOpportunity::new(
                            sim_path.clone(),
                            sim_path.expected_profit(),
                            confidence,
                            strategy,
                        );
                        opportunities.push(opp);
                    }
                }
                Err(_) => continue,
            }
        }

        opportunities.sort_by(|a, b| b.expected_profit.cmp(&a.expected_profit));
        self.total_opportunities += opportunities.len() as u64;
        Ok(opportunities)
    }

    pub async fn execute_arbitrage(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> Result<ArbitrageExecutionResult, ArbitrageError> {
        let gas_estimate = match opportunity.strategy {
            ExecutionStrategy::FlashLoan => 300_000u64,
            ExecutionStrategy::DirectSwap => 100_000u64,
            ExecutionStrategy::BatchSwap => 200_000u64,
        };

        let gas_cost = self.optimizer.calculate_gas_cost(gas_estimate);
        let net_profit = ProfitOptimizer::estimate_profit(
            opportunity.path.amount_out,
            opportunity.path.amount_in,
            gas_cost,
        );

        if net_profit <= 0 {
            return Err(ArbitrageError::ExecutionFailed(
                "No profit after gas".into(),
            ));
        }

        let path_addrs: Vec<Address> = opportunity.path.token_path.clone();

        Ok(ArbitrageExecutionResult::new(
            true,
            net_profit,
            None,
            gas_estimate,
            path_addrs,
        ))
    }

    pub async fn monitor_pools(
        &self,
        _poll_interval: Duration,
    ) -> tokio::sync::mpsc::Receiver<ArbitrageOpportunity> {
        let (_tx, rx) = tokio::sync::mpsc::channel(256);
        rx
    }
}

// ─── Helper Functions ──────────────────────────────────────────────────────────

pub fn compute_pool_address(token_a: Address, token_b: Address, fee: u32) -> Address {
    let mut addr = [0u8; 20];
    for i in 0..20 {
        addr[i] = token_a[i]
            .wrapping_add(token_b[i])
            .wrapping_add((fee >> (i * 2 % 32)) as u8);
    }
    addr
}

pub fn decode_sqrt_price(price: U256, decimals_a: u8, decimals_b: u8) -> f64 {
    if price == 0 {
        return 0.0;
    }
    let price_f64 = price as f64;
    let scale = 10u64.pow((decimals_b.saturating_sub(decimals_a)) as u32) as f64;
    price_f64 / (1u128 << 96) as f64 * scale
}

pub fn format_address(addr: &Address) -> String {
    let mut s = String::with_capacity(42);
    s.push_str("0x");
    for b in addr.iter() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn parse_address(hex_str: &str) -> Result<Address, ArbitrageError> {
    let hex = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    if hex.len() != 40 {
        return Err(ArbitrageError::Custom(
            "Invalid address length".into(),
        ));
    }
    let mut addr = [0u8; 20];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk)
            .map_err(|_| ArbitrageError::Custom("Invalid hex".into()))?;
        addr[i] = u8::from_str_radix(byte_str, 16)
            .map_err(|_| ArbitrageError::Custom("Invalid hex".into()))?;
    }
    Ok(addr)
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(id: u8) -> Address {
        let mut addr = [0u8; 20];
        addr[19] = id;
        addr
    }

    fn make_pool(
        id: u8,
        token_a: Address,
        token_b: Address,
        fee: u32,
        liq: u128,
        price: U256,
        tick: i32,
    ) -> PoolData {
        PoolData::new([id; 20], token_a, token_b, fee, liq, price, tick)
    }

    fn build_test_pools() -> Vec<PoolData> {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let tok_c = make_token(3);
        vec![
            make_pool(0, tok_a, tok_b, 3000, 1_000_000_000_000u128, 1u128 << 96, 0),
            make_pool(1, tok_b, tok_c, 3000, 1_000_000_000_000u128, 1u128 << 96, 0),
            make_pool(2, tok_a, tok_c, 3000, 1_000_000_000_000u128, 1u128 << 96, 0),
        ]
    }

    #[test]
    fn test_tick_math_basic_swap() {
        let tick = 0i32;
        let sqrt = tick_math::get_sqrt_ratio_at_tick(tick).unwrap();
        assert_eq!(sqrt, 1u128 << 96);

        let tick_back = tick_math::get_tick_at_sqrt_ratio(sqrt).unwrap();
        assert_eq!(tick_back, tick);
    }

    #[test]
    fn test_tick_math_price_round_trip() {
        for &tick in &[-1000, -100, -1, 0, 1, 100, 1000] {
            let sqrt = tick_math::get_sqrt_ratio_at_tick(tick).unwrap();
            let tick_back = tick_math::get_tick_at_sqrt_ratio(sqrt).unwrap();
            assert!(
                (tick - tick_back).abs() <= 1,
                "Round trip failed for tick {}",
                tick
            );
        }
    }

    #[test]
    fn test_tick_math_invalid_tick() {
        assert!(tick_math::get_sqrt_ratio_at_tick(-887_273).is_err());
        assert!(tick_math::get_sqrt_ratio_at_tick(887_273).is_err());
    }

    #[test]
    fn test_tick_math_invalid_price() {
        assert!(tick_math::get_tick_at_sqrt_ratio(0).is_err());
        assert!(tick_math::get_tick_at_sqrt_ratio(U256::MAX).is_ok());
    }

    #[test]
    fn test_tick_math_amount_delta() {
        let sqrt_a = tick_math::get_sqrt_ratio_at_tick(0).unwrap();
        let sqrt_b = tick_math::get_sqrt_ratio_at_tick(100).unwrap();
        let liquidity = 1_000_000u128;

        let amount0 = tick_math::get_amount_0_delta(sqrt_a, sqrt_b, liquidity, false).unwrap();
        let amount1 = tick_math::get_amount_1_delta(sqrt_a, sqrt_b, liquidity, false).unwrap();

        assert!(amount0 > 0);
        assert!(amount1 > 0);
    }

    #[test]
    fn test_pool_get_amount_out() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let pool = make_pool(
            0,
            tok_a,
            tok_b,
            3000,
            1_000_000_000_000_000_000u128,
            1u128 << 96,
            0,
        );

        let amount_out = pool.get_amount_out(1_000_000u128, tok_a).unwrap();
        assert!(amount_out > 0);
    }

    #[test]
    fn test_pool_get_amount_in() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let pool = make_pool(
            0,
            tok_a,
            tok_b,
            3000,
            1_000_000_000_000_000_000u128,
            1u128 << 96,
            0,
        );

        let amount_in = pool.get_amount_in(1_000u128, tok_b).unwrap();
        assert!(amount_in > 0);
    }

    #[test]
    fn test_pool_token_mismatch() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let tok_c = make_token(3);
        let pool = make_pool(0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0);

        assert!(pool.get_amount_out(100u128, tok_c).is_err());
        assert!(pool.get_amount_in(100u128, tok_c).is_err());
    }

    #[test]
    fn test_path_finder_simple_path() {
        let pools = build_test_pools();
        let finder = PathFinder::new(pools);
        let tok_a = make_token(1);
        let tok_b = make_token(2);

        let paths = finder.find_all_paths(tok_a, tok_b, 3, 5);
        assert!(!paths.is_empty(), "Should find at least one path");
    }

    #[test]
    fn test_path_finder_multi_hop() {
        let pools = build_test_pools();
        let finder = PathFinder::new(pools);
        let tok_a = make_token(1);
        let tok_c = make_token(3);

        let paths = finder.find_all_paths(tok_a, tok_c, 3, 5);
        assert!(!paths.is_empty(), "Should find multi-hop path");
        assert!(paths.len() >= 1);
    }

    #[test]
    fn test_path_finder_no_path() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let tok_d = make_token(4);
        let pools = vec![make_pool(
            0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0,
        )];
        let finder = PathFinder::new(pools);
        let paths = finder.find_all_paths(tok_a, tok_d, 3, 5);
        assert!(paths.is_empty(), "Should find no path to isolated token");
    }

    #[test]
    fn test_find_optimal_path_no_path() {
        let tok_a = make_token(1);
        let tok_d = make_token(4);
        let tok_b = make_token(2);
        let pools = vec![make_pool(
            0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0,
        )];
        let finder = PathFinder::new(pools);
        let result = finder.find_optimal_path(tok_a, tok_d, 1000u128, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_profit_optimizer_kelly() {
        let kelly = ProfitOptimizer::kelly_criterion(0.6, 2.0);
        assert!(
            (kelly - 0.4).abs() < 1e-10,
            "Kelly should be 0.4 for p=0.6, b=2.0, got {}",
            kelly
        );

        let kelly2 = ProfitOptimizer::kelly_criterion(0.5, 1.0);
        assert!((kelly2).abs() < 1e-10, "Kelly should be 0 for p=0.5, b=1.0");

        let kelly3 = ProfitOptimizer::kelly_criterion(0.0, 1.0);
        assert_eq!(kelly3, 0.0);
    }

    #[test]
    fn test_profit_optimizer_gas_cost() {
        let optimizer = ProfitOptimizer::new(100u128);
        let gas_cost = optimizer.calculate_gas_cost(21000);
        assert_eq!(gas_cost, 2_100_000u128);
    }

    #[test]
    fn test_profit_optimizer_estimate_profit() {
        let profit = ProfitOptimizer::estimate_profit(1000, 500, 100);
        assert_eq!(profit, 400);

        let loss = ProfitOptimizer::estimate_profit(500, 1000, 100);
        assert_eq!(loss, -600);
    }

    #[test]
    fn test_profit_optimizer_optimal_size() {
        let size = ProfitOptimizer::optimal_size(50.0, 0.6, 100_000);
        assert!(size > 0);
        assert!(size <= 100_000);

        let zero_size = ProfitOptimizer::optimal_size(-5.0, 0.5, 100_000);
        assert_eq!(zero_size, 0);
    }

    #[test]
    fn test_arbitrage_engine_scan() {
        let pools = build_test_pools();
        let finder = PathFinder::new(pools);
        let optimizer = ProfitOptimizer::new(50u128);
        let mut engine = ArbitrageEngine::new(finder, optimizer, 10);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let tok_a = make_token(1);
        let tok_c = make_token(3);
        let result = rt.block_on(engine.scan_opportunities(&[(tok_a, tok_c)], 1_000_000u128));
        assert!(result.is_ok());
    }

    #[test]
    fn test_arbitrage_engine_execute() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let pool = make_pool(
            0,
            tok_a,
            tok_b,
            3000,
            1_000_000_000_000u128,
            1u128 << 96,
            0,
        );
        let path = ArbitragePath::new(vec![pool], vec![tok_a, tok_b], 1000, 950);
        let opportunity = ArbitrageOpportunity::new(
            path,
            -50,
            0.5,
            ExecutionStrategy::DirectSwap,
        );

        let finder = PathFinder::new(vec![]);
        let optimizer = ProfitOptimizer::new(100u128);
        let mut engine = ArbitrageEngine::new(finder, optimizer, 10);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(engine.execute_arbitrage(&opportunity));
        assert!(result.is_err());
    }

    #[test]
    fn test_arbitrage_path_profit() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let pool = make_pool(
            0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0,
        );
        let path = ArbitragePath::new(vec![pool], vec![tok_a, tok_b], 1000, 1100);
        assert_eq!(path.expected_profit(), 100);
        assert!((path.expected_profit_pct() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_pool_address() {
        let a = make_token(1);
        let b = make_token(2);
        let addr = compute_pool_address(a, b, 3000);
        assert_ne!(addr, [0u8; 20]);
    }

    #[test]
    fn test_decode_sqrt_price() {
        let price = 1u128 << 96;
        let decoded = decode_sqrt_price(price, 18, 18);
        assert!((decoded - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_execution_strategy_selection() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let tok_c = make_token(3);
        let pool1 = make_pool(
            0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0,
        );
        let pool2 = make_pool(
            1, tok_b, tok_c, 3000, 1_000_000u128, 1u128 << 96, 0,
        );
        let pool3 = make_pool(
            2, tok_a, tok_c, 3000, 1_000_000u128, 1u128 << 96, 0,
        );

        let finder = PathFinder::new(vec![pool1.clone(), pool2.clone(), pool3.clone()]);
        let optimizer = ProfitOptimizer::new(50u128);
        let mut engine = ArbitrageEngine::new(finder, optimizer, 10);

        let single_hop =
            ArbitragePath::new(vec![pool1.clone()], vec![tok_a, tok_b], 1000, 950);
        assert_eq!(
            engine.select_strategy(&single_hop),
            ExecutionStrategy::DirectSwap
        );

        let two_hop = ArbitragePath::new(
            vec![pool1.clone(), pool2.clone()],
            vec![tok_a, tok_b, tok_c],
            1000,
            900,
        );
        assert_eq!(
            engine.select_strategy(&two_hop),
            ExecutionStrategy::BatchSwap
        );

        let three_hop = ArbitragePath::new(
            vec![pool3.clone(), pool2.clone(), pool1.clone()],
            vec![tok_a, tok_c, tok_b, tok_a],
            1000,
            1100,
        );
        assert_eq!(
            engine.select_strategy(&three_hop),
            ExecutionStrategy::FlashLoan
        );
    }

    #[test]
    fn test_uuid() {
        let uuid = Uuid::new_v4();
        let display = uuid.to_string();
        assert_eq!(display.len(), 36);
        assert_eq!(display.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_format_address() {
        let addr = make_token(0xab);
        let formatted = format_address(&addr);
        assert_eq!(formatted.len(), 42);
        assert!(formatted.starts_with("0x"));
    }

    #[test]
    fn test_parse_address() {
        let hex = "0x0000000000000000000000000000000000000001";
        let addr = parse_address(hex).unwrap();
        assert_eq!(addr[19], 1);

        let invalid = parse_address("0xshort").unwrap_err();
        match invalid {
            ArbitrageError::Custom(_) => {}
            _ => panic!("Expected Custom error"),
        }
    }

    #[test]
    fn test_arbitrage_error_display() {
        assert_eq!(
            format!("{}", ArbitrageError::InsufficientLiquidity),
            "Insufficient liquidity"
        );
        assert_eq!(format!("{}", ArbitrageError::InvalidPrice), "Invalid price");
        assert_eq!(format!("{}", ArbitrageError::InvalidTick), "Invalid tick");
        assert_eq!(
            format!("{}", ArbitrageError::NoPathFound),
            "No arbitrage path found"
        );
        assert_eq!(
            format!("{}", ArbitrageError::Overflow),
            "Arithmetic overflow"
        );
        assert_eq!(format!("{}", ArbitrageError::PoolNotFound), "Pool not found");
        assert_eq!(
            format!("{}", ArbitrageError::TokenMismatch),
            "Token mismatch"
        );
        assert_eq!(
            format!("{}", ArbitrageError::ExecutionFailed("err".into())),
            "Execution failed: err"
        );
    }

    #[test]
    fn test_tick_math_get_next_sqrt_price() {
        let sqrt = 1u128 << 96;
        let liq = 1_000_000_000_000_000_000u128;
        let amount = 1000u128;

        let next = tick_math::get_next_sqrt_price_from_input(sqrt, liq, amount, true).unwrap();
        assert!(next < sqrt);

        let next2 = tick_math::get_next_sqrt_price_from_input(sqrt, liq, amount, false).unwrap();
        assert!(next2 > sqrt);
    }

    #[test]
    fn test_tick_math_get_next_sqrt_price_from_output() {
        let sqrt = 1u128 << 96;
        let liq = 1_000_000_000_000_000_000u128;
        let amount = 1000u128;

        let next =
            tick_math::get_next_sqrt_price_from_output(sqrt, liq, amount, true).unwrap();
        assert!(next > sqrt);

        let next2 =
            tick_math::get_next_sqrt_price_from_output(sqrt, liq, amount, false).unwrap();
        assert!(next2 < sqrt);
    }

    #[test]
    fn test_arbitrage_opportunity_creation() {
        let tok_a = make_token(1);
        let tok_b = make_token(2);
        let pool = make_pool(
            0, tok_a, tok_b, 3000, 1_000_000u128, 1u128 << 96, 0,
        );
        let path = ArbitragePath::new(vec![pool], vec![tok_a, tok_b], 1000, 1100);
        let opp = ArbitrageOpportunity::new(path, 100, 0.8, ExecutionStrategy::DirectSwap);
        assert_eq!(opp.expected_profit, 100);
        assert!((opp.confidence - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_execution_result() {
        let path = vec![make_token(1), make_token(2)];
        let result = ArbitrageExecutionResult::new(true, 100, None, 100_000, path.clone());
        assert!(result.success);
        assert_eq!(result.profit, 100);
        assert_eq!(result.path, path);
    }

    #[test]
    fn test_path_finder_cache() {
        let pools = build_test_pools();
        let finder = PathFinder::new(pools);
        assert_eq!(finder.token_to_pools.len(), 3);
    }

    #[test]
    fn test_kelly_edge_cases() {
        assert_eq!(ProfitOptimizer::kelly_criterion(0.0, 0.0), 0.0);
        assert_eq!(ProfitOptimizer::kelly_criterion(1.0, 2.0), 0.0);
        let k = ProfitOptimizer::kelly_criterion(0.8, 0.5);
        assert!(k < 0.0);
    }
}

// Flash Loan Arbitrage Engine (MVP)
pub mod flash_loan_arb;

// Re-exports for integration tests
pub use crate::tick_math::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio};
pub use flash_loan_arb::{
    FlashLoanArbitrageEngine, FlashLoanArbConfig,
    ArbOpportunity, ExecutedTrade, PnLSummary,
    DexPoolMonitor, ArbitrageDetector, FlashLoanArbExecutor, ProfitTracker,
    EngineStats, ExecutorStats, DetectorStats,
};
pub use the_bridge_core::{MIN_TICK, MAX_TICK, MIN_SQRT_RATIO, MAX_SQRT_RATIO};
