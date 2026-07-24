# DECISION INTELLIGENCE LOG — THE-BRIDGE

**Version:** 1.0.0  
**Last Updated:** 2026-06-26  
**Status:** LIVING DOCUMENT  

---

## I. DECISION REGISTER

### D-001: Lock-Free Pipeline (push_mutex → ArrayQueue)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Performance / Latency |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/pipeline.rs`, `src/main.rs`, `src/matching.rs` |

**Context:**
The original design used a `parking_lot::Mutex` around the pipeline push from the matching engine. At 1.5M TPS target, this `Mutex` would become a contention hotspot — every trade requires a pipeline push, and mutual exclusion on the hot path adds non-deterministic latency spikes.

**Alternatives Considered:**
1. **`parking_lot::Mutex<Vec<TradePayload>>`** — Simple, but blocks the matching engine on contention. Rejected.
2. **`tokio::mpsc::Sender`** — Async channel, but the matching engine runs on the main thread (non-async). Rejected.
3. **`crossbeam::ArrayQueue` (lock-free MPSC) + Disruptor (SP)** — Chosen. Zero-wait in the common case, no allocation on push, fixed capacity eliminates unbounded growth.

**Trade-offs:**
| Pro | Con |
|-----|-----|
| Zero contention in hot path (no locks) | Fixed capacity — overflow drops trades (log + metric) |
| No allocation on `push()` | Requires separate drain thread (core 1) |
| O(1) bounded MPSC | Consumers must spin-loop when empty |
| Matches LMAX Disruptor pattern | Slightly more complex than simple Mutex |

**Performance Impact:**
- Hot path: `ArrayQueue::push()` = single `fetch_add` + `copy_nonoverlapping` — no syscall, no park/unpark
- Drain thread: continuous pop → `Disruptor::push_sp()` (single producer, no CAS)
- Workers: `claim_batch()` uses `compare_exchange_weak` (lock-free CAS on atomic gate)

**Rationale:**
At 1.5M TPS, even a 100ns lock hold time translates to 15% of available CPU on the core. Lock-free eliminates this entirely. The trade-off (fixed capacity, drop risk) is acceptable because:
- `STAGE_CAPACITY = 262,144` — absorbs micro-bursts
- Pipeline workers drain at line rate
- Overflow indicates sustained over-capacity (signals auto-scaling)

---

### D-002: Runtime Segregation (Single Runtime → Data/Control Plane)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Architecture / Isolation |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/main.rs` |

**Context:**
Originally the entire system ran under `#[tokio::main]`. The async runtime handles HTTP, WebSockets, consensus, FIX gateway, and billing. The matching engine ran as `tokio::spawn` tasks. This meant:
- Async task scheduling jitter affected matching latency
- Blocking I/O (WAL writes) could stall the async runtime
- No CPU isolation between control plane (variable latency) and data plane (tight latency budget)

**Alternatives Considered:**
1. **Single `#[tokio::main]` with priority tasks** — Tokio doesn't support priority scheduling. Rejected.
2. **Two runtimes on same core** — No isolation benefit. Rejected.
3. **`fn main()` + dedicated matching thread + `tokio::runtime` for async** — Chosen. Full CPU isolation.

**Trade-offs:**
| Pro | Con |
|-----|-----|
| Matching engine immune to async jitter | Slightly more complex startup code |
| WAL writes don't block HTTP responses | Cannot use async traits in matching engine |
| CPU core pinning eliminates cache thrash | Pipeline workers need separate threads |
| Control plane can scale across multiple cores | Two-thread communication via ArrayQueue only |

**Performance Impact:**
- Matching on core 0: dedicated, no preemption
- Pipeline drain on core 1: dedicated, no preemption
- Workers on cores 2+: dedicated, no preemption
- Control plane on any core: async, multi-threaded
- Cross-plane communication: lock-free ArrayQueue (see D-001)

