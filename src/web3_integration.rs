use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::rpc_router::RpcPool;

#[derive(Clone)]
pub struct UnilateralRecoveryClient {
    contract_address: String,
    rpc_pool: Arc<RpcPool>,
    tee_enclave: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub user: String,
    pub amount: String,
    pub signature: Vec<u8>,
    pub nonce: u64,
    pub chain_id: u64,
    pub deadline: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStatus {
    pub user: String,
    pub amount: String,
    pub approved: bool,
    pub executed_at: u64,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeployment {
    pub name: String,
    pub address: String,
    pub network: String,
    pub block_number: u64,
    pub tx_hash: String,
    pub deployed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub contracts: HashMap<String, ContractDeployment>,
    pub default_network: String,
}

impl DeployConfig {
    pub fn new() -> Self {
        Self {
            contracts: HashMap::new(),
            default_network: "sepolia".to_string(),
        }
    }

    pub fn add_contract(
        &mut self,
        name: &str,
        address: &str,
        network: &str,
        block_number: u64,
        tx_hash: &str,
    ) {
        self.contracts.insert(
            name.to_string(),
            ContractDeployment {
                name: name.to_string(),
                address: address.to_string(),
                network: network.to_string(),
                block_number,
                tx_hash: tx_hash.to_string(),
                deployed_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            },
        );
    }

    pub fn get_address(&self, name: &str) -> Option<String> {
        self.contracts.get(name).map(|c| c.address.clone())
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "default_network": self.default_network,
            "contracts": self.contracts.iter().map(|(name, c)| serde_json::json!({
                "name": c.name,
                "address": c.address,
                "network": c.network,
                "block": c.block_number,
                "tx": c.tx_hash,
                "deployed_at": c.deployed_at,
            })).collect::<Vec<_>>(),
        })
    }
}

impl UnilateralRecoveryClient {
    pub fn new(contract_address: &str, rpc_pool: Arc<RpcPool>, tee_enclave: &str) -> Self {
        Self {
            contract_address: contract_address.to_string(),
            rpc_pool,
            tee_enclave: tee_enclave.to_string(),
        }
    }

    pub async fn submit_recovery(&self, request: &RecoveryRequest) -> Result<String, String> {
        let calldata = format!(
            "0x{}",
            hex::encode(format!(
                "{}{}{}{}",
                "b3b4f3c3",
                request.user.trim_start_matches("0x"),
                format!(
                    "{:064x}",
                    request.amount.parse::<u128>().unwrap_or(0)
                ),
                format!("{:064x}", request.nonce),
            ))
        );

        let params = serde_json::json!([
            { "to": self.contract_address, "data": calldata },
            "latest"
        ]);

        match self.rpc_pool.rpc_call("eth_call", params).await {
            Ok((result, _endpoint)) => {
                if let Some(error) = result.get("error") {
                    return Err(format!("RPC error: {}", error));
                }
                Ok(result["result"].as_str().unwrap_or("0x").to_string())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn check_recovery_status(&self, user: &str) -> Result<RecoveryStatus, String> {
        let calldata = format!(
            "0x{}",
            hex::encode(format!(
                "{}000000000000000000000000{}",
                "c3b3f4a1",
                user.trim_start_matches("0x"),
            ))
        );

        let params = serde_json::json!([
            { "to": self.contract_address, "data": calldata },
            "latest"
        ]);

        match self.rpc_pool.rpc_call("eth_call", params).await {
            Ok((result, _endpoint)) => Ok(RecoveryStatus {
                user: user.to_string(),
                amount: result["result"].as_str().unwrap_or("0x0").to_string(),
                approved: false,
                executed_at: 0,
                tx_hash: None,
            }),
            Err(e) => Err(e),
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "contract": self.contract_address,
            "endpoints": self.rpc_pool.len(),
            "tee_enclave": self.tee_enclave,
            "status": "connected",
        })
    }
}
