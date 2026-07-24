# SOVEREIGN FEATURE REGISTRY — Tiered Value Proposition

**Version:** 1.0.0  
**Last Updated:** 2026-06-26  
**Status:** LIVING DOCUMENT  

---

## I. TIER ARCHITECTURE

```
Layer 3 ─ Sovereign ── Encrypted Identity, Regulator-Only Decryption
Layer 2 ─ Institutional ── Counterparty Visibility, NDA-Controlled Matching
Layer 1 ─ Verified ── LEI + KYC, Legal Entity Onboarding
Layer 0 ─ Public ── Anonymous Trading, No Identity Required
```

Each layer is a superset of the layers below it. A Sovereign user gets all features of Public + Verified + Institutional + Sovereign.

---

## II. FEATURE-TO-TIER MAPPING

| Feature | Layer 0 Public | Layer 1 Verified | Layer 2 Institutional | Layer 3 Sovereign | Status |
|---------|:--------------:|:----------------:|:--------------------:|:-----------------:|:------:|
| Place limit/market orders | ✅ | ✅ | ✅ | ✅ | DONE |
| Market depth (aggregated) | ✅ | ✅ | ✅ | ✅ | DONE |
| FIX 5.0 SP2 connectivity | ❌ | ❌ | ✅ | ✅ | DONE |
| LEI verification | ❌ | ✅ | ✅ | ✅ | DONE |
| KYC/sanctions screening | ❌ | ✅ | ✅ | ✅ | DONE |
| TEE-attested trades | ❌ | ✅ | ✅ | ✅ | DONE |
| DOT settlement | ❌ | ✅ | ✅ | ✅ | DONE |
| Stealth orders (depth-hidden) | ❌ | ✅ | ✅ | ✅ | DONE |
| Hard floor orders | ❌ | ✅ | ✅ | ✅ | DONE |
| WASM hooks (custom logic) | ❌ | ❌ | ✅ (Pro) | ✅ (Ent) | DONE |
| Dedicated engine cores | ❌ | ❌ | ❌ | ✅ (Ent) | DONE |
| Batch auction mode | ❌ | ❌ | ✅ | ✅ | DONE |
| **Counterparty visibility** | ❌ | ❌ | ✅ | ✅ | **DONE** |
| **Encrypted identity** | ❌ | ❌ | ❌ | ✅ | **DONE** |
| **ISO 20022 reports** | ❌ | ❌ | ✅ | ✅ | **DONE** |
| Sovereign kill switch | ❌ | ❌ | ❌ | ✅ | DONE |
| Hot migration | ❌ | ❌ | ❌ | ✅ | DONE |
| ZK-proof settlement | ❌ | ❌ | ❌ | ✅ (Roadmap) | PENDING |
| SGX/SEV hardware attestation | ❌ | ❌ | ❌ | ✅ (Roadmap) | PENDING |

---

## III. TECHNICAL KPIs PER TIER

### Layer 0 — Public
| KPI | Target | Bound |
|-----|--------|-------|
| Max orders/month | 100,000 | Hard limit |
| Max concurrent connections | 2 | Per tenant |
| TPS share | Pooled | Best effort |
| P99 latency | <100µs | Shared resources |
| Market depth visibility | Full | No stealth |
| Identity storage | None | Anonymous UUID |

### Layer 1 — Verified
| KPI | Target | Bound |
|-----|--------|-------|
| Max orders/month | 10,000,000 | Hard limit |
| Max concurrent connections | 50 | Per tenant |
| TPS share | Pooled | Prioritized |
| P99 latency | <50µs | Shared resources |
| Compliance | LEI + KYC | On-chain attestation |
| Identity storage | Plaintext | Encrypted at rest |

### Layer 2 — Institutional
| KPI | Target | Bound |
|-----|--------|-------|
| Max orders/month | Unlimited | Soft limit |
| Max concurrent connections | 10,000 | Per tenant |
| TPS share | Reserved | Dedicated cores |
| P99 latency | <35µs | Core-pinned pipeline |
| Counterparty visibility | Bloom filter | Mutual acceptance |
| ISO 20022 | camt.054 | Per-trade XML |
| WASM hooks | Custom logic | Per-tenant |

### Layer 3 — Sovereign
| KPI | Target | Bound |
|-----|--------|-------|
| Max orders/month | Unlimited | No limit |
| Max concurrent connections | 10,000+ | Per tenant |
| TPS share | Reserved + Burst | Dedicated cores |
| P99 latency | <35µs | Core 0 data plane |
| Identity storage | ECIES encrypted | Regulator only |
| Kill switch | Armed | Hot migration ready |
| ZK proofs | (Planned) | bellman/arkworks |

---

## IV. COMPETITIVE MOAT

| Exclusive Feature | THE-BRIDGE | Binance | Coinbase | ICE/NYSE |
|------------------|:----------:|:-------:|:--------:|:--------:|
| Stealth orders | ✅ | ❌ (Iceberg only) | ❌ | ❌ |
| Hard floor orders | ✅ | ❌ | ❌ | ❌ |
| Batch auction (anti-sniping) | ✅ | ❌ | ❌ | ❌ |
| Counterparty visibility | ✅ | ❌ | ❌ | ❌ |
| Encrypted sovereign identity | ✅ | ❌ | ❌ | ❌ |
| WASM hooks | ✅ | ❌ | ❌ | ❌ |
| Sovereign kill switch | ✅ | ❌ | ❌ | ❌ |
| NUMA-aware thread pool | ✅ | ❌ | ❌ | ❌ |
| Progressive disclosure KYC | ✅ | ❌ | ❌ | ❌ |

---

## V. COMMERCIAL TIERS

| Tier | Monthly Price (Planned) | Target Customer |
|------|------------------------|-----------------|
| Free | $0 | Retail traders, testing |
| Pro | $99/mo | Active traders, small funds |
| Enterprise | Custom | Banks, hedge funds, exchanges |
| Sovereign | Custom + Compliance | Central banks, SWF, regulators |

---

*See `MASTER_ROADMAP_EN.md` for implementation progress of each feature.*
