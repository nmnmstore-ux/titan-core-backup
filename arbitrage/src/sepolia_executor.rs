//! Sepolia on-chain executor for the deployed FlashLoanArbitrage contract.
//!
//! Calls `executeArbitrage(address asset, uint256 amount, bytes params)` on the
//! contract deployed at `ARBITRAGE_CONTRACT` (Sepolia) using the wallet in
//! `EXECUTOR_KEY` and the RPC in `SEPOLIA_RPC_URL`.
//!
//! `params` is ABI-encoded as `(address[] path, uint24[] fees, uint256 minOutAfterFees, address recipient)`,
//! matching the contract's `executeOperation` decode.

use ethers::abi::{decode, encode, Abi, ParamType, Token};
use ethers::contract::Contract;
use ethers::core::types::{Address as EthAddress, Bytes, H256, U256 as EthU256, U64 as EthU64};
use ethers::middleware::SignerMiddleware;
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::utils::keccak256;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Minimal ABI for the deployed FlashLoanArbitrage contract.
const FLASH_LOAN_ARB_ABI: &str = r#"[
  {"type":"constructor","inputs":[{"name":"_pool","type":"address"},{"name":"_router","type":"address"}],"stateMutability":"nonpayable"},
  {"type":"function","name":"executeArbitrage","inputs":[{"name":"asset","type":"address"},{"name":"amount","type":"uint256"},{"name":"params","type":"bytes"}],"outputs":[],"stateMutability":"nonpayable"},
  {"type":"function","name":"flashFeeBps","inputs":[],"outputs":[{"name":"","type":"uint256"}],"stateMutability":"view"},
  {"type":"function","name":"slipProtectionBps","inputs":[],"outputs":[{"name":"","type":"uint256"}],"stateMutability":"view"},
  {"type":"function","name":"owner","inputs":[],"outputs":[{"name":"","type":"address"}],"stateMutability":"view"},
  {"type":"function","name":"swapRouter","inputs":[],"outputs":[{"name":"","type":"address"}],"stateMutability":"view"},
  {"type":"event","name":"ArbExecuted","inputs":[
    {"name":"asset","type":"address","indexed":false},
    {"name":"amount","type":"uint256","indexed":false},
    {"name":"repay","type":"uint256","indexed":false},
    {"name":"profit","type":"uint256","indexed":false}
  ],"anonymous":false}
]"#;

pub const SEPOLIA_CHAIN_ID: u64 = 11155111;
const MAX_GAS_PRICE_GWEI: u64 = 50;
const FALLBACK_GAS_LIMIT_U64: u64 = 1_200_000;
const RECEIPT_POLL_INTERVAL: Duration = Duration::from_secs(2);
const RECEIPT_POLL_TIMEOUT: Duration = Duration::from_secs(150);

#[derive(Debug, Clone)]
pub struct SepoliaExecutionResult {
    pub success: bool,
    pub tx_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub profit_wei: Option<EthU256>,
    pub error: Option<String>,
    pub executed_at: u64,
    pub duration_ms: u64,
}

pub struct SepoliaExecutor {
    pub contract_address: EthAddress,
    pub executor_address: EthAddress,
    flash_fee_bps: RwLock<Option<EthU256>>,
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    contract: Contract<SignerMiddleware<Provider<Http>, LocalWallet>>,
}

impl std::fmt::Display for SepoliaExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SepoliaExecutor(contract={}, executor={})",
            self.contract_address, self.executor_address
        )
    }
}

