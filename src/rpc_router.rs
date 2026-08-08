pub use the_bridge_arbitrage::rpc_fallback::{pool_from_env, RpcPool};

use std::sync::Arc;

pub fn build_default_pool() -> Arc<RpcPool> {
    let primary = std::env::var("ETH_RPC_URL")
        .unwrap_or_else(|_| "https://ethereum-rpc.publicnode.com".to_string());
    Arc::new(pool_from_env(&primary))
}

pub fn build_pool_from_url(rpc_url: &str) -> Arc<RpcPool> {
    Arc::new(pool_from_env(rpc_url))
}
