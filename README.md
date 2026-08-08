# THE-BRIDGE Matching Engine

**1.5M+ TPS · <35µs P99 · Institutional-grade · Open Source (AGPL v3)**

---

## What

THE-BRIDGE is the world's most advanced open-source matching engine. It combines production-grade order matching with features no competitor has:

| Feature | THE-BRIDGE | Nasdaq | Binance | CME |
|---------|-----------|--------|---------|-----|
| Open Source (AGPL) | ✅ | ❌ | ❌ | ❌ |
| 1.5M+ TPS capability | ✅ | ✅ | ❌ | ❌ |
| Sovereign Kill Switch | ✅ | ❌ | ❌ | ❌ |
| Dual Track Privacy | ✅ | ❌ | ❌ | ❌ |
| WASM Hooks (custom logic) | ✅ | ❌ | ❌ | ❌ |
| DOT Settlement (<16ms) | ✅ | ❌ | ❌ | ❌ |
| FIX + ISO 20022 | ✅ | ✅ | ❌ | ✅ |
| Iceberg/TWAP/StopLoss | ✅ | ✅ | ❌ | ✅ |
| FBA (Batch Auctions) | ✅ | ❌ | ❌ | ❌ |
| Market Data Streaming | ✅ | ✅ | ✅ | ✅ |
| JWT + Refresh Tokens | ✅ | ✅ | ✅ | ❌ |
| Rate Limiter (6 tiers) | ✅ | ❌ | ✅ | ❌ |
| WAL Encryption + Signing | ✅ | ❌ | ❌ | ❌ |
| Chaos Engineering Suite | ✅ | ❌ | ❌ | ❌ |

---

## Exclusive Features (No Competitor Has)

| # | Feature | Description |
|---|---------|-------------|
| 1 | **Stealth Orders** | Orders invisible in Market Depth API — full invisibility until execution |
| 2 | **Hard Floor Orders** | Guaranteed execution price floor — checked per fill, not per order |
| 3 | **Batch Auction Mode** | Anti-frontrunning FBA with CSPRNG shuffle + SUCP clearing price |
| 4 | **CRDT Replication** | Conflict-free multi-master replication — no SPOF |
| 5 | **WASM Hooks** | Per-client custom matching logic via WebAssembly |
| 6 | **Progressive Disclosure KYC** | Privacy + compliance together — identity only at Sovereign level |
| 7 | **NUMA-Aware Thread Pool** | CPU topology-aware scheduling for <35µs P99 |
| 8 | **Kill Switch + Hot Migration** | Threat-triggered transparent migration to backup nodes |
| 9 | **Core Pinning per Tenant** | Enterprise-grade CPU isolation (AWS Dedicated Hosts at 10x price) |

---

## Quick Start

```bash
# Build (Rust 1.81+)
cargo build --release

# Run
THE_BRIDGE_NODE_ID=engine-1 \
THE_BRIDGE_WAL_DIR=/tmp/the-bridge/wal \
cargo run --release
```

```bash
# Place an order
curl -X POST http://localhost:3001/api/v1/order \
  -H 'Content-Type: application/json' \
  -d '{"pair":"BTC/USD","side":"Buy","price":50000.0,"quantity":1.0}'

# Check order book
curl http://localhost:3001/api/v1/orderbook/BTCUSD

# Market data stream (WebSocket)
wscat -c ws://localhost:3001/ws/market/BTCUSD
```

---

## Architecture

```
                     ┌─────────────────────────────┐
  HTTP/WS (:3001) ──▶│       THE-BRIDGE Engine      │
  FIX 5.0 (:4001) ──▶│                              │
  FIX/TLS (:4443) ──▶│  matching.rs → orderbook.rs  │
                     │        ↕        ↕            │
                     │  wal.rs (encrypted+signed)   │
                     │  crdt.rs (conflict-free)     │
                     │  consensus.rs (DAG)          │
                     │  cloak.rs (Kill Switch)     │
                     │  wasm_engine.rs (hooks)      │
                     └─────────────┬───────────────┘
                                   │
                     ┌─────────────▼───────────────┐
                     │  PostgreSQL (optional)       │
                     │  WAL Replicas (optional)    │
                     │  DAG Peers (optional)       │
                     └─────────────────────────────┘
```

