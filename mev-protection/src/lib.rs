use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use parking_lot::RwLock;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock as TokioRwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub mod mev_extraction_engine;
pub const FLASHBOTS_RELAY_MAINNET: &str = "https://relay.flashbots.net";
pub const FLASHBOTS_RELAY_GOERLI: &str = "https://relay-goerli.flashbots.net";
pub const MEV_SHARE_MAINNET: &str = "https://mev-share.flashbots.net";
pub const DEFAULT_PERCENTILE: u8 = 50;
pub const MAX_RETRIES: u32 = 3;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum MevError {
    BundleRejected(String),
    SimulationFailed(String),
    RelayError(String),
    InvalidBundle(String),
    NetworkError(String),
    DeadlineExceeded,
}

impl std::fmt::Display for MevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MevError::BundleRejected(msg) => write!(f, "Bundle rejected: {}", msg),
            MevError::SimulationFailed(msg) => write!(f, "Simulation failed: {}", msg),
            MevError::RelayError(msg) => write!(f, "Relay error: {}", msg),
            MevError::InvalidBundle(msg) => write!(f, "Invalid bundle: {}", msg),
            MevError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            MevError::DeadlineExceeded => write!(f, "Deadline exceeded"),
        }
    }
}

impl std::error::Error for MevError {}

impl From<reqwest::Error> for MevError {
    fn from(e: reqwest::Error) -> Self {
        MevError::NetworkError(e.to_string())
    }
}

