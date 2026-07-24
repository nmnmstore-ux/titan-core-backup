# THE-BRIDGE — Agent Operations Manual

## 1. System Overview

THE-BRIDGE is a production-grade institutional matching engine that combines:
- **O(log n) order matching** via BTreeMap price-level grouping
- **Ed25519 cryptographic signatures** via TEE enclave
- **FIX 5.0 SP2 protocol** for institutional banking connectivity
- **DAG-based consensus** for decentralized settlement
- **CRDT replication** for conflict-free multi-node operation
- **WAL with sync replication** for crash-safe persistence
- **WASM hooks** for per-client custom matching logic
- **Sovereign Kill Switch** with threat analysis and hot migration

Target: 1.5M TPS, <35µs P99 latency, <16.7ms DOT settlement.

## 2. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     THE-BRIDGE ENGINE                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  REST API (:3001)  ────  OrderBookManager ────  Matching   │
│       │                          │                          │
│       │                    ┌─────┴──────┐                   │
│       │                    │            │                   │
│  FIX Gateway (:4001)   WAL Replication  WASM Hooks         │
│       │                    │            │                   │
│       ├── JPMorgan         ├── Replica 1  ├── hook_a.wasm  │
│       ├── Goldman Sachs    ├── Replica 2  ├── hook_b.wasm  │
│       ├── Deutsche Bank    ├── Replica 3  └── hook_c.wasm  │
│       ├── Barclays         └── Replica N                   │
│       ├── HSBC                                             │
│       ├── BNP Paribas           DAG Consensus (:4002)      │
│       ├── Citi                    ├── Peer 1              │
│       └── Morgan Stanley          ├── Peer 2              │
│                                    ├── Peer 3              │
│  Threat Analyzer                  └── Peer N               │
│       └── Kill Switch ── Hot Migration ── Backup Nodes     │
│                                                             │
│  TEE Enclave (Ed25519)                                     │
│       ├── Key generation (OsRng)                           │
│       ├── Sign/Verify                                      │
│       ├── Attestation                                      │
│       └── Key rotation                                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 3. Module Map

| Module | File | Responsibility |
|--------|------|---------------|
| `types` | `src/types.rs` | Core types: Order, Trade, OrderBookSummary, enums |
| `orderbook` | `src/orderbook.rs` | BTreeMap-based order book, price_key=*(price*10000) |
| `matching` | `src/matching.rs` | O(log n) matching engine, execute_market() |
| `dot` | `src/dot.rs` | DOT transfer engine, Ed25519 signing via TEE |
| `tee` | `src/tee.rs` | HardwareEnclave trait, TEEEnclave (software Ed25519) |
| `fix` | `src/fix.rs` | FIX 5.0 SP2 TCP gateway, session management |
| `wal` | `src/wal.rs` | Write-Ahead Log, CRC32, sync replication |
| `consensus` | `src/consensus.rs` | DAG consensus, Blake2b512, gossip protocol |
| `crdt` | `src/crdt.rs` | CRDT OR-Set, version vectors, merge |
| `wasm_engine` | `src/wasm_engine.rs` | WASM hooks runtime (wasmtime, optional) |
| `cloak` | `src/cloak.rs` | Threat analyzer, Sovereign Kill Switch |
| `numa` | `src/numa.rs` | NUMA-aware allocation, thread pool |
| `metrics` | `src/metrics.rs` | Prometheus metrics, latency histograms |
| `snapshot` | `src/snapshot.rs` | Engine state snapshots for migration |
| `main` | `src/main.rs` | Entry point, API routes, system wiring |

## 4. Flow: Order Placement (critical path)

```
1. Client → POST /api/v1/order  (HTTP)
2. main::place_order handler called
3. state.kill_switch.threat_analyzer.record_request  ← rate limit tracking
4. state.wasm_hook.on_place(&order)                   ← WASM validation
5. state.dot.validate_order(&order)                   ← DOT rules check
6. state.metrics.inc_orders()                         ← metrics
7. state.wal.append(PlaceOrder)                       ← WAL (crash-safe)
8. state.consensus.submit(PlaceOrder)                 ← DAG consensus
9. state.crdt.apply_add(order)                        ← CRDT replication
10. state.books.place_order(order)                    ← ACTUAL MATCHING
11. state.metrics.inc_trades_by(result.trades.len())  ← metrics
12. state.wal.append(TradeSettled)                    ← WAL trade record
13. Return result to client
```

## 5. How to Modify

### 5.1 Adding a new API route
1. Add a handler function in `main.rs` (e.g., `async fn my_route(...)`)
2. Register it in the Router chain (`.route("/api/v1/...", get(my_route))`)
3. Add `AppState` fields if needed
4. Add tests in `tests/`

### 5.2 Adding a new order type
1. Add variant to `OrderType` enum in `types.rs`
2. Add matching logic in `matching.rs`
3. Add serialization support
4. Update DOT validation in `dot.rs`

### 5.3 Adding a new FIX message type
1. Add msg_type constant in `fix.rs`
2. Add parsing logic in `parse_fix_message()`
3. Add response handler in the session loop
4. Add seqnum tracking