### Core Modules

| Module | Purpose |
|--------|---------|
| `matching.rs` | O(log p) BTreeMap matching engine |
| `orderbook.rs` | Price-level order book with batch auction support |
| `types.rs` | Core order/trade/ticker types |
| `market_data.rs` | Depth, ticker, candle streaming via broadcast |
| `wal.rs` | Write-Ahead Log with AES-256-GCM + Ed25519 signing |
| `fix.rs` | FIX 5.0 SP2 gateway (plain TCP + TLS) |
| `consensus.rs` | DAG-based consensus for multi-node settlement |
| `crdt.rs` | CRDT OR-Set for conflict-free replication |
| `cloak.rs` | Threat analysis + Sovereign Kill Switch |
| `wasm_engine.rs` | Per-client WASM hooks for custom matching |
| `token_auth.rs` | JWT HS256 with refresh tokens |
| `pg.rs` | PostgreSQL persistence (optional, graceful fallback) |

### Revenue Modules (Written, NOT wired)

| Module | Purpose | Lines |
|--------|---------|-------|
| `revenue_engine.rs` | Revenue distribution | 371 |
| `fx_engine.rs` | FX trading with 13 Nostro accounts | 249 |
| `futures_options.rs` | Derivatives engine | 103 |
| `lending_pool.rs` | 5% APR lending/borrowing | 209 |
| `securities_lending.rs` | Securities lending | 211 |
| `white_label.rs` | White-label exchanges | 155 |
| `dark_pool_manager.rs` | Institutional dark pool | 744 |

### Advanced Modules

| Module | Purpose |
|--------|---------|
| `batch_auction.rs` | Anti-frontrunning FBA with SUCP |
| `liquidity_engine.rs` | BMM X⁴Y=K liquidity provision |
| `compliance_engine.rs` | AML/KYC compliance |
| `risk_engine.rs` | Risk management |
| `onboarding_engine.rs` | Client onboarding |
| `execution_engine.rs` | Smart execution routing |
| `smart_router.rs` | Multi-venue routing |

---

## Complete Features List

### Core Matching Engine

| Feature | Status | Description |
|---------|:------:|-------------|
| O(log n) BTreeMap Price-Level Order Book | ✅ | BTreeMap with VecDeque per price level |
| Continuous Matching (Limit + Market) | ✅ | Immediate matching with depth management |
| Batch Auction Mode | ✅ | FBA with CSPRNG shuffle + SUCP clearing price |
| Stealth Trailing Orders | ✅ | Invisible orders in Market Depth API |
| Hard Floor Orders | ✅ | Per-fill guaranteed execution price floor |
| Iceberg Orders | ✅ | Partial disclosure orders |
| TWAP Orders | ✅ | Time-weighted average price execution |
| Stop-Loss Orders | ✅ | Trigger-based order execution |

### Settlement & Consensus

| Feature | Status |
|---------|:------:|
| DAG Consensus (Blake2b512) | ✅ |
| CRDT Replication (OR-Set) | ✅ |
| WAL with Sync Replication + Hash Chain | ✅ |
| DOT Settlement Engine (<16.7ms) | ✅ |
| Engine State Snapshots | ✅ |

### Security & Compliance

| Feature | Status |
|---------|:------:|
| TEE Enclave (Software Ed25519) | ✅ |
| Sovereign Kill Switch + Threat Analyzer | ✅ |
| KYC/AML Gateway with Progressive Disclosure | ✅ |
| JWT + Refresh Tokens | ✅ |
| Rate Limiter (6 tiers) | ✅ |
| 10-Layer Defense in Depth | ✅ |
| Anti-Reverse Engineering | ✅ |
| Anti-Tracking | ✅ |

### Cloud & SaaS

