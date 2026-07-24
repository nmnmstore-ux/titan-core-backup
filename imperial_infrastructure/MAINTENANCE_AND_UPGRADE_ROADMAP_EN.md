# MAINTENANCE & UPGRADE ROADMAP — THE-BRIDGE

**Version:** 1.0.0  
**Last Updated:** 2026-06-26  
**Status:** LIVING DOCUMENT  

---

## I. CPU CORE LAYOUT MATRIX

```
┌─────────────────────────────────────────────────────────┐
│                    CPU Core Layout                       │
├───────┬───────────────┬─────────────────────────────────┤
│ Core  │ Role          │ Responsibilities                 │
├───────┼───────────────┼─────────────────────────────────┤
│   0   │ Data Plane    │ Matching engine, WAL,            │
│       │ (Main Thread) │ pipeline push (ArrayQueue)       │
├───────┼───────────────┼─────────────────────────────────┤
│   1   │ Pipeline      │ ArrayQueue drain → Disruptor    │
│       │ Drain Thread  │ push_sp (single producer)        │
├───────┼───────────────┼─────────────────────────────────┤
│  2+   │ Pipeline      │ Disruptor claim_batch →         │
│       │ Workers       │ handlers → sequencer             │
├───────┼───────────────┼─────────────────────────────────┤
│  ANY  │ Control Plane │ axum HTTP, WebSocket,            │
│       │ (async_rt)    │ payment webhook, auth, KYC,      │
│       │               │ consensus, FIX gateway            │
└───────┴───────────────┴─────────────────────────────────┘
```

**Source files:**
- `src/main.rs:102` — `CPUAffinity::pin_to_core(0)` for data plane
- `src/pipeline.rs:234` — `CPUAffinity::pin_to_core(PIN_DRAIN_CORE=1)`
- `src/pipeline.rs:261` — `CPUAffinity::pin_to_core(PIN_WORKER_BASE + id)`
- `src/main.rs:221-226` — `tokio::runtime::Builder::new_multi_thread()` for control plane

**Verification command (Linux):**
```bash
ps -eo pid,tid,comm,psr | grep the-bridge
taskset -p <pid>
```

---

## II. ATOMIC MONITORING SCRAPE GUIDELINES

### Available Metrics Endpoints

| Endpoint | Format | Description |
|----------|--------|-------------|
| `/metrics` | Prometheus text | Full metrics for scraping |
| `/api/v1/metrics` | JSON | Structured metrics snapshot |
| `/api/v1/health` | JSON | Service health status |

### Key Metrics (All AtomicU64 / Relaxed Ordering)

| Metric | Source | Access |
|--------|--------|--------|
| `total_orders` | `OrderBookManager.total_orders` | `state.books.total_orders()` |
| `total_trades` | `OrderBookManager.total_trades` | `state.books.total_trades()` |
| `tps_current` | `OrderBookManager.tps_count` | Sampled & reset each read |
| `tps_peak` | `OrderBookManager.tps_peak` | Historical peak |
| `active_pairs` | `DashMap::len()` | O(1) lock-free |
| `total_settlements` | `DOTEngine` | Atomic counter |
| `fix_sessions` | `FIXGateway` | `RwLock` read (control plane) |
| `consensus_vertices` | `DAGConsensus` | `tokio::sync::RwLock` (control plane) |
| `wal_healthy` | `WriteAheadLog` | Boolean check |
| `sovereign_identity_count` | `SovereignIdentityStore` | `DashMap::len()` |
| `counterparty_tenant_count` | `CounterpartyVisibilityStore` | `DashMap::len()` |

### Scraping Rules
1. **Never** scrape metrics faster than once per second (TPS counter resets on read)
2. **Never** access `OrderBook` directly from monitoring — use pre-aggregated atomics
3. Prometheus `/metrics` is the canonical endpoint for production monitoring
4. All aggregators use `Ordering::Relaxed` — no memory barrier overhead on hot path

