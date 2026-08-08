# SYSTEM_STATE.md — The-Bridge Sovereign Constitution

> **Logical Hard-Lock**: This document is the immutable "Sovereign Constitution" for all
> AI/Human developers. It binds the Pirate/Vampire paradigm to the Sovereign Block Builder
> infrastructure. Every subsystem, entry-point, and dependency is referenced by path.

---

## 1. The Dual Soul Architecture

| Aspect | Mode A — Pirate / Vampire | Mode B — Sovereign / Imperial |
|---|---|---|
| Capital model | Zero-capital (flash loans, mempool shadowing) | Self-funded L2 block builder capturing 100% MEV |
| Entry point | `arbitrage/src/flash_loan_arb_v2.rs` (FlashLoanArbEngine) | `mev-protection/src/lib.rs` (MEVExtractionEngineV2) |
| Risk profile | Single-trade atomicity, must repay | Full block control, proposer-builder separation |
| Revenue sink | Arbitrage spread − fees | All block gas fees + MEV |
| Activation | `NETWORK=mainnet` + funded executor key | Private RPC gateway + sequencer + builder relay |

**Invariants (both modes)**:
- Multi-RPC fallback chain in `arbitrage/src/rpc_fallback.rs` keeps the engine alive
  under Alchemy 429 / Cloudflare 521 / publicnode exhaustion.
- Executor key isolation: `secrets.0` (chmod 600), never inlined in `.env`.
- Zero-trust shutdown path via circuit breaker in `flash_loan_arb_v2.rs` (`circuit_breaker_*`).

---

## 2. Seven Core Engines

### Engine 1 — Rust Matching Engine `Core::Rust::Matcha`

**Path**: [`src/pipeline.rs`](../../src/pipeline.rs), [`src/numa.rs`](../../src/numa.rs), [`local-additions/A-core-infrastructure/matching-engine/src/numa.rs`](../../local-additions/A-core-infrastructure/matching-engine/src/numa.rs)
**Spec**: 1.4 M TPS target / 45.2 µs median latency (see `EXECUTION_AGENDA.md` [1,2]).

- `DualPipeline` (`src/pipeline.rs:205`): lock-free MPSC `ArrayQueue<TradePayload>` →
  LMAX Disruptor (single-producer) → worker threads pinned to cores 2+ via
  `std::thread::Builder(...).spawn()` with CPU affinity (`taskset`).