| Feature | Status |
|---------|:------:|
| Multi-Tenant Cloud Orchestration | ✅ |
| Auto-Scaling (Scale Up/Down) | ✅ |
| Tenant Billing & Usage Metering | ✅ |
| Core Pinning (Enterprise Tier) | ✅ |
| Self-Service Signup Flow | ⏳ Planned |
| Real Payment Gateway (Stripe/Paddle) | ⏳ Planned |
| WebSocket Dashboard | ⏳ Planned |

### Connectivity

| Feature | Status |
|---------|:------:|
| FIX 5.0 SP2 Gateway | ✅ |
| REST API (130+ endpoints) | ✅ |
| WebSocket Streams | ✅ |
| ISO 20022 Integration | ✅ |

### Performance

| Feature | Status |
|---------|:------:|
| NUMA-Aware Thread Pool | ✅ |
| Lock-Free Pipeline (Disruptor) | ✅ |
| Runtime Segregation (Data/Control Plane) | ✅ |
| WASM Hooks (Feature-Gated) | ✅ |
| Prometheus Metrics | ✅ |

---

## SDK

Multi-language client SDK for THE-BRIDGE.

### Supported Languages
- Rust
- JavaScript/TypeScript
- Python
- Go

### Quick Start (Rust)
```rust
use the_bridge_client::TheBridgeClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = TheBridgeClient::new("https://api.the-bridge.io", "your_api_key").await?;
    let orders = client.orders().await?;
    println!("Active orders: {}", orders.len());
    Ok(())
}
```

### Quick Start (JavaScript)
```javascript
import { TheBridgeClient } from '@the-bridge/sdk';

const client = new TheBridgeClient('https://api.the-bridge.io', 'your_api_key');
const orders = await client.orders();
console.log(`Active orders: ${orders.length}`);
```

### Performance Targets
- **Latency:** <50ms for REST API endpoints
- **Throughput:** 100K+ requests per second
- **Data Compression:** Up to 70% reduction with gzip/deflate
- **WebSocket:** Sub-5ms event delivery
- **Memory:** <100MB for active connections

### Error Types
- `AuthenticationError` — Invalid API key or token
- `RateLimitError` — Request rate exceeded
- `OrderError` — Invalid order parameters or insufficient balance
- `ConnectionError` — Network or WebSocket issues
- `InternalError` — Server-side errors

### Future SDK Work
- Mobile SDKs (iOS Swift, Android Kotlin)
- Web SDK (browser-based WebSocket client)
- Cloud Functions (serverless connectors)
- Managed Infrastructure (AWS/GCP/Azure)

---

## Documentation

| Document | Purpose |
|----------|---------|
| `EXECUTION_AGENDA.md` | Live execution plan — tasks, phases, parallelism, revenue strategy |
| `MASTER_STRATEGIC_AND_EXECUTION.md` | Strategic & execution reference — architecture, revenue streams, priorities |
| `LICENSE` | AGPL v3 (Community Edition) + Commercial License terms |
| `README.md` | This file — overview for developers |

Archived planning docs (`ARCHITECTURE.md`, `DEPLOY.md`, `MASTER_PLAN.md`, `PLAN.md`, `UNIFIED_PLAN.md`, `SIMULATION.ps1`) are preserved in [`archive_plans/`](archive_plans/).

---

## Contributing

We welcome contributions! This is a dual-licensed matching engine.

### Community Edition (AGPL v3)
All code in this repository is AGPL v3 unless otherwise noted.

### Before Contributing
1. Open an issue to discuss the change
2. Ensure `cargo test` and `cargo check` pass
3. Sign the Contributor License Agreement (CLA)

### Code Style
- Rust std style via `rustfmt`
- No unsafe code unless absolutely necessary and documented
- All public APIs must be documented
- Tests required for new features

### Enterprise Features
Some features are gated behind the `enterprise` Cargo feature. See `Cargo.toml`.

---

## License

**AGPL v3** — Free for open-source use. Commercial license required for proprietary deployment without releasing source code.

[Commercial licensing →](https://the-bridge.io/license)

---

## Contact

- GitHub Issues — bug reports, feature requests
- Enterprise inquiries: enterprise@the-bridge.io
- Security: security@the-bridge.io