impl SepoliaExecutor {
    /// Reads `SEPOLIA_RPC_URL`, `ARBITRAGE_CONTRACT`, `EXECUTOR_KEY` from the
    /// environment and builds the signer/contract. Returns `None` when the
    /// executor is not configured (the engine then runs simulated execution).
    pub fn try_init() -> Option<SepoliaExecutor> {
        let rpc_url = std::env::var("SEPOLIA_RPC_URL").ok();
        let contract = std::env::var("ARBITRAGE_CONTRACT").ok();
        let key = std::env::var("EXECUTOR_KEY").ok();

        let (rpc_url, contract_str, key) = match (rpc_url, contract, key) {
            (Some(r), Some(c), Some(k)) if !r.trim().is_empty() && !c.trim().is_empty() && !k.trim().is_empty() => (r, c, k),
            _ => {
                warn!("Sepolia on-chain executor not configured (SEPOLIA_RPC_URL / ARBITRAGE_CONTRACT / EXECUTOR_KEY missing) — simulated execution only");
                return None;
            }
        };

        let contract_address = match EthAddress::from_str(contract_str.trim()) {
            Ok(a) => a,
            Err(e) => {
                warn!("ARBITRAGE_CONTRACT is not a valid address: {e:?}");
                return None;
            }
        };

        let wallet = match LocalWallet::from_str(key.trim()) {
            Ok(w) => w.with_chain_id(SEPOLIA_CHAIN_ID),
            Err(e) => {
                warn!("EXECUTOR_KEY is not a valid private key: {e:?}");
                return None;
            }
        };
        let executor_address = wallet.address();

        let http = match Http::from_str(&rpc_url) {
            Ok(h) => h,
            Err(e) => {
                warn!("Invalid SEPOLIA_RPC_URL: {e:?}");
                return None;
            }
        };
        let provider = Provider::new(http);
        let client = Arc::new(SignerMiddleware::new(provider, wallet));

        let abi: Abi = match serde_json::from_str(FLASH_LOAN_ARB_ABI) {
            Ok(a) => a,
            Err(e) => {
                warn!("Failed to parse contract ABI: {e}");
                return None;
            }
        };
        let contract = Contract::new(contract_address, abi, client.clone());

        info!(
            "Sepolia on-chain executor initialized: contract={} executor={} rpc={}",
            contract_address, executor_address, rpc_url
        );

        Some(SepoliaExecutor {
            contract_address,
            executor_address,
            flash_fee_bps: RwLock::new(None),
            client,
            contract,
        })
    }

    /// Loads the Aave V3 flash fee (bps) from the contract and caches it.
    pub async fn load_fee_bps(&self) -> Result<EthU256, String> {
        if let Some(fee) = *self.flash_fee_bps.read().unwrap() {
            return Ok(fee);
        }
        let call = self
            .contract
            .method::<_, EthU256>("flashFeeBps", ())
            .map_err(|e| format!("flashFeeBps method: {e}"))?;
        let fee = match call.await {
            Ok(v) => v,
            Err(e) => return Err(format!("flashFeeBps probe failed: {e}")),
        };
        *self.flash_fee_bps.write().unwrap() = Some(fee);
        debug!("Sepolia Aave V3 flash fee cached: {} bps", fee);
        Ok(fee)
    }

    /// Cached flash fee in bps, or `None` if not loaded yet.
    pub fn current_fee_bps(&self) -> Option<u64> {
        self.flash_fee_bps.read().unwrap().map(|v| v.as_u64())
    }

    /// Builds the ABI-encoded `params` payload: `(address[] path, uint24[] fees, uint256 minOutAfterFees, address recipient)`.
    pub fn build_params(
        path: &[EthAddress],
        fees: &[u16],
        min_out_after_fees: EthU256,
        recipient: EthAddress,
    ) -> Vec<u8> {
        let tokens = vec![
            Token::Array(path.iter().map(|a| Token::Address(*a)).collect()),
            Token::Array(fees.iter().map(|f| Token::Uint(EthU256::from(*f as u64))).collect()),
            Token::Uint(min_out_after_fees),
            Token::Address(recipient),
        ];
        encode(&tokens)
    }