### 5.4 Changing the matching algorithm
- Currently O(log p) with BTreeMap + VecDeque
- To change: modify `matching.rs` `execute_market()` function
- The price key format: `(price * PRICE_MULTIPLIER).round() as i64` (defined in `orderbook.rs`)

### 5.5 Adding a new database/backend
- Currently: no database (in-memory with WAL persistence)
- To add persistence: extend `wal.rs` with new `WALRecord` variants
- To add SQL: place after WAL append, before consensus submit

## 6. Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THE_BRIDGE_NODE_ID` | `engine-1` | Unique node identifier |
| `THE_BRIDGE_PEERS` | `` | DAG consensus peers (comma-separated host:port) |
| `THE_BRIDGE_REPLICAS` | `` | WAL replicas (comma-separated host:port) |
| `THE_BRIDGE_WAL_DIR` | `/var/lib/the-bridge/wal` | WAL storage directory |
| `THE_BRIDGE_HOOKS_DIR` | `/etc/the-bridge/hooks` | WASM hooks directory |
| `RUST_LOG` | `the_bridge=info` | Log level |
| `RUST_BACKTRACE` | `1` | Backtrace on error |

## 7. Ports

| Port | Protocol | Service |
|------|----------|---------|
| 3001 | HTTP/REST | API server |
| 4001 | TCP/FIX | FIX 5.0 SP2 gateway |
| 4002 | TCP/gossip | DAG consensus |

## 8. Cargo Features

- `wasm` — enables `wasmtime` runtime for WASM hooks
- `default` — minimal build for production without WASM

## 9. Testing

```bash
# Run all tests
cargo test --release -- --nocapture

# Run specific test
cargo test --release --test integration_test -- --nocapture

# Stress test (requires NUMA hardware)
cargo test --release --test stress_test -- --nocapture

# Cloak kill-switch tests
cargo test --release --test cloak_test -- --nocapture

# DOT settlement tests
cargo test --release --test dot_settlement_test -- --nocapture
```

## 10. Production Tuning

```bash
# CPU governor
cpupower frequency-set -g performance

# Disable hyperthreading for latency
echo off > /sys/devices/system/cpu/smt/control

# Hugepages
echo 2048 > /proc/sys/vm/nr_hugepages

# Network
sysctl -w net.core.rmem_max=134217728
sysctl -w net.core.wmem_max=134217728
sysctl -w net.core.netdev_budget=600
```

## 11. Emergency Procedures

### Kill Switch Manual Activation
```bash
curl -X POST http://localhost:3001/api/v1/sovereign/shield
```

### WAL Recovery After Crash
```bash
# WAL auto-recovers on startup
# Check health:
curl http://localhost:3001/api/v1/wal/status
```

### Full Node Reset
```bash
rm -rf /var/lib/the-bridge/wal/*
systemctl restart the-bridge
```

## 12. Code Conventions

- **No panics/unwraps/expect in production paths** — use `Result<_, String>` everywhere
- **No dead code** — every module must be wired in `main.rs`
- **Every edge case tested** — integration tests in `tests/`
- **Constants over magic numbers** — `PRICE_MULTIPLIER=10_000`, `MAX_BATCH_SIZE=64`
- **Arabic/English bilingual documentation** — code comments in English, operational docs in Arabic

---

## 13. Architect Profile & Workflow Preferences

- **Name / Handle**: (user prefers to stay anonymous — referenced as "Architect")
- **Role**: Sovereign FinTech Architect — vision holder, makes all strategic decisions
- **Workflow**: Sends proposals / ideas → expects brutally honest technical assessment → decides whether to implement
- **Language preference**: Arabic for strategy/concepts, English for code
- **Expectation from agents**: Direct unfiltered technical opinion ("راي ضريخ"), no fluff, no diplomacy
- **Decision rule**: Architect decides after hearing pros/cons — agents execute, don't gatekeep
- **Key principle**: "نطور ونرتقع مش نعمل حاجه ترجعنا" — every change must advance the system, never regress
- **Currently supported proposals (saved for reference)**:
  - **P1 — Confidential Computing (SGX/SEV Enclave Layer)**: Heavy latency tax (~10-15%+), impractical for 35µs P99 target. Keep current TEEEnclave (software Ed25519) as base. SGX/SEV = future optional hardware tier only.
  - **P2 — Anti-Frontrunning Batch Auctions (Frequent Batch Auctions)**: ✅ Approved architecture direction. Implement as `BatchAuction` mode in `matching.rs` alongside existing `Continuous` mode. Config-switchable. Destroys colocation advantage. Top priority feature.
  - **P3 — ZK Clearing Bridges (T+0 Settlement via ZK-Proofs)**: Computationally infeasible at 1.5M TPS today (proof generation = ms/s per trade, not ns). Long-term roadmap only. Current WAL + CRDT + DAG consensus is the practical settlement path.
- **Build standard**: Must always pass `cargo build` + `cargo check --tests` with 0 errors before any submission