**Rationale:**
The "Berlin Wall" between data and control plane is non-negotiable for <35µs P99 latency. The matching engine must never wait for an HTTP handler or a database query. The main thread pattern (`loop { thread::park_timeout(10s) }`) after spawning async work ensures the data plane lives on its own core forever.

---

### D-003: Stealth Orders (Flag on Order Struct)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Competitive |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/types.rs`, `src/orderbook.rs` |

**Context:**
Institutional traders need to place large orders without revealing their full hand to the market. Traditional Iceberg orders (Binance/Coinbase) reveal a portion of the size. We wanted full invisibility until execution.

**Alternatives Considered:**
1. **Iceberg orders** — Standard in industry, but reveals part of order. Rejected.
2. **Reserve orders** — Similar to Iceberg. Rejected.
3. **Stealth flag** — Chosen. Order participates in matching normally but is filtered from `get_depth()`. Simple, effective, unique.

**Rationale:**
No competitor offers full stealth. This is an exclusive feature for the Verified+ tiers. The implementation is trivially `order.stealth: bool` + filter in depth aggregation. No matching engine changes needed.

---

### D-004: Hard Floor Orders

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Risk |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/types.rs`, `src/matching.rs` |

**Context:**
Traders wanted a guarantee that their order won't execute below (for sells) or above (for buys) a certain price, even in volatile markets. This is distinct from a limit order (which specifies the worst acceptable price for the entire order).

**Rationale:**
`hard_floor: Option<f64>` checked per individual fill, not per order. If a maker's price violates the taker's floor, the matching loop breaks immediately. If a taker's price is violated, the maker is skipped and the next maker is tried. Both sides protected.

---

### D-005: Batch Auction Mode (Continuous + FBA Switch)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Anti-Frontrunning |
| **Status** | ✅ IMPLEMENTED (Base) |
| **Milestone pending** | Anti-sniping jitter (T-006) |
| **Files affected** | `src/types.rs`, `src/matching.rs`, `src/orderbook.rs`, `src/main.rs` |

**Context:**
Continuous Double Auction (CDA) allows frontrunning, sniping, and latency arbitrage. Frequent Batch Auctions (FBA) destroy the colocation advantage by batching orders into discrete time windows and executing them via CSPRNG shuffle.

**Alternatives Considered:**
1. **CDA only** — Current system, vulnerable to frontrunning. Rejected for institutional tiers.
2. **FBA only** — Destroys latency arbitrage but adds settlement delay. Not suitable for all users.
3. **Dual mode (CDA + FBA switch)** — Chosen. Backward compatible, tier-configurable.

**Implementation:**
- `MatchingMode` enum with `Continuous` / `BatchAuction { window_ns }`
- `place_order_batch()` accumulates orders in a timed window
- `execute_batch_auction()` shuffles by price level using Blake2b-seeded CSPRNG
- Toggle via `POST /api/v1/matching/mode`

---

### D-009: Anti-Sniping Jitter & Single Uniform Clearing Price (SUCP)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Batch Auction |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/types.rs`, `src/matching.rs`, `src/orderbook.rs`, `src/main.rs` |

**Context:**
Batch auctions with fixed-duration windows create a predictable deadline — a sniper can time their order submission to arrive just before the window closes. This defeats the purpose of batch auctions (eliminating latency advantage). Additionally, the original CSPRNG shuffle within price levels was order-independent but did not find a single clearing price across price levels.

**Anti-Sniping Jitter:**
- `rdtsc()` CPU cycle counter provides sub-nanosecond timestamp input
- `splitmix64()` (single-cycle avalanche hash) mixes `window_number + (rdtsc & 0xFFFF)` into a deterministic-but-unpredictable offset
- `compute_jitter_micros(range, window_num)` → offset in `[-range, +range]` microseconds
- `actual_window_ns(ns, range, num)` → jittered deadline applied at window creation
- Zero allocations, zero locks, single-cycle mixing — fits within <35µs latency budget

