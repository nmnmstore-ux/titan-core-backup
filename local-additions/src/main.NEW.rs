use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, error, warn, debug};
use the_bridge_arbitrage::flash_loan_arb::{
    FlashLoanArbitrageEngine, FlashLoanArbConfig,
};
use the_bridge_mev_protection::mev_extraction_engine::{
    MevExtractionEngine, MevExtractionConfig,
};
use the_bridge_cross_venue_arb::{
    CrossVenueArbitrageEngine, CrossVenueConfig,
};
use the_bridge_super_arb::{
    SuperArbEngine, SuperConfig,
};

#[cfg(unix)]
mod ipc {
    pub async fn create_uds_stream() -> std::io::Result<tokio::net::UnixStream> {
        unimplemented!("UNIX domain sockets not yet implemented");
    }
}

#[cfg(windows)]
mod ipc {
    pub async fn create_uds_stream() -> std::io::Result<tokio::net::UnixStream> {
        unimplemented!("Windows named pipes not yet implemented");
    }
}

mod types;
mod orderbook;
mod matching;
mod tee;
mod metrics;
mod numa;
mod snapshot;
mod cloak;
mod wal;
mod crdt;
mod wasm_engine;
mod encrypted;
mod memory;
mod anti_debug;
mod cloud;
mod kyc;
mod pipeline;
mod auth;
mod dashboard;
mod sovereign;
mod counterparty;
mod iso20022;
mod sovereign_protocol;
mod universal_bridge;
mod llm_sidecar;
mod backup;
mod sovereign_fortress;
mod circuit_breaker;
mod io;
mod market_data;
mod token_auth;
mod shariah;
mod ai_agent;
mod liquidation;

mod dual_track;
mod ghost_integration;
mod threshold_crypto;
mod encrypted_mempool;
mod batch_auction;
mod smart_router;
mod dark_pool_orchestrator;
mod dark_pool_manager;
mod web3_integration;

#[cfg(target_os = "linux")]
use tikv_jemallocator::Jemalloc;

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

