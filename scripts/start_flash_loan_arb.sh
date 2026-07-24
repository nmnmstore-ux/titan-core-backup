#!/bin/bash
# Flash Loan Arbitrage Engine - Startup Script
source /home/mohamednoureldinrefaay/.cargo/env

export ETH_RPC_URL=" https://eth-mainnet.g.alchemy.com/v2/demo\
export BSC_RPC_URL=\https://bsc-dataseed.binance.org\
export POLYGON_RPC_URL=\https://polygon-rpc.com\
export ARB_RPC_URL=\https://arb1.arbitrum.io/rpc\
export OP_RPC_URL=\https://mainnet.optimism.io\

export SCAN_INTERVAL_MS=2000
export MIN_PROFIT_USD=20
export MIN_PROFIT_BPS=10
export MAX_CONCURRENT_TRADES=3
export MAX_GAS_PRICE_GWEI=100
export SLIPPAGE_TOLERANCE_BPS=30
export MAX_DAILY_LOSS_USD=5000
export MAX_CONSECUTIVE_FAILURES=5
export CIRCUIT_BREAKER_COOLDOWN_SECS=300

cd /home/mohamednoureldinrefaay/the-bridge/A-core-infrastructure/matching-engine

echo \[Flash Loan Arb] Building release...\
cargo build --release 2>&1 | tail -5

echo \[Flash Loan Arb] Starting server on port 8080...\
./target/release/matching-engine