**Single Uniform Clearing Price:**
- `find_sucp(buys_desc, sells_asc)` → two-pointer walk of cumulative buy/sell curves
- Returns midpoint price at crossing point (or `None` if no overlap)
- `execute_batch_auction_sucp()` replaces old shuffle-based matching — all trades at SUCP
- Price-time priority sort (best price → earliest timestamp) before SUCP calculation
- Orders that violate their limit price at SUCP are skipped (returned as remaining)

**Alternatives Considered:**

| Approach | Status | Reason |
|----------|--------|--------|
| `FxHasher` for jitter | Rejected | Too fast (2 cycles) — not enough avalanche for deadline unpredictability |
| CSPRNG (OsRng) for jitter | Rejected | Too slow (syscall per call) — violates 35µs latency budget |
| VDF (Verifiable Delay Function) | Deferred | Computationally expensive, adds ms-level latency |
| CSPRNG shuffle (original) | Replaced | No single clearing price — order execution was price-level dependent |

**Performance Impact:**
- `rdtsc()` = ~25 cycles (sub-ns)
- `splitmix64()` = ~8 cycles
- `compute_jitter_micros()` = ~50 cycles total (~20ns at 2.5GHz)
- `find_sucp()` = O(b+s) — negligible at 10K order batch size
- **Total jitter overhead per window: <100ns** — well within 35µs P99 target

**Rationale:**
The combination of rdtsc + splitmix64 provides the best trade-off: predictability from the window number (deterministic for the same inputs) + unpredictability from the CPU cycle counter (different on each window creation). The deadline is commited at window creation time, so a sniper cannot predict it before the window opens.

---

### D-006: Layer 2 — Counterparty Visibility (Bloom Filters)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Institutional |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/counterparty.rs`, `src/matching.rs`, `src/orderbook.rs`, `src/main.rs` |

**Context:**
Institutional traders require the ability to restrict who they trade with. A Central Bank (Sovereign) may only accept trades from vetted counterparties. An exchange may not want to trade with specific entities.

**Alternatives Considered:**
1. **Whitelist stored in database** — Simple but reveals full list on any API call. Rejected.
2. **Bloom filter** — Chosen. Space-efficient, privacy-preserving, O(1) membership test.
3. **Private Set Intersection (PSI)** — Cryptographically ideal but computationally expensive at 1.5M TPS. Deferred.

**Rationale:**
Bloom filters provide one-way privacy: a tenant can check if a specific counterparty is in their list, but cannot enumerate the full list. The 3 Blake2b hash functions and 4096-byte filter provide ~0.1% false positive rate at 100 entries — acceptable for institutional matching.

**Enforcement:**
- Checked in `match_order()`, `execute_market()`, and `execute_batch_auction()`
- If check fails: maker pushed to back of queue, next maker tried (not `break`)
- Mutual acceptance required: both taker and maker must accept each other

---

