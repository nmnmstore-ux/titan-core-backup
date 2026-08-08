# THE-BRIDGE — Technical Architecture Reference

> **Status:** Living Document | **Last Updated:** 2026-07-27
> **Language:** Arabic (strategy) + English (code)

---

## TABLE OF CONTENTS

1. [System Overview](#1-system-overview)
2. [10-Layer Security](#2-10-layer-security)
3. [Network Architecture & DAG](#3-network-architecture--dag)
4. [Module Map](#4-module-map)
5. [API Endpoints (130+)](#5-api-endpoints-130)
6. [Architectural Decisions (9)](#6-architectural-decisions-9)
7. [Tier Architecture (4 Layers)](#7-tier-architecture-4-layers)
8. [CPU Core Layout](#8-cpu-core-layout)
9. [Order Placement Critical Path](#9-order-placement-critical-path)
10. [How to Modify](#10-how-to-modify)

---

## 1. System Overview

THE-BRIDGE is a production-grade institutional matching engine combining:
- **O(log n) order matching** via BTreeMap price-level grouping
- **Ed25519 cryptographic signatures** via TEE enclave
- **FIX 5.0 SP2 protocol** for institutional banking connectivity
- **DAG-based consensus** for decentralized settlement
- **CRDT replication** for conflict-free multi-node operation
- **WAL with sync replication** for crash-safe persistence
- **WASM hooks** for per-client custom matching logic
- **Sovereign Kill Switch** with threat analysis and hot migration

Target: 1.5M TPS, <35µs P99 latency, <16.7ms DOT settlement.

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

---

## 2. 10-Layer Security

### 2.1 Defense in Depth

```
Layer  10: Application Logic Validation
Layer   9: WASM Hook Sandbox
Layer   8: Rate Limiting + Threat Analysis
Layer   7: DOT Dual-Signature Settlement
Layer   6: CRDT Conflict Resolution
Layer   5: DAG Consensus Finalization
Layer   4: WAL Crash Recovery
Layer   3: Memory Encryption + mlock
Layer   2: TEE Enclave (Ed25519)
Layer   1: Binary Obfuscation + Anti-Debug
```

### 2.2 Anti-Reverse-Engineering

| Technique | Implementation | Effectiveness |
|-----------|---------------|---------------|
| Strip symbols | `strip = true` in Cargo.toml | Prevents function name visibility |
| Encrypt strings | All literals XOR-encrypted at compile time | Prevents string extraction |
| Control flow obfuscation | LLVM passes + custom | Prevents flow analysis |
| Anti-debugging | `ptrace` detection + timing checks | Prevents debugging |
| Self-checksumming | Integrity verification at startup | Detects binary modification |
| No debug symbols | `codegen-units=1` prevents inlining leaks | Prevents source correlation |

### 2.3 Anti-Tracking

- Every FIX session gets random SenderCompID
- Heartbeat timing randomized (±30%) to defeat pattern analysis
- DAG gossip via proxy random
- No logging of client IPs after threat analysis window
- Traffic padding to nearest 1KB
- MAC address randomization in P2P connections

### 2.4 Transport Security

**FIX Gateway (Port 4001) — TLS 1.3**
- TLS keys rotate every 24h
- Perfect Forward Secrecy with X25519
- CA-issued certificates (internal, not public)
- Per-session keys

**REST API (Port 3001) — TLS 1.3 + mTLS**
- Client certificate required
- Rate limiting per certificate

**DAG Consensus (Port 4002) — Noise Protocol**
```
Handshake: Noise_XK_25519_ChaChaPoly_BLAKE2s
  - X: initiator sends static key
  - K: responder knows initiator key beforehand
  - 25519: Diffie-Hellman on Curve25519
  - ChaChaPoly: symmetric encryption
  - BLAKE2s: hash
```

**WAL Replication**
- Pre-shared key from ENV
- AES-256-GCM per record
- Nonce = seq_num (replay prevention)

### 2.5 Memory Protection

```rust
// All private keys in isolated memory region
pub struct Secret<T: Zeroize> {
    data: T,
    // mlock() prevents swapping
    // mprotect(PROT_NONE) after use
    // Zeroize on drop
}
```

- Every `SigningKey` zeroized on Drop
- Stack cleared after each sign operation
- Heap buffers zeroed before `free`
- Cache line flushing after sensitive operations
- Guard pages between sensitive objects (PROT_NONE)
- ASLR for random page placement

### 2.6 TEE Enclave Architecture

**Software TEE (Developer Mode)**
- ed25519-dalek with OsRng
- Keys encrypted with machine-derived factor
- Never written to disk
- Zeroized on process exit

**Hardware TEE (Production Mode)**
```
┌──────────────────────────────────────┐
│           Intel SGX Enclave          │
│  ┌────────────────────────────────┐  │
│  │  Signing Key (generated in SGX)│  │
│  │  • OS cannot read              │  │
│  │  • Hypervisor cannot read      │  │
│  │  • Physical access cannot read │  │
│  └────────────────────────────────┘  │
│  ┌────────────────────────────────┐  │
│  │  Attestation (DCAP)            │  │
│  │  • Proves original code runs   │  │
│  │  • Remote verification         │  │
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
```

### 2.7 Key Rotation

- Every 24h: automatic rotation
- Old key accepts transfers for 1h
- Rotation signed with TEE attestation
- Any failure stops engine immediately

### 2.8 Threat Levels & Kill Switch

| Level | Threshold | Action |
|-------|-----------|--------|
| Green | Normal | Monitoring only |
| Yellow | >100 req/s from single IP | Log + alert |
| Orange | >1000 req/s or anomaly | Rate limit + verify |
| Red | >10000 req/s or attack confirmed | Hot migration |
| Black | System compromised | Emergency shutdown + evidence capture |

**Kill Switch Chain:**
```
Threat → analyze() → Red/Black?
  → Hot Migration (save all state)
  → Cloaking (hide identity)
  → Evidence snapshot (forensic data)
  → Backup node takeover
  → Primary node sunset (self-destruct)
```

**Self-Destruct Protocol:**
1. Zero all keys
2. Wipe WAL
3. Wipe memory
4. Shred disk sectors
5. Stop self
6. Backup takes over

### 2.9 Bank-Grade Compliance

| Standard | Status | Notes |
|----------|--------|-------|
| SOC 2 Type II | Built-in | Audit trail + access controls |
| PCI-DSS | Encrypted everywhere | No plaintext secrets |
| GDPR | No PII in logs | IPs anonymized |
| MiFID II | FIX 5.0 SP2 | Full order record-keeping |
| Basel III | WAL + CRDT | Settlement finality |
| FATF Travel Rule | DOT tracking | Transfer audit chain |
| ISO 27001 | Architecture | Design principles aligned |

### 2.10 Compromise Scenarios

| Scenario | Result |
|----------|--------|
| Attacker gains OS access | Can't read TEE, can't modify engine, can't steal keys. Kill Switch activates. |
| Reverse engineering | Can't find names, can't extract strings, can't trace execution. Binary is a black box. |
| Network interception | Can't read FIX (TLS 1.3), can't read DAG (Noise), can't replay (nonce). Network is a black box. |
| Insider threat | Can't access keys (TEE), can't modify logic (integrity check), can't hide actions (WAL+DAG immutable). |

### 2.11 Footprint Elimination

```
Domain:      the-bridge.io (not in public records)
Servers:     Private hosting, not public cloud
Certificates: Let's Encrypt with DNS-01 (hidden)
DNS:         NS records encrypted, not in public resolvers
Code:        Private git, not on GitHub
Team:        Each member knows only their part (need-to-know)
Traffic:     All ports TLS 1.3, no clear text
Timing:      Random padding hides traffic patterns
Identity:    Each node knows only the next node's address
```

---

## 3. Network Architecture & DAG

### 3.1 DAG vs Blockchain

```
Bitcoin / Ethereum (blockchain):
    blocks ← blocks ← blocks
    Every block follows the previous
    Slow (seconds per block)

THE-BRIDGE (DAG):
    tx ← tx ← tx ← tx
           ↙     ↘
         tx     tx
    Every transaction references the previous
    Fast (microseconds)
    No blocks
    No waiting
```

### 3.2 The Three Layers

**Layer 1: DAG Consensus (Matching Engine)**
- Records all transactions
- Computes balances
- Confirms settlement
- Runs the DRM Token
- Speed: 1M+ TPS
- Finality: microseconds (not seconds/minutes)

**Layer 2: Mesh Network (libp2p)**
- Device-to-device communication without internet
- Bluetooth / WiFi Direct
- Cannot be shut down
- Every node = complete copy of DAG

**Layer 3: External Blockchains (Ethereum / BSC / Polygon)**
- Only for Uniswap / Aave / Flash Loans
- Simple bridge, nothing more
- Not needed for Token or DAO

### 3.3 Smart Contracts on DAG

The Smart Contracts in `Z-smart-contracts/` (Solidity):
- THE-BRIDGEDAO.sol
- DRS.sol
- UnilateralRecovery.sol
- RWA.sol
- DRMToken.sol

Can run on:
a) Ethereum / BSC / Polygon (traditional)
b) Directly in Rust inside the DAG (better)

Best solution: Write them in Rust and run inside the matching engine. No Gas fees, no confirmation waits.

---

## 4. Module Map

### Core Engine (13 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `types` | `src/types.rs` | Core types: Order, Trade, OrderBookSummary, enums |
| `orderbook` | `src/orderbook.rs` | BTreeMap-based order book, price_key=(price*10000) |
| `matching` | `src/matching.rs` | O(log n) matching engine, execute_market() |
| `execution_engine` | `src/execution_engine.rs` | Smart execution routing |
| `market_data` | `src/market_data.rs` | Depth, ticker, candle streaming |
| `smart_router` | `src/smart_router.rs` | Multi-venue cost/latency/reliability routing |
| `liquidity_engine` | `src/liquidity_engine.rs` | BMM X⁴Y=K liquidity provision |
| `pipeline` | `src/pipeline.rs` | Lock-free Disruptor pipeline |
| `orchestrator` | `src/orchestrator.rs` | Pipeline orchestration |
| `handlers` | `src/handlers.rs` | API request handlers |
| `batch_auction` | `src/batch_auction.rs` | FBA anti-frontrunning |
| `circuit_breaker` | `src/circuit_breaker.rs` | 3-Level circuit breaker |
| `numa` | `src/numa.rs` | NUMA-aware thread pool |

### Settlement & Consensus (7 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `dot` | `src/dot.rs` | DOT settlement, Ed25519 signing |
| `tee` | `src/tee.rs` | HardwareEnclave trait, TEEEnclave |
| `consensus` | `src/consensus.rs` | DAG consensus, Blake2b512 |
| `crdt` | `src/crdt.rs` | CRDT OR-Set, version vectors |
| `wal` | `src/wal.rs` | Write-Ahead Log, CRC32, sync replication |
| `snapshot` | `src/snapshot.rs` | Engine state snapshots |
| `backup` | `src/backup.rs` | Backup management |

### Security & Privacy (9 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `cloak` | `src/cloak.rs` | Threat analyzer, Sovereign Kill Switch |
| `anti_debug` | `src/anti_debug.rs` | Anti-debugging detection |
| `encrypted` | `src/encrypted.rs` | Encryption primitives |
| `encrypted_mempool` | `src/encrypted_mempool.rs` | Encrypted mempool |
| `threshold_crypto` | `src/threshold_crypto.rs` | DKG + ElGamal + ZK |
| `sovereign` | `src/sovereign.rs` | Sovereign Layer |
| `sovereign_fortress` | `src/sovereign_fortress.rs` | Fortress + Dead Man |
| `sovereign_protocol` | `src/sovereign_protocol.rs` | Sovereign Protocol |
| `gatekeeper` | `src/gatekeeper.rs` | Access control |

### Compliance & Identity (6 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `kyc` | `src/kyc.rs` | KYC verification |
| `auth` | `src/auth.rs` | JWT + MFA |
| `token_auth` | `src/token_auth.rs` | Token authentication |
| `counterparty` | `src/counterparty.rs` | Counterparty management (Bloom filters) |
| `compliance_engine` | `src/compliance_engine.rs` | AML/KYC compliance |
| `onboarding_engine` | `src/onboarding_engine.rs` | Client onboarding |

### Financial Rails (6 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `fix` | `src/fix.rs` | FIX 5.0 SP2 gateway (:4001) |
| `iso20022` | `src/iso20022.rs` | ISO 20022 XML generation |
| `web3_integration` | `src/web3_integration.rs` | Web3 + UnilateralRecovery |
| `wasm_engine` | `src/wasm_engine.rs` | WASM hooks runtime |
| `universal_bridge` | `src/universal_bridge.rs` | Universal Bridge |
| `ghost_integration` | `src/ghost_integration.rs` | Ghost Protocol |

### Revenue Modules (7 files)
| Module | File | Lines |
|--------|------|-------|
| `revenue_engine` | `src/revenue_engine.rs` | 371 |
| `fx_engine` | `src/fx_engine.rs` | 249 |
| `futures_options` | `src/futures_options.rs` | 103 |
| `lending_pool` | `src/lending_pool.rs` | 209 |
| `securities_lending` | `src/securities_lending.rs` | 211 |
| `white_label` | `src/white_label.rs` | 155 |
| `prime_brokerage` | `src/prime_brokerage.rs` | 232 |

### AI & Infrastructure (6 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `ai_agent` | `src/ai_agent.rs` | AI Agent main |
| `llm_sidecar` | `src/llm_sidecar.rs` | Local LLM AR/EN |
| `dashboard` | `src/dashboard.rs` | Dashboard |
| `metrics` | `src/metrics.rs` | Prometheus metrics |
| `memory` | `src/memory.rs` | Memory management |
| `io` | `src/io.rs` | I/O operations |

### Dark Pool & Advanced Trading (4 files)
| Module | File | Responsibility |
|--------|------|---------------|
| `dark_pool_manager` | `src/dark_pool_manager.rs` | Dark pool management |
| `dark_pool_orchestrator` | `src/dark_pool_orchestrator.rs` | 7-component coordination |
| `liquidation` | `src/liquidation.rs` | Liquidation engine |
| `risk_engine` | `src/risk_engine.rs` | Risk management |

### Cloud Module
| Module | File | Responsibility |
|--------|------|---------------|
| `cloud/mod` | `src/cloud/mod.rs` | Module re-exports |
| `cloud/billing` | `src/cloud/billing.rs` | Billing meters, invoices |
| `cloud/payment` | `src/cloud/payment.rs` | Payment processing |
| `cloud/tenant` | `src/cloud/tenant.rs` | Tenant management |
| `cloud/dashboard` | `src/cloud/dashboard.rs` | Cloud dashboard |
| `cloud/orchestrator` | `src/cloud/orchestrator.rs` | Cloud orchestration |
| `cloud/apikey` | `src/cloud/apikey.rs` | API key management |

### Smart Contracts (7 Solidity)
| Contract | File | Purpose |
|----------|------|---------|
| DRMToken | `Z-smart-contracts/DRMToken.sol` | ERC20 stablecoin |
| DRMBurner | `Z-smart-contracts/DRMBurner.sol` | Token burning |
| RWA | `Z-smart-contracts/RWA.sol` | Real-World Assets |
| DRS | `Z-smart-contracts/DRS.sol` | Decentralized Revenue Sharing |
| THEBridgeDAO | `Z-smart-contracts/THEBridgeDAO.sol` | DAO governance |
| UnilateralRecovery | `Z-smart-contracts/UnilateralRecovery.sol` | Account recovery |
| MockPriceOracle | `Z-smart-contracts/MockPriceOracle.sol` | Price oracle mock |

### AI Agents (4)
| Agent | Path | Purpose |
|-------|------|---------|
| Compliance Agent | `B-ai-agents/compliance/` | AML/KYC monitoring |
| Growth Agent | `B-ai-agents/growth/` | User acquisition |
| Pricing Agent | `B-ai-agents/pricing/` | Dynamic pricing |
| Risk Agent | `B-ai-agents/risk/` | Risk assessment |

---

## 5. API Endpoints (130+)

### Health & Metrics
`GET /api/v1/health`, `GET /ready`, `GET /api/v1/metrics`, `GET /metrics`

### Orders (9)
`POST /api/v1/order`, `POST /api/v1/order/iceberg`, `POST /api/v1/order/stop-loss`, `POST /api/v1/order/twap`, `GET /api/v1/orders`, `GET /api/v1/trades`, `GET /api/v1/order/{id}`, `DELETE /api/v1/order/{id}`, `GET /api/v1/stop-losses/{pair}`, `GET /api/v1/twap-orders`

### Market Data (4)
`GET /api/v1/market/trades/{pair}`, `GET /api/v1/orderbook/{pair}`, `GET /api/v1/orderbook/{pair}/depth`, `GET /api/v1/ticker/{pair}`

### DOT Settlement (2)
`POST /api/v1/dot/transfer`, `GET /api/v1/dot/status/{id}`

### TEE (2)
`GET /api/v1/tee/status`, `POST /api/v1/tee/rotate`

### FIX (2)
`GET /api/v1/fix/status`, `GET /api/v1/fix/sessions`

### Sovereign (5)
`GET /api/v1/sovereign/status`, `POST /api/v1/sovereign/shield`, `POST /api/v1/sovereign/register-identity`, `GET /api/v1/sovereign/identity/{tenant_id}`, `POST /api/v1/sovereign/decrypt`, `GET /api/v1/sovereign/generate-keypair`

### Consensus / WAL / CRDT / WASM
`GET /api/v1/consensus/stats`, `GET /api/v1/wal/status`, `GET /api/v1/crdt/status`, `GET /api/v1/wasm/status`

### Cloud (7)
`GET /cloud/status`, `GET/POST /cloud/tenants`, `GET/DELETE /cloud/tenants/{id}`, `POST /cloud/tenants/{id}/upgrade`, `POST/GET /cloud/tenants/{id}/apikeys`, `GET /cloud/tenants/{id}/invoices`, `GET /cloud/billing/summary`, `GET /cloud/scaling`

### Compliance (2)
`POST /compliance/onboard`, `GET /compliance/status/{id}`

### Matching & Batch (3)
`POST /api/v1/matching/mode`, `GET /api/v1/batch/status/{pair}`, `POST /api/v1/batch/execute/{pair}`

### Auth (6)
`POST /api/v1/auth/register`, `POST /api/v1/auth/login`, `POST /api/v1/auth/refresh`, `POST /api/v1/auth/verify`, `GET /api/v1/auth/audit`, `POST /api/v1/auth/kyc`, `POST /api/v1/auth/select-tier`

### Counterparty (3)
`POST /api/v1/counterparty/add`, `GET /api/v1/counterparty/list/{tenant_id}`, `GET /api/v1/counterparty/check/{a_id}/{b_id}`

### ISO20022 (2)
`GET /api/v1/iso20022/reports`, `GET /api/v1/iso20022/reports/{filename}`

### Ghost Integration (9)
`GET/POST /api/v1/ghost/tax/rate`, `GET /api/v1/ghost/treasury`, `GET /api/v1/ghost/prohibited`, `POST/DELETE /api/v1/ghost/prohibited/{addr}`, `GET /api/v1/ghost/sleeper`, `POST/DELETE /api/v1/ghost/sleeper/{addr}`, `POST /api/v1/ghost/sleeper/{addr}/freeze`, `POST /api/v1/ghost/sleeper/{addr}/seize`, `POST /api/v1/ghost/sleeper/{addr}/tax/{amount}`, `GET /api/v1/ghost/stats`

### Universal Bridge (5)
`GET/POST /api/v1/bridge/projects`, `DELETE /api/v1/bridge/projects/{name}`, `POST /api/v1/bridge/projects/{name}/forward`, `GET /api/v1/bridge/stats`, `POST /api/v1/bridge/receive`

### LLM / AI (5)
`POST /api/v1/llm/chat`, `GET /api/v1/llm/status`, `POST /api/v1/ai/chat`, `GET/POST /api/v1/ai/config`, `GET /api/v1/ai/status`

### Backup (2)
`POST /api/v1/backup/trigger`, `GET /api/v1/backup/status`

### Fortress (7)
`POST /api/v1/fortress/heartbeat`, `GET /api/v1/fortress/status`, `GET /api/v1/fortress/audit`, `GET/POST /api/v1/fortress/succession`, `POST /api/v1/fortress/succession/disable`, `GET /api/v1/fortress/treasury/balance`, `POST /api/v1/fortress/treasury/withdraw`

### Circuit Breaker (5)
`GET /api/v1/circuit/status`, `GET/POST /api/v1/circuit/config/{pair}`, `GET /api/v1/circuit/events`, `POST /api/v1/circuit/reset/{pair}`, `POST /api/v1/circuit/trigger/{pair}/{level}`

### Wallet (3)
`GET /api/v1/wallet/balance`, `POST /api/v1/wallet/deposit`, `POST /api/v1/wallet/withdraw`

### Webhooks (3)
`GET/POST /api/v1/webhooks`, `DELETE /api/v1/webhooks/{id}`

### Shariah (3)
`GET /api/v1/shariah/audit`, `GET /api/v1/shariah/status`, `POST /api/v1/shariah/prohibit`

### WebSocket (2)
`WS /ws/orders`, `WS /ws/market/{pair}`

### Docs (3)
`GET /docs`, `GET /api/v1/docs`, `GET /api/v1/openapi.json`

### Other
`POST /webhook/payment`, `GET /trade`, `GET /dashboard`, `GET /api/v1/kill-switch-demo`

---

## 6. Architectural Decisions (9)

### D-001: Lock-Free Pipeline (push_mutex → ArrayQueue)

**Context:** Original design used `parking_lot::Mutex` around pipeline push. At 1.5M TPS, this Mutex would become a contention hotspot.

**Alternatives:**
1. `Mutex<Vec<TradePayload>>` — Simple but blocks matching engine. Rejected.
2. `tokio::mpsc::Sender` — Async channel but matching runs on main thread. Rejected.
3. `crossbeam::ArrayQueue` + Disruptor — Chosen. Zero-wait, no allocation on push.

**Trade-offs:** Fixed capacity (overflow drops trades) vs zero contention in hot path.

**Performance:** `ArrayQueue::push()` = single `fetch_add` + `copy_nonoverlapping` — no syscall, no park/unpark.

---

### D-002: Runtime Segregation (Berlin Wall)

**Context:** Entire system ran under `#[tokio::main]`. Async scheduling jitter affected matching latency.

**Solution:** `fn main()` + dedicated matching thread + `tokio::runtime` for async. Full CPU isolation.

```
Core 0 = Data Plane (matching engine, WAL, ArrayQueue push)
Core 1 = Pipeline Drain (ArrayQueue pop → Disruptor)
Cores 2+ = Pipeline Workers (batch processing)
Any Core = Control Plane (axum, WebSocket, billing, KYC, FIX)
```

---

### D-003: Stealth Orders

Institutional traders need full invisibility. `order.stealth: bool` + filter in `get_depth()`. No competitor offers full stealth.

---

### D-004: Hard Floor Orders

`hard_floor: Option<f64>` checked per individual fill, not per order. If maker's price violates taker's floor, matching loop breaks immediately.

---

### D-005: Batch Auction Mode (CDA + FBA Switch)

`MatchingMode` enum with `Continuous` / `BatchAuction { window_ns }`. Backward compatible, tier-configurable.

---

### D-006: Counterparty Visibility (Bloom Filters)

4KB Bloom filter with 3 Blake2b hashes, ~0.1% false positive at 100 entries. One-way privacy: check membership without enumeration.

---

### D-007: Sovereign Encrypted Identity (ECIES)

- Key Exchange: X25519 ECDH
- Key Derivation: HKDF-SHA256 (salt: `"THE-BRIDGE-SOVEREIGN-2026"`, info: `"the-bridge-sovereign-identity"`)
- Encryption: AES-256-GCM with 96-bit random nonce
- Platform cannot decrypt. Only regulator can.

---

### D-008: ISO 20022 Pipeline Integration

Generates `camt.054.001.09` XML per trade batch. Traditional banks require ISO 20022 for settlement.

---

### D-009: Anti-Sniping Jitter & SUCP

- `rdtsc()` CPU cycle counter + `splitmix64()` for deterministic-but-unpredictable jitter
- `find_sucp()` — two-pointer walk of cumulative buy/sell curves for single uniform clearing price
- Total overhead: <100ns per window

---

## 7. Tier Architecture (4 Layers)

| Layer | Name | KPIs |
|-------|------|------|
| Layer 3 | Sovereign | Unlimited orders, 10K+ connections, Reserved + Burst TPS, ECIES encrypted |
| Layer 2 | Institutional | Unlimited orders, 10K connections, <35µs P99, Core-pinned |
| Layer 1 | Verified | 10M orders/month, 50 connections, <50µs P99 |
| Layer 0 | Public | 100K orders/month, 2 connections, <100µs P99 |

### Commercial Pricing

| Tier | Price | Features |
|------|-------|----------|
| Free | $0 | Layer 0 access |
| Pro | $99/mo | Layer 1 access |
| Enterprise | Custom | Layer 2 access |
| Sovereign | Custom | Layer 3 + Compliance |

---

## 8. CPU Core Layout

```
┌─────────────────────────────────────────────────────────┐
│                    CPU Core Layout                       │
├───────┬───────────────┬─────────────────────────────────┤
│ Core  │ Role           │ Responsibilities               │
├───────┼───────────────┼─────────────────────────────────┤
│   0   │ Data Plane     │ Matching engine, WAL,          │
│       │ (main thread)  │ ArrayQueue push                │
├───────┼───────────────┼─────────────────────────────────┤
│   1   │ Pipeline Drain │ ArrayQueue pop → Disruptor     │
├───────┼───────────────┼─────────────────────────────────┤
│  2+   │ Pipeline Workers│ Batch processing → Sequential │
├───────┼───────────────┼─────────────────────────────────┤
│  Any  │ Control Plane  │ axum, WebSocket, billing, KYC  │
│       │ (async_rt)     │ FIX, compliance                │
└───────┴───────────────┴─────────────────────────────────┘
```

### Technical Debt Register

| # | Location | Description | Impact | Target Date |
|---|----------|-------------|--------|-------------|
| T-001 | `pipeline.rs` | `private_handler` stub | ISO reports not saved | Q3 2026 |
| T-002 | `consensus.rs` | gossip async stub | Consensus not tested | Q3 2026 |
| T-003 | `tee.rs` | `SgxDcapEnclave` error | SGX unavailable | Q4 2026 |
| T-004 | `wasm_engine.rs` | WASM behind `--features` | Needs recompile | Q4 2026 |
| T-005 | `iso20022.rs` | XML not persisted | Reports in logs only | Q3 2026 |
| T-006 | `matching.rs` | Batch auction jitter | FBA timing predictable | ✅ DONE |
| T-007 | `main.rs` | Memory-only expansion | No real expansion | Q4 2026 |

---

## 9. Order Placement Critical Path

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

---

## 10. How to Modify

### 10.1 Adding a new API route
1. Add handler in `handlers.rs`
2. Register in Router chain
3. Add `AppState` fields if needed
4. Add tests

### 10.2 Adding a new order type
1. Add variant to `OrderType` enum in `types.rs`
2. Add matching logic in `matching.rs`
3. Add serialization support
4. Update DOT validation in `dot.rs`

### 10.3 Adding a new FIX message type
1. Add msg_type constant in `fix.rs`
2. Add parsing in `parse_fix_message()`
3. Add response handler in session loop
4. Add seqnum tracking

### 10.4 Changing the matching algorithm
- Currently O(log p) with BTreeMap + VecDeque
- Modify `matching.rs` `execute_market()` function
- Price key format: `(price * PRICE_MULTIPLIER).round() as i64`

### 10.5 Adding a new database/backend
- Currently: in-memory with WAL persistence
- Extend `wal.rs` with new `WALRecord` variants
- To add SQL: place after WAL append, before consensus submit

---

## Build Standard

Must always pass:
```bash
cargo build --release
cargo check --tests
cargo test --release
```
with 0 errors before any submission.

## Architect Profile

- **Role:** Sovereign FinTech Architect — vision holder, makes all strategic decisions
- **Workflow:** Proposals → honest technical assessment → decides whether to implement
- **Language:** Arabic for strategy/concepts, English for code
- **Expectation:** Direct unfiltered technical opinion, no fluff, no diplomacy
- **Decision rule:** Architect decides after hearing pros/cons — agents execute, don't gatekeep
- **Key principle:** "نطور ونرتقع مش نعمل حاجه ترجعنا" — every change must advance, never regress

## P1/P2/P3 Assessments

| Proposal | Assessment |
|----------|------------|
| P1 — SGX/SEV Enclave Layer | Heavy latency tax (~10-15%+), impractical for 35µs P99. Keep current TEEEnclave. SGX/SEV = future optional hardware tier only. |
| P2 — Anti-Frontrunning Batch Auctions | ✅ Approved. Implement as `BatchAuction` mode alongside `Continuous`. Config-switchable. Destroys colocation advantage. |
| P3 — ZK Clearing Bridges (T+0 via ZK-Proofs) | Computationally infeasible at 1.5M TPS today. Long-term roadmap only. Current WAL + CRDT + DAG is the practical path. |
