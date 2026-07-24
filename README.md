# THE-BRIDGE Matching Engine

**1.5M+ TPS · <35µs P99 · Institutional-grade · CEFI-only**

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

## Quick Start

```bash
# Build (Rust 1.81+)
cd matching-engine
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

## Modules

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

## Why Open Source?

1. **Trust** — Auditable matching logic for institutions
2. **Distribution** — Developer community adoption drives enterprise sales
3. **Innovation** — Community contributions improve the engine for everyone

## License

**AGPL v3** — Free for open-source use. Commercial license required for proprietary deployment without releasing source code.

[Commercial licensing →](https://the-bridge.io/license)

## Contact

- GitHub Issues — bug reports, feature requests
- Enterprise inquiries: enterprise@the-bridge.io
- Security: security@the-bridge.io