### D-007: Layer 3 — Sovereign Encrypted Identity (ECIES)

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Sovereign |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/sovereign.rs`, `src/main.rs` |

**Context:**
The highest tier requires that even the platform operator cannot read the entity's identity. Only a designated regulator (holding a separate private key) can decrypt the identity on demand. This is essential for Central Banks and Sovereign Wealth Funds.

**Cryptographic Design:**
- **Key Exchange**: X25519 ECDH (regulator static key + ephemeral user key)
- **Key Derivation**: HKDF-SHA256 (salt = `"THE-BRIDGE-SOVEREIGN-2026"`, info = `"the-bridge-sovereign-identity"`)
- **Encryption**: AES-256-GCM with 96-bit random nonce
- **Output**: `nonce (12 bytes) || ciphertext`

**Security Properties:**
- Platform **cannot** decrypt (does not hold regulator's private key)
- Encrypted blob is **public-key authenticated** (only regulator's key can decrypt)
- **Forward secrecy** not required (regulator's static key is long-lived)
- **Replay resistance** via random nonce per encryption

**Regulator Key Management:**
- Keypair loaded from `THE_BRIDGE_REGULATOR_SECRET` env var (hex-encoded 32 bytes)
- If not set, an ephemeral keypair is generated and logged (development only)
- The platform stores only the **public key** — never the private key
- Utility endpoint `GET /api/v1/sovereign/generate-keypair` for key generation

---

### D-008: ISO 20022 Pipeline Integration

| Field | Detail |
|-------|--------|
| **Date** | 2026-Q2 |
| **Category** | Feature / Compliance |
| **Status** | ✅ IMPLEMENTED |
| **Files affected** | `src/iso20022.rs`, `src/pipeline.rs`, `src/main.rs` |

**Context:**
Traditional banks require ISO 20022 XML messages for settlement and reconciliation. The existing `sovereign_handler` in the pipeline was a no-op. We needed real XML generation wired into the trade pipeline.

**Implementation:**
- `build_camt_054()` generates valid `camt.054.001.09` XML (Bank-to-Customer Debit/Credit Notification)
- Each trade batch produces a complete Document with GrpHdr + Ntry per trade
- Wired into `sovereign_handler` closure in `main.rs`
- `Iso20022Report` struct with msg_type, xml_content, trade_count for downstream processing

**Pending:**
- XML is currently logged via `tracing::debug!` — needs persistence to file or message queue (T-005)
- Additional message types: `seev.042` (trade confirmation), `colr.014` (collateral management)
- Full ISO 20022 mapping requires financial domain expert review

---

## II. ALTERNATIVES TRACKING

| Decision | Chosen | Rejected | Reason |
|----------|--------|----------|--------|
| Inter-thread communication | `ArrayQueue` | `tokio::mpsc`, `Mutex<Vec>` | Lock-free, non-async, bounded |
| Cross-plane isolation | `fn main()` + `async_rt` | `#[tokio::main]` | CPU isolation, no jitter |
| Order invisibility | Stealth flag | Iceberg | Full invisibility vs partial |
| Counterparty privacy | Bloom filter | Whitelist DB, PSI | Balance of privacy + speed |
| Sovereign encryption | ECIES (X25519 + AES-GCM) | Plaintext, XOR | Real asymmetric encryption |
| ISO 20022 approach | per-batch XML generation | per-trade XML | Reduce volume at 1.5M TPS |
| Jitter source | `rdtsc()` + `splitmix64` | `FxHasher`, `OsRng`, VDF | Speed vs unpredictability trade-off |
| Batch clearing price | SUCP (two-pointer cross) | CSPRNG shuffle | Fairer — unified price across all levels |

---

## III. IMPACT ANALYSIS SUMMARY

| Decision | Latency Impact | Throughput Impact | Complexity Impact |
|----------|---------------|-------------------|-------------------|
| ArrayQueue | ✅ Eliminates lock contention | ✅ Bounded, O(1) | ⚠️ Drain thread needed |
| Runtime segregation | ✅ Isolates matching from async | ✅ Control plane can scale | ⚠️ Startup complexity |
| Stealth orders | ✅ Zero (filter in depth only) | ✅ Zero | ✅ Trivial |
| Hard floor | ✅ Zero (one float compare) | ✅ Zero | ✅ Trivial |
| Batch auction | ⚠️ Adds window delay | ✅ Higher throughput | ⚠️ Configuration needed |
| Counterparty visibility | ⚠️ Bloom filter check per match | ✅ Zero | ✅ Simple |
| Sovereign encryption | ⚠️ ECIES on registration only | ✅ Zero on trade path | ⚠️ Key management |
| ISO 20022 | ✅ Off critical path (pipeline) | ✅ Batch processing | ⚠️ XML generation |
| Jitter + SUCP | ✅ <100ns overhead | ✅ O(b+s) SUCP | ✅ No new dependencies |

---

*This document is the canonical record of all architectural decisions. Update every time a significant trade-off is made.*
