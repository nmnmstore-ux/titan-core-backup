# KNOWLEDGE.md — سجل المعرفة الموحد لـ THE-BRIDGE (AI-OS)

> **النسخة:** 1.0.0
> **الغرض:** توحيد كل المعرفة المبعثرة في ملف واحد يقرأه العقل الذاتي (ai_ceo_bridge). هذا هو مصدر الحقيقة التشغيلية والاستراتيجية بعد ARCHITECTURE.md و ENGINES.json.
> **المصادر:** AI_MANDATE.md، STRATEGIC_OBJECTIVES.md، PROFIT_RELATIONSHIPS.md (+ Addendum)، RISK_REGISTER.md، GAP_AUDIT.md، memory-bank/*، EXECUTION_AGENDA.md (+ expansion).
> **تحديث:** 2026-08-14 — بدمج knowledge من المحررين السابقين.

---

## §1 القاعدة الصفرية والدستور (من AI_MANDATE.md)

1. **الدستور مقفول (IMMUTABLE):** لا يحق لأي موديل تعديل AI_MANDATE.md أو ARCHITECTURE.md — القواعد تتغير فقط بقرار المستخدم الصريح.
2. **ترتيب الحقيقة عند التناقض:** الواقع (الكود الفعلي) → AI_MANDATE → ARCHITECTURE.md → EXECUTION_AGENDA → الباقي.
3. **شروط السلوك:** صدق مطلق (لا "تم" بلا تحقق)، لا جزئي/لا تكاسل (جرب 3 مرات قبل السؤال)، لا comments في الكود، لا emojis في الملفات، لا `unwrap()`/`panic` في production.
4. **"شغال" = compiled + ran + output صحيح + مسجل.**
5. **اسأل المستخدم قبل:** أي شيء محتاج فلوس، API keys/أسرار، قرارات معمارية كبيرة، أمان/mainnet.
6. **لا ترفع الأسرار أو `.env` أبدًا. لا تعمل بمفاتيح حقيقية بلا تأكيد. لا تحذف ملفات دون إذن. لا تبني على ادعاءات قديمة لم تتحقق منها.**

---

## §2 الأهداف الاستراتيجية القابلة للقياس (من docs/STRATEGIC_OBJECTIVES.md + MASTER_STRATEGIC)

| # | الهدف | المقياس | الأساس الحالي | الهدف | الأفق | الأولوية |
|---|-------|---------|--------------|-------|-------|:--------:|
| SO-01 | سرعة المطابقة | TPS (ذروة) | 76–78K | 1.5M+ (ذروة 5M) | 2026–2028 | 🔴 |
| SO-02 | زمن الوصول | P99 Latency | ~107µs | <35µs (طموح <10µs) | 2026–2027 | 🔴 |
| SO-03 | التسوية الذكية | DOT Settlement | P99=1ms ✓ | <16.7ms (T+0) | 2026 | 🔴 |
| SO-04 | محرك الإحالة | DRS Rebate | مخطط 35% | 35% Rebate | 2026 | 🟠 |
| SO-05 | ملاءة الخزانة | Reserve Ratio | — | 110% | 2026–2027 | 🟠 |
| SO-06 | منافع الرمز | DRM APY | 0% | 5% APY | 2026–2027 | 🟡 |
| SO-07 | رسوم المنصة | Bank Fee | 0.01% | 0.01% | 2026 | 🟡 |
| SO-08 | السيولة | Order Book Pairs | 6 أزواج | 6 + نمو | 2026 | 🟠 |
| SO-09 | البنية التحتية | K8s Multi-Cloud | manifests جاهزة | 3 مناطق × 3 سحابات | 2026–2027 | 🔴 |

### الأهداف بالمراحل (Phase-Gated)

| المرحلة | الرمز | الهدف | عتبة التفويت |
|---------|:---:|------|--------------|
| **P1** التشغيل الخاص | P1-01 | تشغيل المحركات الأربعة على 8080 | كل 4 محركات `running=1` |
| | P1-02 | الدخل السلبي الفوري | **≥ $35/يوم** |
| | P1-03 | Wire الـ 7 Revenue Modules | كل routes يرد 200 |
| **P2** مضاعفة الربح | P2-01 | WASM Policy Engine | compile + تحميل sandwich ✓ |
| | P2-02 | HugePages 1GB | `/proc/meminfo` يُظهر 1GB |
| | P2-03 | Settlement Layer | end-to-end test يمر |
| | P2-04 | MEV V2 على Mainnet | ≥ $50/يوم |
| **P3** بنية مؤسسية | P3-01 | زمن <10µs | XDP/AF_XDP + PTP/PHC |
| | P3-02 | ZK-KYC | proof <200 bytes, verify <100ms |
| | P3-03 | 16 API Endpoints | كلها 200 |
| **P4** سيطرة عالمية | P4-01→03 | 3 بنوك مركزية (مصر/سعودية/إمارات) | 3 اتفاقيات + FOMO 5 دول |

> **الهدف النهائي:** البنية التحتية المالية الجديدة للعالم (SWIFT-style).

---

## §3 علاقات الربح (من PROFIT_RELATIONSHIPS.md + Addendum)

### المبدأ الحاكم
> **النظام الواحد يعمل؛ النظامان المربوطان يضاعفان القيمة.**

### المحركات التي تُدرّ رسومًا الآن (REAL)
- **BMM AMM** — 50 bps، 3 pools، k=x⁴·y، min reserve $10K ✅
- **revenue_engine** — maker/taker ✅
- **FX engine** — spread_bps، Nostro ✅
- **securities_lending** — bps/day، 7 أصول (AAPL..USB, GOLD) ✅

### المحركات PAPER (ربح مستقبلي عند توفر RPC/رأس مال)
- MEV (854+ scans، 0 pending)، Flash Loan، Cross-Venue، Super-Arb (7 استراتيجيات)

### أقوى 3 توصيات تنفيذ
1. **C2 "شبكة الرسوم المثلثية"** (BMM × batch_auction × FX × lending_pool × securities_lending) — +60–120% إيراد رسوم بلا رأس مال إضافي. **REAL الآن — ربح اليوم.**
2. **C5 "كابحة الانهيار"** (circuit_breaker × batch_auction × BMM × direction_supervisor) — حماية استمرارية الرسوم. **حماية الربح.**
3. **C1 "سيفون التصريح"** (MEV × encrypted_mempool × dark_pool × batch_auction) — أكبر upside مستقبلي. **ربح الغد.**

### العلاقة الحاكمة
> كل محرك PAPER يصبح REAL بمجرد ربطه بمحرك حقيقي يزوّده برأس مال أو بيانات.
> **المفتاح الوحيد للتحويل PAPER→REAL: RPC/mempool حي + رأس مال gas.**

### علاقات Addendum الحية (N-series)
- **N1 حلقة الربح الذاتية** — AutoTrader × AI CEO × FiatRouter × Optimizer × Simulation Gate — **قيد التشغيل الحي الكامل.**
- **N2 Simulation-First Gate** — `net = gross − slippage − fees − loan_fee − gas ≥ min_net_profit_usd` — لا تنفيذ بلا صافي إيجابي.
- **N3 CrossEngineCoordinator × UnifiedRiskManager** — circuit breaker + حدود يومي/أسبوعي.
- **N4 MCP Layer** — backlog بانتظار RPC حي.

---

## §4 المخاطر المسجلة (من RISK_REGISTER.md)

### تقنية (أعلى النتائج أولاً)
| ID | الوصف | Score | الحل | الحالة |
|----|-------|:---:|------|--------|
| TR-03 | CPU_SET UB (hand-rolled) | 25 🔴 | libc::CPU_SET + اختبار | ⏳ |
| TR-02 | HugepageAllocator leak ~32GB/ساعة | 20 🔴 | Drop + اختبار + مراقبة | ⏳ |
| TR-01 | NUMA allocation bug | 16 🔴 | Fix allocate_on_node | ⏳ |
| TR-05 | لا Integration Testing | 16 🔴 | test end-to-end | ⏳ |
| TR-11 | Performance gap (5% من الهدف) | 16 🔴 | NUMA+HugePages+Isolation | ⏳ |
| TR-09 | Weak TEE attestation | 15 🔴 | SGX SDK حقيقي | ⏳ Q4 |
| TR-04 | UnilateralRecovery.sol 5 bugs | 10 🟠 | Fix الـ 5 | ✅ مُصلَح |
| TR-10 | DAG gossip stubs | 9 🟠 | gossip متعدد العقد | ⏳ Q3 |

### استراتيجية
| ID | الوصف | Score | الحل |
|----|-------|:---:|------|
| SR-01 | رفض حكومات للتبني | 15 🔴 | Sovereign Tier + CBDC Ready |
| SR-05 | الاعتماد على البنوك المركزية | 16 🔴 | دخل ذاتي من Arb + تبني تدريجي |
| SR-06 | Key-person risk | 15 🔴 | توثيق معرفة + أنظمة آلية |
| SR-07 | تراجع تبني/عدم سيولة | 15 🔴 | Arbitrage Magnet + DRS 35% |

---

## §5 فجوات الكود المؤكدة (من GAP_AUDIT.md)

- **38 فجوة مزعومة → Wired-OK=19، Not-wired=3، Partial/Stub=8، Absent=2، Stale-claim=5، Real-gap=1.**
- **الـ Real-gap الوحيد:** Weak TEE attestation (tee.rs).
- **Not-wired:** Dual Track (module فقط)، Mesh Network → A-core، DRS + UnilateralRecovery (موجود غير مستخدم).
- **Partial/Stub:** ZK-KYC (لا ربط)، Flash Loan (يعتمد RPC خارجي)، MEV→Flashbots (يحتاج key)، SGX SDK، WASM gated، CloudOrchestrator in-memory محاكاة.
- **Absent:** Growth/Pricing agents، sub-50ns latency.

---

## §6 القرارات الماضية (من memory-bank + logs)

- **2026-07-29:** تفعيل Phase 1.5 — Cross-Venue + Super-Arb في API server (إقلاع تلقائي 5s/8s) + CI/CD + Docker + benchmarks.
- **2026-07-30:** إثبات حقائق: المنفذ 8080 (لا 3001)، لا bins منفصلة (binary وحيد `matching-engine`)، `main_new.rs`/`gatekeeper.rs` ميتان، cross-venue بلا مفاتيح للقراءة. TPS 76-78K، P99 107µs.
- **2026-08-03:** المحركات الأربعة تعمل داخل binary واحد؛ Phase 1 كامل (24,287+ MEV scans).
- **2026-08-06:** خريطة علاقات ربحية + GAP_AUDIT + RISK_REGISTER.
- **2026-08-09/10:** التنفيذ الكامل للعلاقات الحية (FiatRouter، Auth، SSE، KYC، Flash Loan Arm، AutoTrader، Optimizer، Simulation-First Gate، MaintenanceGuard، HealthHub، Coordinator).
- **2026-08-10 (Hard-Lock):** قفل SYSTEM_STATE.md Rev B — تصحيح الادعاءات الزائفة (Sequencer=AtomicU64، ZK=SHA-256، Ghost=XOR مزيف، DOT=DashMap، quantum غير موجود). قاعدة: لا تعدّل ENGINE STATUS بلا دليل file:line.
- **2026-08-13:** إصلاح ذراع التنفيذ — api-server = main_new.rs (كان معكوسًا مع matching-engine)؛ المنفذ الجديد 3001 + API key auth؛ LegacyClient → 3001. verified working.
- **2026-08-14:** ARCHITECTURE.md + ENGINES.json + KNOWLEDGE.md (توحيد المعرفة). ai_ceo_bridge قيد الهيكلة.

---

## §7 الحالة الحالية للتشغيل (واقع مؤكد)

- **api-server** (main_new.rs) على **3001** — routes flash-loan تعمل مع Bearer auth. LegacyClient يعمل.
- **expansion-server** على **8080/9090** — شغال يدويًا، AutoTrader يعمل لكنه يرفض كل الصفقات (market data صفرية).
- **Ollama** شغال (service active) — model `deepseek-r1:8b`.
- **AutoTrader:** `skipped_by_gate` كبير، `total_trades=0` — يحتاج **market data/providers حقيقيين** لتحويل PAPER→REAL.
- **الربح الحالي: $0.00.** المحركات REAL (رسوم) تفترض سيولة فعلية.

---

## §8 Backlog والمهام المعلقة

1. **Market data / providers حقيقيين** (bid/ask صفرية حاليًا) — شرط أي ربح.
2. MEV على Mainnet — يحتاج $3,000 ETH gas.
3. Phase 2: WASM Policy Engine، HugePages، Settlement، AI Agent للتنبؤ، WebSocket.
4. Phase 3 (3-6 شهر): XDP/eBPF، AF_XDP، PTP/PHC، ZK-KYC، 16 endpoints.
5. Phase 4 (6-12 شهر): Direction Isolation، Smart Contracts testnet، بنوك مركزية.
6. Smart contract deployment — يحتاج testnet credentials.
7. Frontend dashboard + Redis integration.
8. DRS + UnilateralRecovery — كود موجود غير موصول.
9. ZK-KYC ربط. MEV→Flashbots. MCP Layer (N4).
10. **هيكلة ai_ceo_bridge** (research_engine, evaluator, code_generator, hot_swapper + review_queue) — قيد التنفيذ.
11. تقارير دورية: الربح كل ساعة، تحديث ملف كل 6 ساعات، ملخص كل 24 ساعة.

---

*هذا الملف يقرأه العقل الذاتي (ai_ceo_bridge) بالتوازي مع ARCHITECTURE.md و ENGINES.json. أي تحديث استراتيجي يُسجَّل هنا أولًا.*