lazy_static::lazy_static! {
    static ref FLASH_LOAN_ARB: FlashLoanArbitrageEngine = {
        let config = FlashLoanArbConfig {
            eth_rpc_url: std::env::var("ETH_RPC_URL")
                .unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/v2/demo".into()),
            bsc_rpc_url: std::env::var("BSC_RPC_URL")
                .unwrap_or_else(|_| "https://bsc-dataseed.binance.org".into()),
            polygon_rpc_url: std::env::var("POLYGON_RPC_URL")
                .unwrap_or_else(|_| "https://polygon-rpc.com".into()),
            scan_interval_ms: std::env::var("SCAN_INTERVAL_MS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(2000),
            min_profit_usd: std::env::var("MIN_PROFIT_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(50.0),
            min_profit_bps: std::env::var("MIN_PROFIT_BPS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(15),
            max_concurrent_trades: std::env::var("MAX_CONCURRENT_TRADES")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(3),
            ..Default::default()
        };
        FlashLoanArbitrageEngine::new(config)
    };
    static ref MEV_ENGINE: MevExtractionEngine = {
        let config = MevExtractionConfig {
            eth_rpc_url: std::env::var("ETH_RPC_URL")
                .unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/v2/demo".into()),
            flashbots_auth_key: std::env::var("FLASHBOTS_AUTH_KEY")
                .unwrap_or_default(),
            scan_interval_ms: std::env::var("MEV_SCAN_INTERVAL_MS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(1000),
            min_profit_usd: std::env::var("MEV_MIN_PROFIT_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(100.0),
            max_concurrent_bundles: std::env::var("MEV_MAX_CONCURRENT_BUNDLES")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(3),
            ..Default::default()
        };
        MevExtractionEngine::new(config)
    };
    static ref CROSS_VENUE_ENGINE: CrossVenueArbitrageEngine = {
        let config = CrossVenueConfig {
            scan_interval_ms: std::env::var("CV_SCAN_INTERVAL_MS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(2000),
            min_profit_usd: std::env::var("CV_MIN_PROFIT_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(10.0),
            min_profit_bps: std::env::var("CV_MIN_PROFIT_BPS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(5.0),
            max_trade_size_usd: std::env::var("CV_MAX_TRADE_SIZE_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(10000.0),
            ..Default::default()
        };
        CrossVenueArbitrageEngine::new(config)
    };
    static ref SUPER_ARB_ENGINE: SuperArbEngine = {
        let config = SuperConfig {
            scan_interval_ms: std::env::var("SUPER_SCAN_INTERVAL_MS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(1500),
            min_profit_usd: std::env::var("SUPER_MIN_PROFIT_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(10.0),
            max_trade_size_usd: std::env::var("SUPER_MAX_TRADE_SIZE_USD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(10000.0),
            max_concurrent: std::env::var("SUPER_MAX_CONCURRENT")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(5),
            ..Default::default()
        };
        SuperArbEngine::new(config)
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting APEX Media Engine with Sovereign Dark Pool + Flash Loan Arbitrage");

    let arb_engine = &*FLASH_LOAN_ARB;
    tokio::spawn(async move {
        info!("Flash Loan Arbitrage Engine background task starting...");
        match arb_engine.run().await {
            Ok(()) => info!("Flash Loan Arbitrage Engine stopped gracefully"),
            Err(e) => error!("Flash Loan Arbitrage Engine error: {}", e),
        }
    });

    let mev_engine = &*MEV_ENGINE;
    tokio::spawn(async move {
        info!("MEV Extraction Engine background task starting...");
        match mev_engine.run().await {
            Ok(()) => info!("MEV Extraction Engine stopped gracefully"),
            Err(e) => error!("MEV Extraction Engine error: {}", e),
        }
    });

    let cv_engine = &*CROSS_VENUE_ENGINE;
    tokio::spawn(async move {
        info!("Cross-Venue Arbitrage Engine background task starting...");
        match cv_engine.run().await {
            Ok(()) => info!("Cross-Venue Arbitrage Engine stopped gracefully"),
            Err(e) => error!("Cross-Venue Arbitrage Engine error: {}", e),
        }
    });

    let super_engine = &*SUPER_ARB_ENGINE;
    tokio::spawn(async move {
        info!("Super-Arb Engine background task starting...");
        match super_engine.run().await {
            Ok(()) => info!("Super-Arb Engine stopped gracefully"),
            Err(e) => error!("Super-Arb Engine error: {}", e),
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    info!("Flash Loan Arbitrage Engine initialized");
    info!("MEV Extraction Engine initialized");
    info!("Cross-Venue Arbitrage Engine initialized");
    info!("Super-Arb Engine initialized");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("Server listening on port 8080");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                debug!("Error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0u8; 1024];

    match stream.read(&mut buffer).await {
        Ok(0) => return Ok(()),
        Ok(n) => {
            let request = String::from_utf8_lossy(&buffer[..n]);
            debug!("Incoming request: {}", request);

            let response = process_request(&request).await;
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                debug!("Failed to write response: {}", e);
            }
        }
        Err(e) => debug!("Failed to read from connection: {}", e),
    }

    Ok(())
}

async fn process_request(request: &str) -> String {
    if request.contains("/health") {
        "OK".to_string()
    } else if request.contains("/api/v1/orders") {
        "[]".to_string()
    } else if request.contains("/api/v1/trades") {
        "[]".to_string()
    } else if request.contains("/api/v1/darkpool/status") {
        "{\"running\":true,\"total_orders\":0,\"total_trades\":0}".to_string()
    } else if request.contains("/api/v1/flashloanarb/status") {
        let engine = &*FLASH_LOAN_ARB;
        let running = engine.is_running();
        let stats = engine.get_engine_stats().await;
        let pnl = engine.get_pnl().await;
        let trades = engine.get_recent_trades(5).await;
        let recent: Vec<serde_json::Value> = trades.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "success": t.success,
                "profit_usd": t.net_profit_usd,
                "provider": t.provider,
                "duration_ms": t.duration_ms,
                "chain": t.chain,
            })
        }).collect();
        serde_json::json!({
            "engine_running": running,
            "uptime_seconds": stats.uptime_seconds,
            "total_scans": stats.total_scans,
            "pool_count": stats.pool_count,
            "total_opportunities": stats.total_opportunities_found,
            "total_trades_executed": stats.total_trades_executed,
            "total_profit_usd": stats.total_profit_usd,
            "success_rate": pnl.success_rate,
            "successful_trades": pnl.successful_trades,
            "failed_trades": pnl.failed_trades,
            "running_balance_eth": pnl.running_balance_eth,
            "daily_pnl": pnl.daily_pnl,
            "weekly_pnl": pnl.weekly_pnl,
            "monthly_pnl": pnl.monthly_pnl,
            "circuit_breaker": stats.circuit_breaker_active,
            "recent_trades": recent,
        }).to_string()
    } else if request.contains("/api/v1/flashloanarb/pnl") {
        let engine = &*FLASH_LOAN_ARB;
        let pnl = engine.get_pnl().await;
        let log = engine.get_profit_log();
        let profit_log: Vec<serde_json::Value> = log.iter().map(|(k, v)| {
            serde_json::json!({ "period": k, "profit_usd": v })
        }).collect();
        serde_json::json!({
            "total_trades": pnl.total_trades,
            "successful_trades": pnl.successful_trades,
            "failed_trades": pnl.failed_trades,
            "success_rate": pnl.success_rate,
            "total_net_profit_usd": pnl.total_net_profit,
            "total_gas_cost_usd": pnl.total_gas_cost,
            "total_flash_loan_fees": pnl.total_flash_loan_fees,
            "average_profit_per_trade": pnl.average_profit_per_trade,
            "best_trade_usd": pnl.best_trade,
            "worst_trade_usd": pnl.worst_trade,
            "daily_pnl": pnl.daily_pnl,
            "weekly_pnl": pnl.weekly_pnl,
            "monthly_pnl": pnl.monthly_pnl,
            "running_balance_eth": pnl.running_balance_eth,
            "profit_log": profit_log,
        }).to_string()
    } else if request.contains("/api/v1/mev/status") {
        let engine = &*MEV_ENGINE;
        let running = engine.is_running();
        let stats = engine.get_stats().await;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "engine_running": running,
            "uptime_seconds": stats.uptime_seconds,
            "total_scans": stats.total_scans,
            "mempool_size": stats.mempool_size,
            "queue_size": stats.queue_size,
            "last_scan_time": stats.last_scan_time,
            "circuit_breaker": stats.circuit_breaker,
            "total_profit_usd": pnl.total_profit_usd,
            "total_bundles": pnl.total_bundles,
            "success_rate": pnl.success_rate,
            "running_balance_eth": pnl.running_balance_eth,
        }).to_string()
    } else if request.contains("/api/v1/mev/pnl") {
        let engine = &*MEV_ENGINE;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "total_bundles": pnl.total_bundles,
            "confirmed_bundles": pnl.confirmed_bundles,
            "failed_bundles": pnl.failed_bundles,
            "success_rate": pnl.success_rate,
            "total_profit_usd": pnl.total_profit_usd,
            "total_gas_usd": pnl.total_gas_usd,
            "average_profit_usd": pnl.average_profit_usd,
            "best_trade_usd": pnl.best_trade_usd,
            "worst_trade_usd": pnl.worst_trade_usd,
            "daily_pnl": pnl.daily_pnl,
            "weekly_pnl": pnl.weekly_pnl,
            "monthly_pnl": pnl.monthly_pnl,
            "running_balance_eth": pnl.running_balance_eth,
        }).to_string()
    } else if request.contains("/api/v1/mev/trades") {
        let engine = &*MEV_ENGINE;
        let trades = engine.get_recent_trades(10).await;
        let recent: Vec<serde_json::Value> = trades.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "op_type": t.op_type,
                "profit_usd": t.profit_usd,
                "success": t.success,
                "block_number": t.block_number,
                "duration_ms": t.duration_ms,
            })
        }).collect();
        serde_json::json!({ "trades": recent }).to_string()
    } else if request.contains("/api/v1/mev/config") {
        let engine = &*MEV_ENGINE;
        serde_json::json!({
            "scan_interval_ms": engine.config.scan_interval_ms,
            "min_profit_usd": engine.config.min_profit_usd,
            "max_gas_price_gwei": engine.config.max_gas_price_gwei,
            "max_concurrent_bundles": engine.config.max_concurrent_bundles,
            "sandwich_enabled": engine.config.sandwich_enabled,
            "liquidation_enabled": engine.config.liquidation_enabled,
            "backrun_enabled": engine.config.backrun_enabled,
            "max_daily_loss_usd": engine.config.max_daily_loss_usd,
            "max_consecutive_failures": engine.config.max_consecutive_failures,
            "circuit_breaker_cooldown_secs": engine.config.circuit_breaker_cooldown_secs,
            "mev_share_enabled": engine.config.mev_share_enabled,
        }).to_string()
    } else if request.contains("/api/v1/flashloanarb/config") {
        let engine = &*FLASH_LOAN_ARB;
        serde_json::json!({
            "scan_interval_ms": engine.config.scan_interval_ms,
            "min_profit_usd": engine.config.min_profit_usd,
            "min_profit_bps": engine.config.min_profit_bps,
            "max_concurrent_trades": engine.config.max_concurrent_trades,
            "max_daily_loss_usd": engine.config.max_daily_loss_usd,
            "max_consecutive_failures": engine.config.max_consecutive_failures,
            "circuit_breaker_cooldown_secs": engine.config.circuit_breaker_cooldown_secs,
            "enabled_chains": engine.config.enabled_chains,
            "enabled_dexes": engine.config.enabled_dexes,
            "tracked_tokens": engine.config.tracked_tokens,
            "auto_reinvest": engine.config.auto_reinvest,
            "slippage_tolerance_bps": engine.config.slippage_tolerance_bps,
            "profit_sharing_percent": engine.config.profit_sharing_percent,
        }).to_string()
    } else if request.contains("/api/v1/crossvenuarb/status") {
        let engine = &*CROSS_VENUE_ENGINE;
        let running = engine.is_running();
        let stats = engine.get_stats().await;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "engine_running": running,
            "uptime_seconds": stats.uptime_seconds,
            "total_scans": stats.total_scans,
            "opportunities_found": stats.opportunities_found,
            "opportunities_profitable": stats.opportunities_profitable,
            "trades_executed": stats.trades_executed,
            "circuit_breaker": stats.circuit_breaker,
            "total_profit_usd": pnl.total_net_profit_usd,
            "total_trades": pnl.total_trades,
            "success_rate": pnl.success_rate,
        }).to_string()
    } else if request.contains("/api/v1/crossvenuarb/pnl") {
        let engine = &*CROSS_VENUE_ENGINE;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "total_trades": pnl.total_trades,
            "successful_trades": pnl.successful_trades,
            "failed_trades": pnl.failed_trades,
            "success_rate": pnl.success_rate,
            "total_net_profit_usd": pnl.total_net_profit_usd,
            "total_gas_usd": pnl.total_gas_usd,
            "average_profit_usd": pnl.average_profit_usd,
            "best_trade_usd": pnl.best_trade_usd,
            "worst_trade_usd": pnl.worst_trade_usd,
            "daily_pnl": pnl.daily_pnl,
            "weekly_pnl": pnl.weekly_pnl,
            "monthly_pnl": pnl.monthly_pnl,
            "running_balance_usd": pnl.running_balance_usd,
        }).to_string()
    } else if request.contains("/api/v1/crossvenuarb/trades") {
        let engine = &*CROSS_VENUE_ENGINE;
        let trades = engine.get_recent_trades(10).await;
        let recent: Vec<serde_json::Value> = trades.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "pair": t.pair,
                "buy_venue": t.buy_venue,
                "sell_venue": t.sell_venue,
                "profit_usd": t.net_profit_usd,
                "success": t.success,
                "trade_size_usd": t.trade_size_usd,
            })
        }).collect();
        serde_json::json!({ "trades": recent }).to_string()
    } else if request.contains("/api/v1/crossvenuarb/config") {
        let engine = &*CROSS_VENUE_ENGINE;
        serde_json::json!({
            "scan_interval_ms": engine.config.scan_interval_ms,
            "min_profit_usd": engine.config.min_profit_usd,
            "min_profit_bps": engine.config.min_profit_bps,
            "max_trade_size_usd": engine.config.max_trade_size_usd,
            "slippage_bps": engine.config.slippage_bps,
            "gas_estimate_usd": engine.config.gas_estimate_usd,
            "max_concurrent_trades": engine.config.max_concurrent_trades,
            "binance_enabled": engine.config.binance_enabled,
            "coinbase_enabled": engine.config.coinbase_enabled,
            "uniswap_enabled": engine.config.uniswap_enabled,
        }).to_string()
    } else if request.contains("/api/v1/crossvenuarb/prices") {
        let engine = &*CROSS_VENUE_ENGINE;
        let prices = engine.get_prices().await;
        let mut result = std::collections::HashMap::new();
        for (k, v) in &prices {
            let prices_list: Vec<serde_json::Value> = v.iter().map(|p| {
                serde_json::json!({
                    "venue": p.venue,
                    "bid": p.bid,
                    "ask": p.ask,
                    "mid": p.mid,
                    "latency_ms": p.latency_ms,
                })
            }).collect();
            if let Some(last) = prices_list.last() {
                result.insert(k.clone(), last.clone());
            }
        }
        serde_json::json!(result).to_string()
    } else if request.contains("/api/v1/superarb/status") {
        let engine = &*SUPER_ARB_ENGINE;
        let running = engine.is_running();
        let stats = engine.get_stats().await;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "engine_running": running,
            "uptime_seconds": stats.uptime_seconds,
            "total_scans": stats.total_scans,
            "opportunities_found": stats.opportunities_count,
            "trades_executed": stats.trades_count,
            "circuit_breaker": stats.circuit_breaker,
            "total_profit_usd": pnl.total_net_profit_usd,
            "total_trades": pnl.total_trades,
            "success_rate": pnl.success_rate,
            "per_strategy": pnl.per_strategy,
        }).to_string()
    } else if request.contains("/api/v1/superarb/pnl") {
        let engine = &*SUPER_ARB_ENGINE;
        let pnl = engine.get_pnl().await;
        serde_json::json!({
            "total_trades": pnl.total_trades,
            "successful_trades": pnl.successful_trades,
            "failed_trades": pnl.failed_trades,
            "success_rate": pnl.success_rate,
            "total_net_profit_usd": pnl.total_net_profit_usd,
            "daily_pnl": pnl.daily_pnl,
            "weekly_pnl": pnl.weekly_pnl,
            "monthly_pnl": pnl.monthly_pnl,
            "running_balance_usd": pnl.running_balance_usd,
            "per_strategy": pnl.per_strategy,
        }).to_string()
    } else if request.contains("/api/v1/superarb/trades") {
        let engine = &*SUPER_ARB_ENGINE;
        let trades = engine.get_recent_trades(10).await;
        let recent: Vec<serde_json::Value> = trades.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "strategy": t.strategy,
                "pair": t.pair,
                "profit_usd": t.profit_usd,
                "cost_usd": t.cost_usd,
                "net_profit_usd": t.net_profit_usd,
                "success": t.success,
                "error": t.error,
                "duration_ms": t.duration_ms,
            })
        }).collect();
        serde_json::json!({ "trades": recent }).to_string()
    } else if request.contains("/api/v1/superarb/config") {
        let engine = &*SUPER_ARB_ENGINE;
        serde_json::json!({
            "scan_interval_ms": engine.config.scan_interval_ms,
            "min_profit_usd": engine.config.min_profit_usd,
            "max_trade_size_usd": engine.config.max_trade_size_usd,
            "max_concurrent": engine.config.max_concurrent,
            "max_daily_loss_usd": engine.config.max_daily_loss_usd,
            "max_consecutive_failures": engine.config.max_consecutive_failures,
            "circuit_breaker_cooldown_secs": engine.config.circuit_breaker_cooldown_secs,
            "slippage_bps": engine.config.slippage_bps,
            "gas_estimate_usd": engine.config.gas_estimate_usd,
            "cross_venue_enabled": engine.config.cross_venue_enabled,
            "jit_liquidity_enabled": engine.config.jit_liquidity_enabled,
            "staking_arb_enabled": engine.config.staking_arb_enabled,
            "statistical_arb_enabled": engine.config.statistical_arb_enabled,
        }).to_string()
    } else if request.contains("/api/v1/superarb/prices") {
        let engine = &*SUPER_ARB_ENGINE;
        let prices = engine.get_prices().await;
        let mut result = std::collections::HashMap::new();
        for (k, v) in &prices {
            let prices_list: Vec<serde_json::Value> = v.iter().map(|p| {
                serde_json::json!({
                    "venue": p.venue,
                    "pair": p.pair,
                    "bid": p.bid,
                    "ask": p.ask,
                    "mid": p.mid,
                    "latency_ms": p.latency_ms,
                })
            }).collect();
            if let Some(last) = prices_list.last() {
                result.insert(k.clone(), last.clone());
            }
        }
        serde_json::json!(result).to_string()
    } else {
        "{\"error\":\"Not Found\",\"endpoints\":[\"/health\",\"/api/v1/orders\",\"/api/v1/trades\",\"/api/v1/darkpool/status\",\"/api/v1/flashloanarb/status\",\"/api/v1/flashloanarb/pnl\",\"/api/v1/flashloanarb/config\",\"/api/v1/mev/status\",\"/api/v1/mev/pnl\",\"/api/v1/mev/trades\",\"/api/v1/mev/config\",\"/api/v1/crossvenuarb/status\",\"/api/v1/crossvenuarb/pnl\",\"/api/v1/crossvenuarb/trades\",\"/api/v1/crossvenuarb/config\",\"/api/v1/crossvenuarb/prices\",\"/api/v1/superarb/status\",\"/api/v1/superarb/pnl\",\"/api/v1/superarb/trades\",\"/api/v1/superarb/config\",\"/api/v1/superarb/prices\"]}".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = process_request("/health").await;
        assert_eq!(response, "OK");
    }

    #[tokio::test]
    async fn test_orders_endpoint() {
        let response = process_request("/api/v1/orders").await;
        assert_eq!(response, "[]");
    }

    #[tokio::test]
    async fn test_darkpool_status_endpoint() {
        let response = process_request("/api/v1/darkpool/status").await;
        assert!(response.contains("running"));
    }

    #[tokio::test]
    async fn test_flash_loan_arb_status_endpoint() {
        let response = process_request("/api/v1/flashloanarb/status").await;
        assert!(response.contains("engine_running"));
        assert!(response.contains("total_scans"));
    }

    #[tokio::test]
    async fn test_flash_loan_arb_pnl_endpoint() {
        let response = process_request("/api/v1/flashloanarb/pnl").await;
        assert!(response.contains("total_net_profit_usd"));
        assert!(response.contains("success_rate"));
    }

    #[tokio::test]
    async fn test_flash_loan_arb_config_endpoint() {
        let response = process_request("/api/v1/flashloanarb/config").await;
        assert!(response.contains("scan_interval_ms"));
        assert!(response.contains("min_profit_usd"));
    }

    #[tokio::test]
    async fn test_super_arb_status_endpoint() {
        let response = process_request("/api/v1/superarb/status").await;
        assert!(response.contains("engine_running"));
        assert!(response.contains("total_scans"));
    }

    #[tokio::test]
    async fn test_super_arb_pnl_endpoint() {
        let response = process_request("/api/v1/superarb/pnl").await;
        assert!(response.contains("total_trades"));
        assert!(response.contains("success_rate"));
    }

    #[tokio::test]
    async fn test_super_arb_config_endpoint() {
        let response = process_request("/api/v1/superarb/config").await;
        assert!(response.contains("scan_interval_ms"));
        assert!(response.contains("min_profit_usd"));
    }
}