---

## III. TECHNICAL DEBT REGISTER

### Active No-Op / Placeholder Blocks

| # | Location | Description | Impact | Resolution Milestone | Target Date |
|---|----------|-------------|--------|---------------------|-------------|
| T-001 | `src/pipeline.rs:174-181` | `private_handler` is no-op (only `tracing::debug!`) | ISO 20022 private stream not persisted | Wire to file sink or message queue | Q3 2026 |
| T-002 | `src/consensus.rs` | DAG gossip `listen()` + `gossip_loop()` are async stubs | Multi-node consensus not testable | Full gossip protocol implementation | Q3 2026 |
| T-003 | `src/tee.rs` | `SgxDcapEnclave` returns runtime error (Intel SDK not linked) | SGX attestation unavailable on non-Linux | Install Intel SGX SDK + link `sgx_dcap_ql` | Q4 2026 |
| T-004 | `src/wasm_engine.rs` | WASM runtime gated behind `--features wasm` | Custom hooks require recompile | Dynamic WASM loading in production | Q4 2026 |
| T-005 | `src/iso20022.rs` | XML generated but not persisted or transmitted | Reports exist only in logs | Wire to file sink or message queue | Q3 2026 |
| T-006 | `matching.rs` | Batch auction anti-sniping jitter not implemented | FBA predictable timing | ✅ **RESOLVED** — `rdtsc()` + `splitmix64()` jitter + SUCP implemented | Q3 2026 |
| T-007 | `src/main.rs:157` | `CloudOrchestrator` provisions engines in-memory only | No actual multi-host scaling | Docker Compose / Kubernetes integration | Q4 2026 |

### Monitoring Coverage Gap

| Gap | Impact | Resolution |
|-----|--------|------------|
| No latency histogram (P50/P99/P999) | Cannot verify <35µs target | Add `Histogram` from `hdrhistogram` or `metrics` crate |
| No GC/alloc tracking | Allocation spikes invisible | Add `alloc` counters + `tikv_jemalloc_ctl` |
| No pipeline backpressure monitoring | Ring buffer pressure invisible | Add `Disruptor.pending()` to metrics |

---

## IV. UPGRADE PROCEDURES

### 1. Dependency Upgrade
```bash
cargo update            # Safe: only patch versions
cargo outdated -w       # Check for major version bumps
# After upgrade: run full test suite
cargo test --release -- --nocapture
```

### 2. Feature Addition
```bash
# 1. Update DECISION_INTELLIGENCE_LOG first (rationale)
# 2. Update MASTER_ROADMAP (add to [FUTURE BACKLOG])
# 3. Implement feature
# 4. Update MASTER_ROADMAP (move to [COMPLETED])
# 5. Update SOVEREIGN_FEATURE_REGISTRY (map to tiers)
# 6. Update MAINTENANCE_ROADMAP (add to debt register if needed)
```

### 3. New Module Addition
```bash
# 1. Create new file in src/
# 2. Add `mod new_module;` to src/main.rs
# 3. Import types with `use crate::new_module::...`
# 4. Wire into AppState if needed
# 5. Add routes if exposing endpoints
# 6. Follow existing patterns (error handling, no panics)
```

---

## V. INCIDENT RESPONSE

| Severity | Response Time | Escalation |
|----------|---------------|------------|
| Critical (data loss, halt) | <5 min | Architect + DevOps |
| High (partial degradation) | <15 min | On-call engineer |
| Medium (non-critical) | <1 hour | Next business day |
| Low (cosmetic) | <1 week | Normal queue |

### Emergency Shutdown
```bash
curl -X POST http://localhost:3001/api/v1/sovereign/shield
```

### WAL Recovery (after crash)
```bash
# WAL auto-recovers on startup
curl http://localhost:3001/api/v1/wal/status
```

---

*Last verified: 2026-06-26. Update this document when adding/modifying any component listed above.*