- Cache-line false-sharing guard: `CACHELINE = 64` (`src/numa.rs:15`);
  every `NumaVec<T>` slot is aligned to `align_of::<T>().max(CACHELINE)`
  (see [`src/numa.rs:220`](../../src/numa.rs#L220)). This is the NUMA memory
  false-locality fix requested by [15,16] — slots are *not* re-packed across the
  64-byte boundary, so hot fields in `TradePayload` never share a cache line
  with a cold field across cores.

| Field | `TradePayload` (hot) | `AdaptiveBatcher` | `Sequencer` cursor |
|---|---|---|---|
| Size (est) | 56 B | 32 B | 8 B |
| Cache lines | 1 | 1 | 1 |
| Alignment | `max(align_of,64)` | `max(align_of,64)` | native |

> **Kernel co-location note**: to push below 45.2 µs, bind the Disrupton
> drain thread + worker pool to a single NUMA node (`taskset -c 2-7`) so the
> L2/L3 ring is local to both the builder and the matcher (see
> [`deploy/linux-tune.sh`](../../deploy/linux-tune.sh) for hugepages + IRQ affinity).

### Engine 2 — Sovereign L2/L3 Sequencer `Core::Sovereign::Sequencer`

**Path**: [`src/pipeline.rs`](../../src/pipeline.rs) — `Sequencer` struct (`src/pipeline.rs:178`).
**Current state**: **LIVE** (not a stub).

- `Sequencer { next_commit: AtomicU64 }` implements an LMAX-style commit cursor.
  `wait_and_advance(batch_start, batch_end)` is a busy-spin barrier guarded by
  `Ordering::Acquire/Release`; `cursor()` exposes the leading edge.
- The `DarkPoolManager` in `src/dark_pool_orchestrator.rs` is currently a **STUB**
  (`// Stub implementation`). This is the hand-off point for Mode B block
  production wiring (proposer → builder → relay). Target: plug private mempool
  from `mev-protection` into the sequencer batch path (`DualPipeline::run`).

### Engine 3 — BMM (XY=K) & DRS Liquidity Engine `Core::Liquidity::BMM`

**Path**: [`src/bmm_amm.rs`](../../src/bmm_amm.rs)
**Spec**: 3.98× liquidity retention [4,5].

- 3 pools initialised at boot: `BTC/USDT`, `ETH/USDT`, `ETH/BTC`
  (see boot log `BMM X⁴Y=K Engine started — 3 pools, fee=50bps`).
- The AMM uses a custom invariant; pools expose reserve + fee state via the
  `bmm_amm::BMMAMMPool` interface. Fee accrual is routed to the treasury
  (`api_server::dark_pool_manager` / `fortress.treasury`).
- DRS (Dynamic Reserve Swap) hooks are embedded in the swap path
  (`src/bmm_amm.rs` `swap_exact_input` / `swap_exact_output`) to rebalance
  reserves across pools on large trades.

**Stub alert**: `DarkPoolManager` (`src/dark_pool_orchestrator.rs:40`) is empty —
needs liquidity router + batch-auction wiring before Mode B captures builder revenue.

### Engine 4 — DOT Interop Engine `Core::Interop::DOT`

**Path**: [`src/snapshot.rs`](../../src/snapshot.rs), [`src/cloak.rs`](../../src/cloak.rs)
**Spec**: Near-instant cross-chain transfer [6,7].

- `DOTSnapshot` (`src/snapshot.rs:36`) serialises order-book state for XCM-style
  relay: `{ chain_id, nonce, root_hash, dot_pending: Vec<DOTSnapshot> }`.
- `src/cloak.rs:169` holds `dot_pending: Vec<DOTSnapshot>` — pending cross-chain
  packets awaiting finality proof.
- Packet lifecycle: `submit_snapshot()` → relay → `confirm_snapshot()` →
  execute on destination chain. Currently Ethereum-only; DOT bridge target
  is `seleo_dot_bridge` in `local-additions/`.

### Engine 5 — Ghost & ZK-KYC Privacy Mixers `Core::Privacy::GhostZK`

**Path**: [`src/sovereign_ghost.rs`](../../src/sovereign_ghost.rs), [`src/zk_snark.rs`](../../src/zk_snark.rs), [`src/auth.rs`](../../src/auth.rs)
**Spec**: Compliance-silent privacy [8,9].

- `SovereignGhost` (`src/sovereign_ghost.rs:117`): full onion-routing mixer.
  - `build_relay_network()` — 20 relay nodes (`guard`/`relay`/`exit` prefixes).
  - `create_circuit()`, `dissolve_circuit()`, `rotate_identity()`.
  - `GhostStatus` exposes `total_dissolved`, `bytes_relayed_total`,
    `is_emergency_mode` (circuit-breaker for compliance freeze).
- `src/zk_snark.rs`: `ZKProofSystem::{Arkworks, Bellman, Halo2, RISC0, SP1}`.
  - `ZKEngine::generate_proof()` + `verify_proof()` — real path.
  - **SP1 prover** enum variant exists; transition task is wiring `sp1-sdk`
    into `generate_proof` for real on-chain verification [17-19].
- **ZK-KYC**: `src/auth.rs` — KYC hash is mixed into the identity credential
  via `hash(kyc_root || nullifier)` so verification is zero-knowledge but
  revocable by authority. No stub here.

### Engine 6 — AI CEO Orchestrator `Core::AI::CEO`

**Path**: [`src/ai_ceo.rs`](../../src/ai_ceo.rs), [`local-additions/A-core-infrastructure/`](../../local-additions/A-core-infrastructure/), [`AI-OS/`](../../AI-OS/)
**Spec**: Local DeepSeek-R1 management [11,12].

- `AICEO` init log: `AICEO (DeepSeek-R1 CEO) initialized — N pairs, risk: f`.
- Decision loop: `ai_ceo::run_analysis_cycle()` emits
  `{ decisions, recommendations, pnl }` every cycle (see boot log
  `AICEO: analysis cycle complete`).
- DeepSeek-R1 runs **locally** under `local-additions/A-core-infrastructure/`
  (no external API tokens). The Ollama bridge is `local-additions/A-core-infrastructure/AI-OS/ollama_bridge.rs`.
- **Hand-off**: harden the Ollama/DeepSeek socket bridge + PWA/APK dynamic
  generator (`local-additions/D-user-app/`) with Vampire stealth bots [20,21].

### Engine 7 — Fiat ATM Bridge `Core::Fiat::ATM`

**Path**: [`src/main_new.rs`](../../src/main_new.rs) (search `fiat_balances`), [`local-additions/J-local-payments/`](../../local-additions/J-local-payments/)
**Spec**: Sub-60s off-ramp [10,11].

- Boot-time liquidity snapshot: `fiat_balances: { USDC: 12_500_000, USDT: 8_000_000 }`.
- Off-ramp path: liquidity → DRS rebalance → `settle_fiat()` → banking rail.
- Latency SLO: `< 60 s` end-to-end (tracked via `fiat_settle_latency` gauge).

---

## 3. Operational Logic — Dual-Mode Switch

```
                                 ┌── Mode B: Sovereign
                                 │   proposer-builder separation
                                 │   mev-protection/ (bundle → sequencer)
                          (fork point)
 ┌── Mode A: Pirate         │   zero-capital flash loans
 │   arbitrage/flash_loan_arb_v2.rs
 │   Aave V3 flashLoanSimple → swap → repay
 │   multi-RPC fallback (rpc_fallback.rs)
 └──────────────────────────┘
```

| Switch | Env / Flag | Mode A | Mode B |
|---|---|---|---|
| Capital | `EXECUTOR_BALANCE_ETH == 0` | Flash-Loan (parasite) | Self-funded block build |
| RPC | `ETH_RPC_URL` (Alchemy) | Multi-RPC fallback w/ gas-policy | Private RPC gateway |
| Block engine | N/A | N/A | `mev-protection` + `pipeline::Sequencer` |

---

## 4. Hard Dependencies (Logical Lock)

| Sub-system | Depends on | Path | Comment |
|---|---|---|---|
| Matching Engine | `numa.rs` CACHELINE=64 | `src/numa.rs:15` | must stay 64 on x86-64 |
| Sequencer | `Disruptor` cursor | `src/pipeline.rs:178` | Acquire/Release ordering |
| BMM → Treasury | `treasury.deposit` | `src/onboarding_engine.rs:1094` | unused-must-use warning tracked |
| Ghost → ZK-KYC | `auth.rs` root | `src/auth.rs` | kYC nullifier binding |
| AI CEO → ALL | `snapshot.rs` | `src/snapshot.rs` | feeds analysis |
| Fiat → BMM | `swap_exact_*` | `src/bmm_amm.rs` | DRS rebalance |
| Mode A → ZKProof | `zk_snark::verify_proof` | `src/zk_snark.rs:216` | proof verifies batch |
| Mode B → Sequencer | `wait_and_advance` | `src/pipeline.rs:186` | block commit gate |

---

## 5. Deployment & Self-Install

```
bash bare_metal_setup.sh          # zero-touch server bootstrap
systemctl restart the-bridge      # service reload (auto-restart=on-failure)
```

- Service: `/etc/systemd/system/the-bridge.service`
  - `ExecStart = target/release/api-server`
  - `EnvironmentFile = .env`
  - `User = mohamednoureldinrefaay`
- Secrets layout: `.executor_key` (600), `.env` placeholders only.
- Foundry contracts: `Z-smart-contracts/FlashLoanArbitrage.sol`.

> **Sealed**: This file is treated as read-only at runtime; changes require
> manual review + rebuild + restart. It is the canonical source-of-truth for
> cross-agent hand-off (Agents 1-6).