    /// Executes a real flash-loan arbitrage on Sepolia against the deployed contract.
    ///
    /// `path` are token addresses (the first must be `asset`), `fees` one fee tier
    /// per hop (`path.len() - 1`). The call is first simulated with `eth_call`;
    /// it is only broadcast if the simulation succeeds (protects against lossy paths).
    pub async fn execute_arbitrage(
        &self,
        asset: EthAddress,
        amount_wei: u128,
        path: Vec<String>,
        fees: Vec<u16>,
        min_out_after_fees: EthU256,
        recipient: EthAddress,
    ) -> SepoliaExecutionResult {
        let start = Instant::now();
        let executed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let result = self
            .execute_arbitrage_inner(asset, amount_wei, &path, &fees, min_out_after_fees, recipient)
            .await;
        let duration_ms = start.elapsed().as_millis() as u64;
        let mut res = match result {
            Ok(r) => r,
            Err(e) => SepoliaExecutionResult {
                success: false,
                tx_hash: None,
                gas_used: None,
                profit_wei: None,
                error: Some(e),
                executed_at,
                duration_ms,
            },
        };
        res.executed_at = executed_at;
        res.duration_ms = duration_ms;
        res
    }

    async fn execute_arbitrage_inner(
        &self,
        asset: EthAddress,
        amount_wei: u128,
        path: &[String],
        fees: &[u16],
        min_out_after_fees: EthU256,
        recipient: EthAddress,
    ) -> Result<SepoliaExecutionResult, String> {
        if path.len() < 2 {
            return Err("path must contain at least 2 tokens".into());
        }
        let mut path_addrs = Vec::with_capacity(path.len());
        for (i, token) in path.iter().enumerate() {
            let addr = EthAddress::from_str(token)
                .map_err(|e| format!("invalid token address in path[{i}]: {e:?}"))?;
            path_addrs.push(addr);
        }
        if path_addrs[0] != asset {
            return Err(format!(
                "path[0] {} does not match asset {}",
                path_addrs[0], asset
            ));
        }
        if fees.len() != path.len() - 1 {
            return Err(format!(
                "fees len {} does not match hops {}",
                fees.len(),
                path.len() - 1
            ));
        }

        let params_bytes = Self::build_params(&path_addrs, fees, min_out_after_fees, recipient);
        let call = self
            .contract
            .method::<_, ()>(
                "executeArbitrage",
                (asset, EthU256::from(amount_wei), Bytes::from(params_bytes)),
            )
            .map_err(|e| format!("build executeArbitrage call: {e}"))?;

        // Phase 1: simulate with eth_call. If it reverts we do NOT broadcast.
        if let Err(e) = call.clone().call().await {
            return Err(format!("simulation reverted (not broadcast): {e}"));
        }

        // Phase 2: gas estimation with a generous fallback.
        let fallback_gas = EthU256::from(FALLBACK_GAS_LIMIT_U64);
        let gas = match call.clone().estimate_gas().await {
            Ok(g) => g,
            Err(e) => {
                warn!("gas estimation failed ({e}); using fallback {fallback_gas}");
                fallback_gas
            }
        };
        let gas = gas.saturating_mul(EthU256::from(12u64)) / EthU256::from(10u64);
        let gas = gas.max(fallback_gas);

        let max_gas_price = EthU256::from(MAX_GAS_PRICE_GWEI * 1_000_000_000u64);
        let gas_price = match self.client.get_gas_price().await {
            Ok(p) => p.min(max_gas_price),
            Err(e) => {
                warn!("get_gas_price failed ({e}); using cap {max_gas_price}");
                max_gas_price
            }
        };

        info!(
            "Sepolia arb submit: asset={} amount={} path={:?} fees={:?} minProfit={} gas={} gasPrice={}",
            asset, amount_wei, path_addrs, fees, min_out_after_fees, gas, gas_price
        );

        let call = call.gas(gas).gas_price(gas_price).legacy();
        let pending = call
            .send()
            .await
            .map_err(|e| format!("broadcast failed: {e}"))?;
        let tx_hash = pending.tx_hash();

        // Phase 3: poll for the receipt.
        let deadline = Instant::now() + RECEIPT_POLL_TIMEOUT;
        let receipt = loop {
            if let Some(r) = self
                .client
                .get_transaction_receipt(tx_hash)
                .await
                .map_err(|e| format!("receipt poll error: {e}"))?
            {
                break r;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "tx {} pending after {}s — check status manually",
                    tx_hash,
                    RECEIPT_POLL_TIMEOUT.as_secs()
                ));
            }
            tokio::time::sleep(RECEIPT_POLL_INTERVAL).await;
        };

        let gas_used = receipt.gas_used.map(|g| g.as_u64());
        let status_ok = receipt.status == Some(EthU64::from(1u64));

        if !status_ok {
            return Ok(SepoliaExecutionResult {
                success: false,
                tx_hash: Some(format!("{:?}", tx_hash)),
                gas_used,
                profit_wei: None,
                error: Some(format!("tx reverted: {}", Self::extract_revert_reason(&receipt))),
                executed_at: 0,
                duration_ms: 0,
            });
        }

        let profit_wei = Self::extract_arb_executed_profit(&receipt);
        info!(
            "Sepolia arb SUCCESS tx={:?} gas={:?} profitWei={:?}",
            tx_hash, gas_used, profit_wei
        );
        Ok(SepoliaExecutionResult {
            success: true,
            tx_hash: Some(format!("{:?}", tx_hash)),
            gas_used,
            profit_wei,
            error: None,
            executed_at: 0,
            duration_ms: 0,
        })
    }

    /// Decodes `ArbExecuted(address,uint256,uint256,uint256)` from the receipt logs.
    fn extract_arb_executed_profit(receipt: &ethers::core::types::TransactionReceipt) -> Option<EthU256> {
        let topic = H256::from_slice(&keccak256(b"ArbExecuted(address,uint256,uint256,uint256)"));
        for log in &receipt.logs {
            if log.topics.first() == Some(&topic) {
                if let Ok(tokens) = decode(
                    &[
                        ParamType::Address,
                        ParamType::Uint(256),
                        ParamType::Uint(256),
                        ParamType::Uint(256),
                    ],
                    &log.data.0,
                ) {
                    if let Some(Token::Uint(profit)) = tokens.get(3) {
                        return Some(*profit);
                    }
                }
            }
        }
        None
    }

    /// Best-effort revert reason extraction from a receipt (ethers 2 receipts carry no revert_reason).
    fn extract_revert_reason(receipt: &ethers::core::types::TransactionReceipt) -> String {
        match &receipt.status {
            Some(s) => format!("status={}", s),
            None => "unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_encoding_roundtrip_matches_contract_decode() {
        let path = vec![
            EthAddress::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            EthAddress::from_str("0x2222222222222222222222222222222222222222").unwrap(),
            EthAddress::from_str("0x3333333333333333333333333333333333333333").unwrap(),
        ];
        let fees = vec![3000u16, 500u16];
        let min_out = EthU256::from(1_000_000u64);
        let recipient = EthAddress::from_str("0x4444444444444444444444444444444444444444").unwrap();

        let bytes = SepoliaExecutor::build_params(&path, &fees, min_out, recipient);

        // Same decode as the contract: (address[], uint24[], uint256, address)
        let decoded = decode(
            &[
                ParamType::Array(Box::new(ParamType::Address)),
                ParamType::Array(Box::new(ParamType::Uint(24))),
                ParamType::Uint(256),
                ParamType::Address,
            ],
            &bytes,
        )
        .expect("decode");
        assert_eq!(decoded.len(), 4);
        match &decoded[0] {
            Token::Array(tokens) => {
                assert_eq!(tokens.len(), 3);
                for (i, t) in tokens.iter().enumerate() {
                    assert_eq!(*t, Token::Address(path[i]));
                }
            }
            _ => panic!("path not decoded"),
        }
        match &decoded[1] {
            Token::Array(tokens) => {
                assert_eq!(tokens.len(), 2);
                for (i, t) in tokens.iter().enumerate() {
                    assert_eq!(*t, Token::Uint(EthU256::from(fees[i] as u64)));
                }
            }
            _ => panic!("fees not decoded"),
        }
        assert_eq!(decoded[2], Token::Uint(min_out));
        assert_eq!(decoded[3], Token::Address(recipient));
    }

    #[test]
    fn try_init_returns_none_without_env() {
        std::env::remove_var("SEPOLIA_RPC_URL");
        std::env::remove_var("ARBITRAGE_CONTRACT");
        std::env::remove_var("EXECUTOR_KEY");
        assert!(SepoliaExecutor::try_init().is_none());
    }
}