impl From<serde_json::Error> for MevError {
    fn from(e: serde_json::Error) -> Self {
        MevError::NetworkError(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MevBundle {
    pub id: Uuid,
    pub txs: Vec<Vec<u8>>,
    pub block_number: u64,
    pub max_block: u64,
    pub priority_fee: U256,
    pub reverting_tx_hashes: Vec<H256>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MevBundleResult {
    pub bundle_hash: H256,
    pub success: bool,
    pub block_number: Option<u64>,
    pub effective_gas_price: U256,
    pub profit: i128,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: u64,
    pub profit: i128,
    pub logs: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStats {
    pub submitted: u64,
    pub simulated: u64,
    pub confirmed: u64,
    pub failed: u64,
    pub total_profit: i128,
    pub avg_gas_price: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    pub total_bundles: u64,
    pub pending_bundles: u64,
    pub confirmed_bundles: u64,
    pub total_profit: i128,
    pub efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchParams {
    pub to: Option<Address>,
    pub function_signature: Option<[u8; 4]>,
    pub value_threshold: Option<U256>,
}

#[derive(Debug, Clone)]
pub enum TipStrategy {
    Conservative,
    Standard,
    Aggressive,
    Custom(U256),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimates {
    pub slow: U256,
    pub standard: U256,
    pub fast: U256,
    pub base_fee: U256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MevStrategy {
    FlashbotsOnly,
    MevShare,
    SimulateThenSubmit,
    BundleWithReverts,
}

// ---------------------------------------------------------------------------
// JSON-RPC helper types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: T,
}

#[derive(Deserialize)]
struct JsonRpcResponse<R> {
    jsonrpc: String,
    id: u64,
    result: Option<R>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ---------------------------------------------------------------------------
// Flashbots relay client
// ---------------------------------------------------------------------------

pub struct FlashbotsRelayClient {
    pub relay_url: String,
    auth_key: H256,
    client: Client,
}

impl FlashbotsRelayClient {
    pub fn new(relay_url: &str, auth_key: H256) -> Self {
        Self {
            relay_url: relay_url.to_string(),
            auth_key,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    async fn send_json_rpc<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self, method: &'static str, params: T,
    ) -> Result<R, MevError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: rand::thread_rng().gen::<u64>(),
            method,
            params,
        };

        let body = serde_json::to_string(&request)?;

        let mut last_err = MevError::NetworkError("all retries exhausted".into());
        let delays = [
            Duration::from_millis(100),
            Duration::from_millis(500),
            Duration::from_secs(2),
        ];

        for attempt in 0..=MAX_RETRIES as usize {
            let resp = self
                .client
                .post(&self.relay_url)
                .header("X-Flashbots-Signature", hex::encode(self.auth_key))
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send()
                .await;

            match resp {
                Ok(r) => {
                    if r.status().is_success() {
                        let text = r.text().await.map_err(MevError::from)?;
                        let json: JsonRpcResponse<R> = serde_json::from_str(&text)?;
                        if let Some(result) = json.result {
                            return Ok(result);
                        } else if let Some(err) = json.error {
                            return Err(MevError::RelayError(err.message));
                        }
                    } else {
                        let status = r.status();
                        let text = r.text().await.unwrap_or_default();
                        last_err = MevError::RelayError(format!("HTTP {}: {}", status, text));
                    }
                }
                Err(e) => {
                    last_err = MevError::NetworkError(e.to_string());
                }
            }

            if attempt < delays.len() {
                tokio::time::sleep(delays[attempt]).await;
            }
        }

        Err(last_err)
    }

    pub async fn submit_bundle(&self, bundle: MevBundle) -> Result<MevBundleResult, MevError> {
        let params = serde_json::json!({
            "txs": bundle.txs.iter().map(|t| hex::encode(t)).collect::<Vec<_>>(),
            "blockNumber": format!("0x{:x}", bundle.block_number),
            "maxBlock": format!("0x{:x}", bundle.max_block),
            "priorityFee": format!("0x{:x}", bundle.priority_fee),
            "revertingTxHashes": bundle.reverting_tx_hashes.iter().map(|h| hex::encode(h)).collect::<Vec<_>>(),
        });

        let result: serde_json::Value = self.send_json_rpc("eth_sendBundle", params).await?;

        Ok(MevBundleResult {
            bundle_hash: bundle_hash(&bundle),
            success: result.get("bundleHash").is_some(),
            block_number: result
                .get("blockNumber")
                .and_then(|v| v.as_str())
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()),
            effective_gas_price: result
                .get("effectiveGasPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| U256::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            profit: 0,
            error: None,
        })
    }

    pub async fn simulate_bundle(
        &self, bundle: &MevBundle, block_number: u64,
    ) -> Result<SimulationResult, MevError> {
        let params = serde_json::json!({
            "txs": bundle.txs.iter().map(|t| hex::encode(t)).collect::<Vec<_>>(),
            "blockNumber": format!("0x{:x}", block_number),
            "stateBlockNumber": format!("0x{:x}", block_number.saturating_sub(1)),
            "timestamp": format!("0x{:x}", bundle.timestamp),
        });

        let result: serde_json::Value = self.send_json_rpc("eth_callBundle", params).await?;

        Ok(SimulationResult {
            success: result
                .get("error")
                .and_then(|v| v.as_str())
                .map(|e| !e.contains("revert"))
                .unwrap_or(true),
            gas_used: result
                .get("gasUsed")
                .and_then(|v| v.as_str())
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            profit: result
                .get("profit")
                .and_then(|v| v.as_str())
                .and_then(|s| i128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            logs: result
                .get("logs")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| serde_json::to_string(l).ok())
                        .collect()
                })
                .unwrap_or_default(),
            error: result
                .get("error")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    pub async fn cancel_bundle(&self, bundle_hash: H256) -> Result<bool, MevError> {
        let params = serde_json::json!({
            "bundleHash": hex::encode(bundle_hash),
        });

        let result: serde_json::Value =
            self.send_json_rpc("flashbots_cancelBundle", params).await?;

        Ok(result
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub async fn get_bundle_stats(&self, bundle_hash: H256) -> Result<BundleStats, MevError> {
        let params = serde_json::json!({
            "bundleHash": hex::encode(bundle_hash),
        });

        let result: serde_json::Value =
            self.send_json_rpc("flashbots_getBundleStats", params).await?;

        Ok(BundleStats {
            submitted: result
                .get("submitted")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            simulated: result
                .get("simulated")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            confirmed: result
                .get("confirmed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            failed: result.get("failed").and_then(|v| v.as_u64()).unwrap_or(0),
            total_profit: result
                .get("totalProfit")
                .and_then(|v| v.as_str())
                .and_then(|s| i128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            avg_gas_price: result
                .get("avgGasPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| U256::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
        })
    }

    pub async fn get_user_stats(&self) -> Result<UserStats, MevError> {
        let result: serde_json::Value =
            self.send_json_rpc("flashbots_getUserStats", serde_json::json!({}))
                .await?;

        Ok(UserStats {
            total_bundles: result
                .get("totalBundles")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            pending_bundles: result
                .get("pendingBundles")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            confirmed_bundles: result
                .get("confirmedBundles")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_profit: result
                .get("totalProfit")
                .and_then(|v| v.as_str())
                .and_then(|s| i128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            efficiency: result
                .get("efficiency")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        })
    }
}

// ---------------------------------------------------------------------------
// MEV-Share client
// ---------------------------------------------------------------------------

pub struct MevShareClient {
    pub relay_url: String,
    auth_key: H256,
    client: Client,
}

impl MevShareClient {
    pub fn new(relay_url: &str, auth_key: H256) -> Self {
        Self {
            relay_url: relay_url.to_string(),
            auth_key,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    async fn send_json_rpc<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self, method: &'static str, params: T,
    ) -> Result<R, MevError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: rand::thread_rng().gen::<u64>(),
            method,
            params,
        };

        let body = serde_json::to_string(&request)?;
        let mut last_err = MevError::NetworkError("all retries exhausted".into());
        let delays = [
            Duration::from_millis(100),
            Duration::from_millis(500),
            Duration::from_secs(2),
        ];

        for attempt in 0..=MAX_RETRIES as usize {
            let resp = self
                .client
                .post(&self.relay_url)
                .header("X-Flashbots-Signature", hex::encode(self.auth_key))
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send()
                .await;

            match resp {
                Ok(r) => {
                    if r.status().is_success() {
                        let text = r.text().await.map_err(MevError::from)?;
                        let json: JsonRpcResponse<R> = serde_json::from_str(&text)?;
                        if let Some(result) = json.result {
                            return Ok(result);
                        } else if let Some(err) = json.error {
                            return Err(MevError::RelayError(err.message));
                        }
                    } else {
                        let status = r.status();
                        let text = r.text().await.unwrap_or_default();
                        last_err = MevError::RelayError(format!("HTTP {}: {}", status, text));
                    }
                }
                Err(e) => {
                    last_err = MevError::NetworkError(e.to_string());
                }
            }

            if attempt < delays.len() {
                tokio::time::sleep(delays[attempt]).await;
            }
        }

        Err(last_err)
    }

    pub async fn submit_share_bundle(
        &self, bundle: MevBundle, match_params: MatchParams,
    ) -> Result<MevBundleResult, MevError> {
        let match_params_json = serde_json::json!({
            "to": match_params.to.map(|a| hex::encode(a)),
            "functionSignature": match_params.function_signature.map(|s| hex::encode(s)),
            "valueThreshold": match_params.value_threshold.map(|v| format!("0x{:x}", v)),
        });

        let params = serde_json::json!({
            "txs": bundle.txs.iter().map(|t| hex::encode(t)).collect::<Vec<_>>(),
            "blockNumber": format!("0x{:x}", bundle.block_number),
            "maxBlock": format!("0x{:x}", bundle.max_block),
            "matching": match_params_json,
        });

        let result: serde_json::Value =
            self.send_json_rpc("mev_sendBundle", params).await?;

        Ok(MevBundleResult {
            bundle_hash: bundle_hash(&bundle),
            success: true,
            block_number: result
                .get("blockNumber")
                .and_then(|v| v.as_str())
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()),
            effective_gas_price: result
                .get("effectiveGasPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| U256::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            profit: 0,
            error: None,
        })
    }

    pub async fn get_share_stats(&self, bundle_hash: H256) -> Result<BundleStats, MevError> {
        let params = serde_json::json!({
            "bundleHash": hex::encode(bundle_hash),
        });

        let result: serde_json::Value =
            self.send_json_rpc("mev_getBundleStats", params).await?;

        Ok(BundleStats {
            submitted: result
                .get("submitted")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            simulated: result
                .get("simulated")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            confirmed: result
                .get("confirmed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            failed: result.get("failed").and_then(|v| v.as_u64()).unwrap_or(0),
            total_profit: result
                .get("totalProfit")
                .and_then(|v| v.as_str())
                .and_then(|s| i128::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            avg_gas_price: result
                .get("avgGasPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| U256::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
        })
    }

    pub async fn estimate_share_value(&self, bundle: &MevBundle) -> Result<U256, MevError> {
        let params = serde_json::json!({
            "txs": bundle.txs.iter().map(|t| hex::encode(t)).collect::<Vec<_>>(),
            "blockNumber": format!("0x{:x}", bundle.block_number),
        });

        let result: serde_json::Value =
            self.send_json_rpc("mev_estimateBundle", params).await?;

        result
            .get("estimatedValue")
            .and_then(|v| v.as_str())
            .and_then(|s| U256::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .ok_or_else(|| MevError::RelayError("Missing estimatedValue".into()))
    }
}

// ---------------------------------------------------------------------------
// Priority fee estimator
// ---------------------------------------------------------------------------

struct FeeDataPoint {
    priority_fee: U256,
    base_fee: U256,
    timestamp: Instant,
}

pub struct PriorityFeeEstimator {
    pub percentile: u8,
    history: Arc<RwLock<VecDeque<FeeDataPoint>>>,
    max_history: usize,
}

impl PriorityFeeEstimator {
    pub fn new(percentile: u8) -> Self {
        Self {
            percentile: percentile.clamp(1, 100),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            max_history: 100,
        }
    }

    pub async fn estimate_priority_fee(&self, _chain_id: u64) -> Result<U256, MevError> {
        let history = self.history.read();
        if history.is_empty() {
            return Ok(self.default_tip_for_percentile());
        }

        let mut fees: Vec<U256> = history.iter().map(|d| d.priority_fee).collect();
        fees.sort_unstable();
        Ok(percentile_value(&fees, self.percentile))
    }

    pub async fn estimate_priority_fees(&self, _chain_id: u64) -> Result<FeeEstimates, MevError> {
        let history = self.history.read();
        let base_fee = history
            .back()
            .map(|d| d.base_fee)
            .unwrap_or(10_000_000_000u128);

        if history.is_empty() {
            return Ok(FeeEstimates {
                slow: 100_000_000u128,
                standard: 500_000_000u128,
                fast: 2_000_000_000u128,
                base_fee,
            });
        }

        let mut fees: Vec<U256> = history.iter().map(|d| d.priority_fee).collect();
        fees.sort_unstable();

        Ok(FeeEstimates {
            slow: percentile_value(&fees, 10),
            standard: percentile_value(&fees, 50),
            fast: percentile_value(&fees, 90),
            base_fee,
        })
    }

    pub async fn estimate_flashbots_bid(
        &self, gas_used: u64, tip: TipStrategy,
    ) -> Result<U256, MevError> {
        let tip_value = match tip {
            TipStrategy::Conservative => 100_000_000u128,   // 0.1 gwei
            TipStrategy::Standard => 500_000_000u128,       // 0.5 gwei
            TipStrategy::Aggressive => 2_000_000_000u128,   // 2 gwei
            TipStrategy::Custom(v) => v,
        };

        let history = self.history.read();
        let base_fee = history
            .back()
            .map(|d| d.base_fee)
            .unwrap_or(10_000_000_000u128);
        let total_tip = tip_value * gas_used as u128;
        Ok(total_tip + base_fee * gas_used as u128)
    }

    pub fn adjust_percentile(&mut self, percentile: u8) {
        self.percentile = percentile.clamp(1, 100);
    }

    pub fn feed_fee_data(&self, priority_fee: U256, base_fee: U256) {
        let mut history = self.history.write();
        if history.len() >= self.max_history {
            history.pop_front();
        }
        history.push_back(FeeDataPoint {
            priority_fee,
            base_fee,
            timestamp: Instant::now(),
        });
    }

    fn default_tip_for_percentile(&self) -> U256 {
        match self.percentile {
            0..=25 => 100_000_000u128,
            26..=50 => 500_000_000u128,
            51..=75 => 1_000_000_000u128,
            _ => 2_000_000_000u128,
        }
    }

    pub fn history_len(&self) -> usize {
        self.history.read().len()
    }
}

fn percentile_value(sorted_fees: &[U256], pct: u8) -> U256 {
    if sorted_fees.is_empty() {
        return 0;
    }
    let idx = ((pct as f64 / 100.0) * (sorted_fees.len() - 1) as f64).round() as usize;
    sorted_fees[idx.clamp(0, sorted_fees.len() - 1)]
}

// ---------------------------------------------------------------------------
// MevBundleBuilder
// ---------------------------------------------------------------------------

pub struct MevBundleBuilder {
    txs: Vec<Vec<u8>>,
    block_number: Option<u64>,
    max_block: Option<u64>,
    priority_fee: U256,
    reverting_tx_hashes: Vec<H256>,
}

impl MevBundleBuilder {
    pub fn new() -> Self {
        Self {
            txs: Vec::new(),
            block_number: None,
            max_block: None,
            priority_fee: 0,
            reverting_tx_hashes: Vec::new(),
        }
    }

    pub fn add_transaction(&mut self, tx: Vec<u8>) -> &mut Self {
        self.txs.push(tx);
        self
    }

    pub fn set_block_range(&mut self, start: u64, end: u64) -> &mut Self {
        self.block_number = Some(start);
        self.max_block = Some(end);
        self
    }

    pub fn set_priority_fee(&mut self, fee: U256) -> &mut Self {
        self.priority_fee = fee;
        self
    }

    pub fn add_reverting_tx(&mut self, hash: H256) -> &mut Self {
        self.reverting_tx_hashes.push(hash);
        self
    }

    pub fn build(&self) -> Result<MevBundle, MevError> {
        if self.txs.is_empty() {
            return Err(MevError::InvalidBundle(
                "Bundle must contain at least one transaction".into(),
            ));
        }
        let block_number = self
            .block_number
            .ok_or_else(|| MevError::InvalidBundle("Block number is required".into()))?;
        let max_block = self
            .max_block
            .ok_or_else(|| MevError::InvalidBundle("Max block is required".into()))?;

        if max_block < block_number {
            return Err(MevError::InvalidBundle(format!(
                "Max block {} is less than block number {}",
                max_block, block_number
            )));
        }

        Ok(MevBundle {
            id: Uuid::new_v4(),
            txs: self.txs.clone(),
            block_number,
            max_block,
            priority_fee: self.priority_fee,
            reverting_tx_hashes: self.reverting_tx_hashes.clone(),
            timestamp: Utc::now().timestamp() as u64,
        })
    }
}

// ---------------------------------------------------------------------------
// MevEngine
// ---------------------------------------------------------------------------

pub struct MevEngine {
    flashbots: FlashbotsRelayClient,
    share: Option<MevShareClient>,
    fee_estimator: PriorityFeeEstimator,
}

impl MevEngine {
    pub fn new(
        flashbots: FlashbotsRelayClient,
        share: Option<MevShareClient>,
        fee_estimator: PriorityFeeEstimator,
    ) -> Self {
        Self {
            flashbots,
            share,
            fee_estimator,
        }
    }

    pub async fn protect_transaction(
        &self, tx: Vec<u8>, block_number: u64, strategy: MevStrategy,
    ) -> Result<MevBundleResult, MevError> {
        let mut builder = MevBundleBuilder::new();
        builder.add_transaction(tx);
        builder.set_block_range(block_number, block_number + 5);

        let fee = self
            .fee_estimator
            .estimate_priority_fee(1)
            .await
            .unwrap_or(500_000_000);
        builder.set_priority_fee(fee);

        let bundle = builder.build()?;

        match strategy {
            MevStrategy::FlashbotsOnly => self.flashbots.submit_bundle(bundle).await,
            MevStrategy::MevShare => {
                if let Some(ref share) = self.share {
                    let match_params = MatchParams {
                        to: None,
                        function_signature: None,
                        value_threshold: None,
                    };
                    share.submit_share_bundle(bundle, match_params).await
                } else {
                    Err(MevError::RelayError(
                        "MEV-Share client not configured".into(),
                    ))
                }
            }
            MevStrategy::SimulateThenSubmit => self.simulate_and_submit(bundle).await,
            MevStrategy::BundleWithReverts => {
                let hash = bundle_hash(&bundle);
                builder.add_reverting_tx(hash);
                let bundle_with_reverts = builder.build()?;
                self.flashbots.submit_bundle(bundle_with_reverts).await
            }
        }
    }

    pub async fn execute_safe_bundle(
        &self, txs: Vec<Vec<u8>>, block_number: u64, max_blocks: u64,
    ) -> Result<MevBundleResult, MevError> {
        let mut builder = MevBundleBuilder::new();
        for tx in &txs {
            builder.add_transaction(tx.clone());
        }
        builder.set_block_range(block_number, block_number + max_blocks);

        let fee = self
            .fee_estimator
            .estimate_priority_fee(1)
            .await
            .unwrap_or(500_000_000);
        builder.set_priority_fee(fee);

        let bundle = builder.build()?;
        let sim = self
            .flashbots
            .simulate_bundle(&bundle, block_number)
            .await?;

        if !sim.success {
            return Err(MevError::SimulationFailed(
                sim.error
                    .unwrap_or_else(|| "Unknown simulation error".into()),
            ));
        }

        self.flashbots.submit_bundle(bundle).await
    }

    pub async fn simulate_and_submit(
        &self, bundle: MevBundle,
    ) -> Result<MevBundleResult, MevError> {
        let sim = self
            .flashbots
            .simulate_bundle(&bundle, bundle.block_number)
            .await?;

        if !sim.success {
            return Err(MevError::SimulationFailed(
                sim.error
                    .unwrap_or_else(|| "Bundle simulation failed".into()),
            ));
        }

        info!(
            "Simulation succeeded: gas_used={}, profit={}",
            sim.gas_used, sim.profit
        );
        self.flashbots.submit_bundle(bundle).await
    }

    pub fn fee_estimator(&self) -> &PriorityFeeEstimator {
        &self.fee_estimator
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

pub fn bundle_hash(bundle: &MevBundle) -> H256 {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for tx in &bundle.txs {
        hasher.update(tx);
    }
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

pub fn format_gwei(wei: U256) -> String {
    let gwei = wei / 1_000_000_000;
    let remainder = wei % 1_000_000_000;
    if remainder == 0 {
        format!("{} gwei", gwei)
    } else {
        let rem_str = format!("{:09}", remainder)
            .trim_end_matches('0')
            .to_string();
        if rem_str.is_empty() {
            format!("{} gwei", gwei)
        } else {
            format!("{}.{} gwei", gwei, rem_str)
        }
    }
}

pub fn parse_gwei(s: &str) -> Result<U256, MevError> {
    let s = s.trim().to_lowercase().replace("gwei", "").trim().to_string();
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(MevError::InvalidBundle(format!(
            "Invalid gwei format: {}",
            s
        )));
    }
    let integer: U256 = parts[0]
        .parse()
        .map_err(|_| MevError::InvalidBundle(format!("Invalid integer: {}", parts[0])))?;
    let fractional: U256 = if parts.len() == 2 {
        let frac = parts[1];
        if frac.len() > 9 {
            return Err(MevError::InvalidBundle(format!(
                "Too many decimal places: {}",
                s
            )));
        }
        let padded = format!("{:0<9}", frac);
        U256::from_str_radix(&padded, 10)
            .map_err(|_| MevError::InvalidBundle(format!("Invalid fractional: {}", frac)))?
    } else {
        0
    };
    Ok(integer * 1_000_000_000 + fractional)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tx() -> Vec<u8> {
        hex::decode("02f86b01038459682f0085174876e80082520894742006d37f85b0a0c2a0b7b2e0d9c8b6f0c0c0a0d880de0b6b3a764000080c080a0")
            .unwrap_or_default()
    }

    fn dummy_hash(n: u8) -> H256 {
        let mut h = [0u8; 32];
        h[31] = n;
        h
    }

    // -----------------------------------------------------------------------
    // Flashbots relay test helpers (mock-oriented)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_flashbots_submit_bundle() {
        let auth = dummy_hash(1);
        let client = FlashbotsRelayClient::new("http://127.0.0.1:9999", auth);

        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 100,
            max_block: 105,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![],
            timestamp: Utc::now().timestamp() as u64,
        };

        let result = client.submit_bundle(bundle).await;
        assert!(result.is_err());
        match result {
            Err(MevError::NetworkError(_)) | Err(MevError::RelayError(_)) => {}
            Err(MevError::DeadlineExceeded) => {}
            _ => panic!("Expected network error due to fake relay URL"),
        }
    }

    #[tokio::test]
    async fn test_flashbots_simulate_bundle() {
        let auth = dummy_hash(2);
        let client = FlashbotsRelayClient::new("http://127.0.0.1:9998", auth);

        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 200,
            max_block: 205,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![],
            timestamp: Utc::now().timestamp() as u64,
        };

        let result = client.simulate_bundle(&bundle, 200).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flashbots_cancel_bundle() {
        let auth = dummy_hash(3);
        let client = FlashbotsRelayClient::new("http://127.0.0.1:9997", auth);

        let result = client.cancel_bundle(dummy_hash(42)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flashbots_get_stats() {
        let auth = dummy_hash(4);
        let client = FlashbotsRelayClient::new("http://127.0.0.1:9996", auth);

        let result = client.get_bundle_stats(dummy_hash(7)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flashbots_get_user_stats() {
        let auth = dummy_hash(5);
        let client = FlashbotsRelayClient::new("http://127.0.0.1:9995", auth);

        let result = client.get_user_stats().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_share_submit() {
        let auth = dummy_hash(6);
        let client = MevShareClient::new("http://127.0.0.1:9994", auth);

        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 300,
            max_block: 305,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![],
            timestamp: Utc::now().timestamp() as u64,
        };

        let match_params = MatchParams {
            to: None,
            function_signature: None,
            value_threshold: None,
        };

        let result = client.submit_share_bundle(bundle, match_params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_share_get_stats() {
        let auth = dummy_hash(7);
        let client = MevShareClient::new("http://127.0.0.1:9993", auth);
        let result = client.get_share_stats(dummy_hash(1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_share_estimate_value() {
        let auth = dummy_hash(8);
        let client = MevShareClient::new("http://127.0.0.1:9992", auth);
        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 400,
            max_block: 405,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![],
            timestamp: Utc::now().timestamp() as u64,
        };
        let result = client.estimate_share_value(&bundle).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Fee estimator tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fee_estimator_basic() {
        let estimator = PriorityFeeEstimator::new(50);

        for i in 1..=20 {
            estimator.feed_fee_data(i * 100_000_000, 10_000_000_000);
        }

        let fee = estimator.estimate_priority_fee(1).await.unwrap();
        assert!(fee > 0);
    }

    #[tokio::test]
    async fn test_fee_estimator_percentiles() {
        let estimator = PriorityFeeEstimator::new(50);

        for i in 1..=100 {
            estimator.feed_fee_data(i * 10_000_000, 10_000_000_000);
        }

        let estimates = estimator.estimate_priority_fees(1).await.unwrap();
        assert!(estimates.slow <= estimates.standard);
        assert!(estimates.standard <= estimates.fast);
        assert!(estimates.base_fee > 0);
    }

    #[tokio::test]
    async fn test_fee_estimator_flashbots_bid() {
        let estimator = PriorityFeeEstimator::new(50);

        estimator.feed_fee_data(500_000_000, 10_000_000_000);

        let conservative = estimator
            .estimate_flashbots_bid(21000, TipStrategy::Conservative)
            .await
            .unwrap();
        let aggressive = estimator
            .estimate_flashbots_bid(21000, TipStrategy::Aggressive)
            .await
            .unwrap();
        assert!(conservative <= aggressive);
    }

    #[tokio::test]
    async fn test_fee_estimator_adjust_percentile() {
        let mut estimator = PriorityFeeEstimator::new(10);

        for i in 1..=50 {
            estimator.feed_fee_data(i * 10_000_000, 10_000_000_000);
        }

        let fee_10 = estimator.estimate_priority_fee(1).await.unwrap();
        estimator.adjust_percentile(90);
        let fee_90 = estimator.estimate_priority_fee(1).await.unwrap();
        assert!(fee_10 <= fee_90);
    }

    #[tokio::test]
    async fn test_fee_estimator_empty_history() {
        let estimator = PriorityFeeEstimator::new(50);
        let fee = estimator.estimate_priority_fee(1).await.unwrap();
        assert!(fee > 0);
        let estimates = estimator.estimate_priority_fees(1).await.unwrap();
        assert_eq!(estimates.slow, 100_000_000);
        assert_eq!(estimates.standard, 500_000_000);
        assert_eq!(estimates.fast, 2_000_000_000);
    }

    // -----------------------------------------------------------------------
    // Bundle builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bundle_builder_basic() {
        let mut builder = MevBundleBuilder::new();
        let bundle = builder
            .add_transaction(dummy_tx())
            .set_block_range(100, 110)
            .set_priority_fee(500_000_000)
            .build()
            .expect("Bundle should build successfully");

        assert_eq!(bundle.block_number, 100);
        assert_eq!(bundle.max_block, 110);
        assert_eq!(bundle.priority_fee, 500_000_000);
        assert_eq!(bundle.txs.len(), 1);
    }

    #[test]
    fn test_bundle_builder_validation() {
        let mut builder = MevBundleBuilder::new();
        let result = builder.add_transaction(dummy_tx()).build();
        assert!(matches!(result, Err(MevError::InvalidBundle(_))));
    }

    #[test]
    fn test_bundle_builder_invalid_range() {
        let mut builder = MevBundleBuilder::new();
        let result = builder
            .add_transaction(dummy_tx())
            .set_block_range(200, 100)
            .build();
        assert!(result.is_err());
        if let Err(MevError::InvalidBundle(msg)) = result {
            assert!(msg.contains("less than"));
        }
    }

    #[test]
    fn test_bundle_builder_empty_txs() {
        let mut builder = MevBundleBuilder::new();
        let result = builder.set_block_range(1, 10).build();
        assert!(matches!(result, Err(MevError::InvalidBundle(_))));
    }

    #[test]
    fn test_bundle_builder_multiple_txs() {
        let mut builder = MevBundleBuilder::new();
        let bundle = builder
            .add_transaction(dummy_tx())
            .add_transaction(dummy_tx())
            .add_transaction(dummy_tx())
            .set_block_range(50, 60)
            .build()
            .expect("Bundle with 3 txs should build");
        assert_eq!(bundle.txs.len(), 3);
    }

    // -----------------------------------------------------------------------
    // MevEngine tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mev_engine_protect() {
        let auth = dummy_hash(10);
        let flashbots = FlashbotsRelayClient::new("http://127.0.0.1:9991", auth);
        let share = Some(MevShareClient::new(
            "http://127.0.0.1:9990",
            dummy_hash(11),
        ));
        let fee_estimator = PriorityFeeEstimator::new(50);

        let engine = MevEngine::new(flashbots, share, fee_estimator);

        let result = engine
            .protect_transaction(dummy_tx(), 500, MevStrategy::FlashbotsOnly)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_engine_simulate_fail() {
        let auth = dummy_hash(12);
        let flashbots = FlashbotsRelayClient::new("http://127.0.0.1:9989", auth);
        let fee_estimator = PriorityFeeEstimator::new(50);

        let engine = MevEngine::new(flashbots, None, fee_estimator);

        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 600,
            max_block: 605,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![],
            timestamp: Utc::now().timestamp() as u64,
        };

        let result = engine.simulate_and_submit(bundle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_engine_execute_safe_bundle() {
        let auth = dummy_hash(13);
        let flashbots = FlashbotsRelayClient::new("http://127.0.0.1:9988", auth);
        let fee_estimator = PriorityFeeEstimator::new(50);
        let engine = MevEngine::new(flashbots, None, fee_estimator);

        let result = engine
            .execute_safe_bundle(vec![dummy_tx()], 700, 10)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mev_engine_mevshare_not_configured() {
        let auth = dummy_hash(14);
        let flashbots = FlashbotsRelayClient::new("http://127.0.0.1:9987", auth);
        let fee_estimator = PriorityFeeEstimator::new(50);
        let engine = MevEngine::new(flashbots, None, fee_estimator);

        let result = engine
            .protect_transaction(dummy_tx(), 800, MevStrategy::MevShare)
            .await;
        assert!(matches!(result, Err(MevError::RelayError(_))));
    }

    // -----------------------------------------------------------------------
    // Helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_helpers_format_gwei() {
        assert_eq!(format_gwei(0), "0 gwei");
        assert_eq!(format_gwei(1_000_000_000), "1 gwei");
        assert_eq!(format_gwei(1_500_000_000), "1.5 gwei");
        assert_eq!(format_gwei(100_000_000), "0.1 gwei");
    }

    #[test]
    fn test_helpers_parse_gwei() {
        assert_eq!(parse_gwei("1 gwei").unwrap(), 1_000_000_000);
        assert_eq!(parse_gwei("1.5 gwei").unwrap(), 1_500_000_000);
        assert_eq!(parse_gwei("0.1 gwei").unwrap(), 100_000_000);
        assert!(parse_gwei("").is_err());
        assert!(parse_gwei("abc gwei").is_err());
    }

    #[test]
    fn test_helpers_bundle_hash() {
        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx(), dummy_tx()],
            block_number: 1,
            max_block: 10,
            priority_fee: 0,
            reverting_tx_hashes: vec![],
            timestamp: 0,
        };

        let hash = bundle_hash(&bundle);
        assert_ne!(hash, [0u8; 32]);
        let hash2 = bundle_hash(&bundle);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_helpers_parse_gwei_roundtrip() {
        let vals = ["0 gwei", "1 gwei", "1.5 gwei", "0.001 gwei", "100 gwei"];
        for v in &vals {
            let parsed = parse_gwei(v).unwrap();
            let formatted = format_gwei(parsed);
            let reparsed = parse_gwei(&formatted).unwrap();
            assert_eq!(parsed, reparsed, "Roundtrip failed for: {}", v);
        }
    }

    #[test]
    fn test_mev_error_display() {
        let err = MevError::BundleRejected("insufficient funds".into());
        let msg = format!("{}", err);
        assert!(msg.contains("insufficient funds"));

        let err = MevError::DeadlineExceeded;
        assert_eq!(format!("{}", err), "Deadline exceeded");
    }

    #[test]
    fn test_tip_strategy_custom() {
        let custom = TipStrategy::Custom(1_000_000_000);
        match custom {
            TipStrategy::Custom(v) => assert_eq!(v, 1_000_000_000),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_mev_strategies() {
        assert_eq!(
            format!("{:?}", MevStrategy::FlashbotsOnly),
            "FlashbotsOnly"
        );
        assert_eq!(format!("{:?}", MevStrategy::MevShare), "MevShare");
        assert_eq!(
            format!("{:?}", MevStrategy::SimulateThenSubmit),
            "SimulateThenSubmit"
        );
        assert_eq!(
            format!("{:?}", MevStrategy::BundleWithReverts),
            "BundleWithReverts"
        );
    }

    #[test]
    fn test_fee_estimator_default() {
        let estimator = PriorityFeeEstimator::new(99);
        assert_eq!(estimator.history_len(), 0);
    }

    #[test]
    fn test_percentile_value_edge_cases() {
        assert_eq!(percentile_value(&[], 50), 0);
        assert_eq!(percentile_value(&[100], 50), 100);
        assert_eq!(percentile_value(&[1, 2, 3], 0), 1);
        assert_eq!(percentile_value(&[1, 2, 3], 100), 3);
    }

    #[test]
    fn test_mev_bundle_serialization() {
        let bundle = MevBundle {
            id: Uuid::new_v4(),
            txs: vec![dummy_tx()],
            block_number: 42,
            max_block: 50,
            priority_fee: 500_000_000,
            reverting_tx_hashes: vec![dummy_hash(1)],
            timestamp: 12345,
        };
        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: MevBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.id, deserialized.id);
        assert_eq!(bundle.block_number, deserialized.block_number);
    }

    #[test]
    fn test_bundle_result_defaults() {
        let result = MevBundleResult {
            bundle_hash: [0u8; 32],
            success: false,
            block_number: None,
            effective_gas_price: 0,
            profit: 0,
            error: Some("test".into()),
        };
        assert!(!result.success);
        assert!(result.block_number.is_none());
        assert_eq!(result.error.unwrap(), "test");
    }
}
