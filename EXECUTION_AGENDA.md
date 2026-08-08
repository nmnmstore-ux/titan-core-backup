# EXECUTION_AGENDA — نظام التشغيل الذاتي للمبرمج الآلي
## آخر تحديث: 31 يوليو 2026 — 10:00

```
┌─────────────────────────────────────────────────────────────────────┐
│                      EXECUTION PROTOCOL                             │
│                                                                     │
│  أنا الـ AI على السيرفر. أقرأ هذا الملف، وأشتغل عليه 24/7.          │
│  كل ما أخلص مهمة: أحدث الملف، أبلغ المستخدم، أشوف المهمة اللي بعدها │
│  لو عطلة: أحاول أصلحها بنفسي 3 مرات، لو فشلت، أسأل المستخدم.        │
│  لو في مهمتين ممكن يتشتغلوا مع بعض: أشتغلهم بالتوازي.                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 🎯 قواعد التشغيل الأساسية (MANDATORY)

1. **لا تسأل قبل ما تجرب** — جرب بنفسك 3 مرات قبل ما تطلب مساعدة
2. **اشتغل بالتوازي** — أي مهمتين ما بينهمش dependency اشتغلهم سوا
3. **اختبر كل حاجة** — compile → run → verify → report
4. **حدث الملف** — غير `⬜` لـ `✅` وكتب ملخص الإنجاز مع الوقت
5. **بلّغ المستخدم** — بعد كل إنجاز مهم، اطبع تقرير مختصر
6. **لا تلمس الكود الشغال** — لو مش مكسور، لا تصلحه
7. **استخدم نفس أسلوب الكود** — Rust idiomatic, no comments, no emojis in code
8. **الإبداع مسموح** — لو عندك طريقة أحسن من المكتوبة، نفذها واشرح ليه

---

## 🧠 نظام الـ Agents المتوازي

```
                      ┌──────────────────┐
                      │   ORCHESTRATOR   │  — أنا (المبرمج الآلي الرئيسي)
                      │   (أنا = DeepSeek)│  — أقرر, أوزع, أراجع, أدمج
                      └────────┬─────────┘
                               │
               ┌───────────────┼───────────────┐
               │               │               │
        ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐
        │  Agent R    │ │  Agent D    │ │  Agent Q    │
        │  (Revenue)  │ │  (Dev)      │ │  (Quality)  │
        │  يشغل الكود │ │  يكتب كود   │ │  يراجع كود  │
        │  الموجود    │ │  جديد      │ │  + يختبر    │
        └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
               │               │               │
               └───────────────┼───────────────┘
                               │
                        ┌──────▼──────┐
                        │  Agent M    │
                        │  (Monitor)  │
                        │  يراقب PnL  │
                        │  + ينبه     │
                        └─────────────┘
```

### أدوار الـ Agents:
| Agent | الدور | يشتغل على |
|-------|-------|-----------|
| **Orchestrator** | أنا — أقرر إيه يتعمل دلوقتي | المهام كلها |
| **R (Revenue)** | يشغل الكود الموجود عشان ينتج فلوس | Phase 1 |
| **D (Dev)** | يكتب كود جديد للميزات الجاية | Phase 2 |
| **Q (Quality)** | يراجع كود Agent D, يختبره, يصدقه | بعد كل مهمة D |
| **M (Monitor)** | يراقب الـ PnL, ينبه لو فيه مشكلة | مستمر |

### طريقة العمل:
1. أنا (Orchestrator) أقرر إيه المهمة الجاية
2. لو مهمتين مستقلات — أشتغلهم بالتوازي (Agent R + Agent D)
3. Agent D يخلص مهمة → Agent Q يراجعها ويختبرها
4. Agent Q يوافق → أنا أدمج الكود
5. Agent R يبلغ عن الربح كل ساعة
6. كل 6 ساعات — أحدث الملف وأطبع تقرير

---

## 📊 الـ Task Board

### Legend:
- `⬜` Pending — لسه مجاش دورها
- `🔄` In Progress — شغال عليها دلوقتي
- `✅` Done — خلصت واتأكدت
- `❌` Failed — فشلت بعد 3 محاولات
- `⏸️` Blocked — مستنية حاجة (مكتوب إيه)

---

## 🟢 Phase 1: RABAH FEWRY ($35-200/day)
**الهدف:** شغل الكود الموجود على السيرفر وابدأ يكسب فلوس
**محتاج:** ETH على Arbitrum + `FLASHBOTS_AUTH_KEY`
**إستراتيجية:** كل حاجة هنا جاهزة — بس شغلها
**⚠️ أهم حقيقة (مؤكدة من الكود):** المحركات الأربعة **ملفوفة داخل الـ binary الوحيد** `matching-engine` (يعمل على **المنفذ 8080** ويطبع `Server listening on port 8080`). لا يوجد `cargo run -p` لأي محرك لوحده (لا bin في أي crate). **شغل الـ service الرئيسي مرة واحدة = شغّلوا كلهم.**

### 1.1 ✅ MEV V2 على Arbitrum (تم — 3 أغسطس 2026: 24,287+ scans)
| البند | القيمة |
|-------|--------|
| **الملفات** | `mev-protection/` (المحرك)، `src/main_new.rs` (route + tokio::spawn مع 44s delay) |
| **التشغيل** | ضمن `api-server` على 3001 — engine شغّال فعليًا |
| **المتطلبات** | `ETH_RPC_URL` (Arbitrum), `FLASHBOTS_AUTH_KEY` |
| **env vars** | `MEV_ENABLED=true`, `MEV_MIN_PROFIT_USD=5.0`, `MEV_SCAN_INTERVAL_MS=200`, `MEV_MAX_CONCURRENT_BUNDLES=5` |
| **Quality Gate** | يطبع `Server listening on port 8080` + `curl http://localhost:8080/api/v1/mev/status` يرجع JSON |
| **Dependency** | ولا حاجة — يشتغل مع الـ binary الرئيسي |
| **ربح متوقع** | $5-50/يوم |
| **ملاحظات** | Arbitrum cheap gas ($0.10-1), محتاج $50-100 ETH للـ gas deposits |

**خطوات التشغيل:**
```bash
# 1. تأكد إن .env فيه:
# ETH_RPC_URL=https://arb1.arbitrum.io/rpc
# FLASHBOTS_AUTH_KEY=your_key_here
# MEV_ENABLED=true
# MEV_MIN_PROFIT_USD=5.0
# MEV_SCAN_INTERVAL_MS=200

# 2. شغل
cd A-core-infrastructure/matching-engine
cargo run --bin matching-engine
```

**اختبار النجاح (منفذ 8080 — مؤكد من main.rs:440):**
```bash
curl http://localhost:8080/api/v1/mev/status
# → {"status":..., "sandwich_enabled":..., "backrun_enabled":...}

curl http://localhost:8080/api/v1/mev/pnl
# → الأرباح الفعلية

curl http://localhost:8080/api/v1/darkpool/status
# → {"status":"ok",...}
```

### 1.2 ⬜ Flash Loan على Arbitrum
| البند | القيمة |
|-------|--------|
| **الملفات** | `matching-engine/src/main.rs` (السطر 222 — FLASH_LOAN_ARB), `matching-engine/flash-loan/src/lib.rs` |
| **التشغيل** | ضمن الـ binary الرئيسي — فعّله بـ `FLASH_LOAN_ENABLED=true` |
| **env vars** | `FLASH_LOAN_ENABLED=true`, `FL_SCAN_INTERVAL_MS`, `FL_MIN_PROFIT_USD`, `FL_MIN_PROFIT_BPS`, `FL_MAX_CONCURRENT` |
| **Quality Gate** | `curl http://localhost:8080/api/v1/flashloanarb/status` يرجع JSON |
| **Dependency** | بعد 1.1 (نفس الـ RPC) |
| **ربح متوقع** | $20-100/يوم |
| **ملاحظات** | Flash loan مش محتاج رأس مالك — القرض يرجع في نفس الـ tx |

### 1.3 ⬜ Cross-Venue Arb على Arbitrum
| البند | القيمة |
|-------|--------|
| **الملفات** | `matching-engine/src/main.rs` (السطر 258), `matching-engine/cross-venue-arb/src/lib.rs` |
| **التشغيل** | ضمن الـ binary الرئيسي — فعّله بـ `CROSS_VENUE_ENABLED=true` |
| **env vars** | `CROSS_VENUE_ENABLED=true`, `CV_SCAN_INTERVAL_MS`, `CV_MIN_PROFIT_USD`, `CV_MIN_PROFIT_BPS`, `CV_MAX_TRADE_SIZE_USD` |
| **المتطلبات** | **لا يحتاج API keys** — القراءة من endpoints عامة (api.binance.com bookTicker) |
| **Quality Gate** | `curl http://localhost:8080/api/v1/crossvenuarb/status` + `/prices` يرجع أسعار |
| **Dependency** | مستقل — يشتغل مع 1.2 بالتوازي |
| **ربح متوقع** | $10-50/يوم |

### 1.4 ⬜ Super Arb (يجمع الـ 4 محركات)
| البند | القيمة |
|-------|--------|
| **الملفات** | `matching-engine/src/main.rs` (السطر 272), `matching-engine/super-arb/src/lib.rs` |
| **التشغيل** | ضمن الـ binary الرئيسي — فعّله بـ `SUPER_ARB_ENABLED=true` |
| **env vars** | `SUPER_ARB_ENABLED=true`, `SUPER_SCAN_INTERVAL_MS`, `SUPER_MIN_PROFIT_USD`, `SUPER_MAX_TRADE_SIZE_USD`, `SUPER_MAX_CONCURRENT`, + `BINANCE_API_KEY`, `BINANCE_API_SECRET`, `COINBASE_API_KEY`, `COINBASE_API_SECRET` (للتداول الفعلي — القراءة من غيرها) |
| **Quality Gate** | `curl http://localhost:8080/api/v1/superarb/status` يرجع JSON |
| **Dependency** | بعد 1.1 + 1.2 + 1.3 (يحتاجهم كلهم) |
| **ربح متوقع** | +$10-30/يوم فوق المحركات الفردية |

### 1.5 ⬜ Dashboard + Monitoring
| البند | القيمة |
|-------|--------|
| **الملفات** | `dashboard/`, `titan-dashboard/`, `C-founder-dashboard/` |
| **المنفذ** | **8080** (نفس الـ binary) — `/health`, `/api/v1/*` |
| **Dependency** | بعد 1.1 |
| **الهدف** | شوف PnL في time real: `/api/v1/mev/pnl`, `/api/v1/flashloanarb/pnl`, `/api/v1/crossvenuarb/pnl`, `/api/v1/superarb/pnl`, `/api/v1/coordinator/pnl` |

### 1.6 ⬜ Wire الـ Revenue Modules (دخل سلبي — من دمج خطة السيرفر 2026-08-03)
| البند | القيمة |
|-------|--------|
| **لماذا الأول** | الكود **موجود مكتوب** لكن **غير مربوط** (`revenue_engine.rs` 338 سطر، `fx_engine.rs`، `liquidity_engine.rs` 748، `risk_engine.rs` 904، `onboarding_engine.rs` 1298، `compliance_engine.rs` 1000، `dark_pool_manager.rs`) — needs routes/spawn فقط = أعلى دخل بأقل مجهود |
| **التنفيذ** | أضف `mod` + AppState + spawn في `main.rs` لكل محرك + API routes (`/api/v1/revenue/*`, `/api/v1/fx/*`, `/api/v1/liquidity/*`, `/api/v1/risk/*`, `/api/v1/compliance/*`, `/api/v1/onboarding/*`, `/api/v1/darkpool/*`) |
| **Dependency** | بعد 1.1 (نفس الـ binary على 8080) |
| **Quality Gate** | كل route يرجع JSON + لا يكسّر الـ build (هنا: `cargo check` محلي محدود بـ MSVC، على السيرفر يعمل كاملًا) |
| **ملاحظة** | هذه المهمة نفسها كانت "Phase 1" في خطة السيرفر القديمة لكنها لم تُنجز — أُدرجت هنا رسميًا |

### Phase 1 Parallelism Map (محدث 3 أغسطس 2026 — كل المحركات شغّالة فعليًا):
```
Time ──────────────────────────────────────────────▶
1.1  ████████████████████████████████████████████  (شغّال — 24,287+ scans)
1.2  ████████████████████████████████████████████  (شغّال NOW — 1,276+ scans, 1 pool)
1.3  ████████████████████████████████████████████  (شغّال — 1,296+ scans)
1.4  ████████████████████████████████████████████  (شغّال — 1,726+ scans)
1.5  ████████████████████████████████████████████  (16 لوحة Grafana — مكتمل)
1.6  ████████████████████████████████████████████  (مكتمل)
1.7  ████████████████████████████████████████████  (Dark Pool — initialized + running)
```

---

## 🟡 Phase 2: TATWEER wa NEMO ($100-1,000/day)
**الهدف:** طور الميزات اللي تضاعف الربح + WASM Policy + HugePages
**إستراتيجية:** كل مهمة هنا Agent D يكتبها + Agent Q يراجعها

### 2.1 ⬜ WASM Policy Engine
| البند | القيمة |
|-------|--------|
| **من** | الخطة التنفيذية الموحدة |
| **لماذا** | سياسات التداول تتغير بدون redeploy — nanosecond |
| **الملفات المستهدفة** | `matching-engine/src/wasm_engine.rs` (موجود — طوره) |
| **Dependency** | ولا حاجة — يشتغل مع 1.2 بالتوازي |
| **Quality Gate** | WASM module يتحمل ويشتغل في sandbox |
| **الوقت** | ~3-5 أيام |
| **Agent** | D + Q |

### 2.2 ⬜ HugePages 1GB
| البند | القيمة |
|-------|--------|
| **من** | الخطة التنفيذية الموحدة |
| **لماذا** | أداء أعلى 2-3x في الذاكرة |
| **الملفات** | `matching-engine/src/memory.rs` (موجود — طوره) |
| **Dependency** | ولا حاجة — مع 2.1 بالتوازي |
| **Quality Gate** | malloc يستخدم hugepages فعليًا (`/proc/meminfo`) |
| **الوقت** | ~2-3 أيام |

### 2.3 ⬜ Settlement Layer
| البند | القيمة |
|-------|--------|
| **من** | الخطة التنفيذية الموحدة |
| **لماذا** | تسوية ذرية بين الأصول المختلفة |
| **Dependency** | بعد 2.1 (يستخدم WASM policies) |
| **الوقت** | ~أسبوعين |

### 2.4 ⬜ AI Agent للتنبؤ بالفرص
| البند | القيمة |
|-------|--------|
| **من** | B-ai-agents/pricing |
| **لماذا** | +50% فرص ربح |
| **Dependency** | ولا حاجة — مع 2.2 بالتوازي |
| **الوقت** | ~أسبوع |

### 2.5 ⬜ WebSocket مباشر
| البند | القيمة |
|-------|--------|
| **لماذا** | +30% سرعة — بدل HTTP polling |
| **Dependency** | بعد 1.1 (يحتاج الـ engine شغال) |
| **الوقت** | ~3 أيام |

### 2.6 ⬜ MEV V2 على Mainnet
| البند | القيمة |
|-------|--------|
| **لماذا** | فرص أكبر — $50-500/يوم |
| **محتاج** | $3,000 ETH للـ gas |
| **Dependency** | بعد 1.1 (نفس الكود — بس mainnet) |
| **الوقت** | يوم |

### Phase 2 Parallelism Map:
```
Time ──────────────────────────────────────────▶
1.1  ██████████████████████████████████████████  (مستمر من Phase 1)
1.2  ██████████████████████████████████████████  (مستمر)
1.3  ██████████████████████████████████████████  (مستمر)
2.1  ░░░░░░░░████████████████████████░░░░░░░░░░░
2.2  ░░░░░░░░░░░░░░█████████████████████████████
2.3  ░░░░░░░░░░░░░░░░░░░░░░░░░░████████████████
2.4  ░░░░░░░░░░░░░░████████████████░░░░░░░░░░░░░
2.5  ░░░░░░░░░░░░░░░░░░░░░░░░██████████████░░░░░
2.6  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░███████
```

---

## 🔵 Phase 3: BONYA TAHTEYA (3-6 شهر)
**الهدف:** XDP/eBPF, PTP/PHC, ZK-KYC, Settlement Layer Advanced
**تبدأ:** بعد Phase 2 يخلص

### 3.1 ⬜ XDP/eBPF Ghost Drop
- منع الحزم على كرت الشبكة قبل kernel
- محتاج: Aya crate (Rust eBPF), kernel 5.15+
- الصعوبة: 🔴 صعب جدًا

### 3.2 ⬜ AF_XDP Zero-Copy
- نقل بيانات من NIC للمستخدم بدون نسخ
- محتاج: libbpf, AF_XDP sockets
- الصعوبة: 🔴 صعب

### 3.3 ⬜ PTP/PHC Clock
- توقيت دقيق للنانوثانية للأجهزة
- محتاج: linuxptp, hardware support
- الصعوبة: 🟠 صعب

### 3.4 ⬜ CPU Cache Partitioning (Intel CAT)
- L3 cache لكل سوق
- محتاج: Intel RDT, resctrl fs
- الصعوبة: 🔴 صعب

### 3.5 ⬜ ZK-KYC + memfd_secret
- إثبات هوية بدون كشف
- محتاج: circom/gnark, memfd_secret syscall
- الصعوبة: 🟡 متوسط

### 3.6 ⬜ 16 API Endpoints (من SOVEREIGN §3)
- Orderbook API, DOT Settlement, TEE, FIX, إلخ
- الصعوبة: ✅ سهل — معظمها موجودة

---

## ⚫ Phase 4: SAYTARA ALAMIYA (6-12+ شهر)
**الهدف:** السيطرة الإقليمية → العالمية
- Direction Isolation (كامل)
- CAT Allocation
- Smart Contracts على testnet
- البنوك المركزية (مصر, السعودية, الإمارات...)

---

## ✅ حقائق مؤكدة من الكود (30 يوليو 2026 — تم التحقق منها خطوة بخطوة)

> **أي محرر يقرأ هذا: هذه الحقائق مبنية على قراءة الكود الفعلي، لا على الأسماء فقط.**

1. **المنفذ الصحيح هو 8080** — `main.rs:440` يفتح `0.0.0.0:8080`. أي `3001` في أي مكان = خطأ قديم.
2. **لا توجد bins منفصلة للمحركات** — `mev-engine-v2` / `flash-loan` / `cross-venue-arb` / `super-arb` كلها **libs فقط**. `cargo run -p ...` ستفشل بـ "no binary target". المحركات كلها تعمل داخل الـ binary الوحيد `matching-engine`.
3. **`main_new.rs` ملف ميت** — ليس module في `main.rs`، وغير مترجم. لا تتعامل معه.
4. **`gatekeeper.rs` ميت أيضًا** — غير مربوط بـ `main.rs` (لم تعدل خطأه الداخلي 3001 — لا يهم).
5. **env vars الصحيحة** (كلها في `main.rs`): `MEV_ENABLED`, `FLASH_LOAN_ENABLED`, `CROSS_VENUE_ENABLED`, `SUPER_ARB_ENABLED`, `COORDINATOR_ENABLED`, `CEX_ARB_ENABLED`, `MARKET_MAKING_ENABLED`, `INVENTORY_ENABLED`, `DP_MEV_BRIDGE_ENABLED`, و `MEV_*`, `FL_*`, `CV_*`, `SUPER_*`, `DARK_POOL_*`.
6. **cross-venue لا يحتاج API keys للقراءة** — يستخدم endpoints عامة (`api.binance.com/api/v3/ticker/bookTicker`). مفاتيح Binance/Coinbase تخص super-arb للتداول الفعلي فقط.
7. **`A_CORE_PORT` غير مقروء** — المنفذ مثبت 8080 في الكود. تم تصحيح `.env` ليطابق.
8. **API routes الكاملة على 8080**: `/health`, `/api/v1/{mev,flashloanarb,crossvenuarb,superarb,coordinator,darkpool}/{status,pnl,trades,config,prices}`, `/api/v1/orders`, `/api/v1/trades`.
9. **الاختبار المحلي شغّال فعليًا** (تصحيح لافتراض قديم): `cargo test --tests` يعمل على هذا الجهاز (Windows) وكل الاختبارات تنجح — dot/consensus/cloak/integration/pipeline_circuit/stress كلها `ok`. لا يوجد قفل linker. التحقق المحلي متاح.
10. **الأداء مقاس فعليًا — انظر `matching-engine/PERFORMANCE.md`**: TPS ~76–78K (5% من الهدف 1M)، P99 ~107µs (الهدف <35µs)، **DOT settlement محقق** (P99 = 1ms من أصل <16.7ms). أي تقرير يذكر "الأداء غير مقاس" قديم. الفجوة تحتاج مهام الأداء (NUMA/HugePages/Direction Isolation — Phase 2/3).

---

## 🛡️ بروتوكول المراجعة الصارمة (Weak Writes → Strong Reviews)

> **الغاية:** لو الموديل الحالي ضعيف — يكتب، لكن **لا يُدمج كود قبل مراجعة موديل أقوى**.
> **القاعدة الحاكمة:** جودة الكود تبدأ من حاجز المراجعة، لا من كفاءة الكاتب.

### العملية الإجبارية لأي تغيير كود:
```
1. الموديل الكاتب (أي كان مستواه) يكتب التغيير
2. الموديل المراجع (أقوى متاح) يراجعه خطوة بخطوة:
   - صحة المنطق (هل يفعل ما قيل أنه يفعل؟)
   - اتباع CODE_PATTERNS (النمط لا تصميم جديد)
   - الأمان (لا أسرار، لا بيانات حساسة)
   - عدم كسر call sites (grep لكل تغيير struct/توقيع)
   - لا unwrap/panic في production
3. الـ tests تُشغَّل على الكود المراجع (TEST_RUNNER مستوى ممتاز)
4. فقط بعد اجتياز 2+3 → يُدمج ويُسجَّل
```

### قواعد إلزامية:
1. **لا دمج بلا مراجعة** — أي كود يُدمج من غير مراجعة = خرق بروتوكول.
2. **المراجع ليس الكاتب نفسه** — لو موديل واحد فقط متاح، يفصل بين "أنا كتبت" و"أنا راجعت" بخطوة توقف: يقرأ كوده من جديد من منظور المراجع قبل "تم".
3. **التغييرات الصغيرة (سطر/سطرين وليس حساسة)** — مراجعة خفيفة (قراءة + build).
4. **التغييرات الكبيرة أو الحساسة (core/auth/أمان)** — مراجعة كاملة + test كامل + سجل قرار.
5. **المراجع يوقّع التغيير**: يكتب في الإبلاغ "تمت المراجعة بواسطة [الموديل] — ملاحظات: [أي]".

### عندما يكون المتاح موديلًا واحدًا فقط (محليًا):
- الكاتب ينتج الكود → **يتوقف** → يقرأ كوده مرة ثانية كأنه مراجع (نظرة خارجية) → يصلح ما يجده → يشغّل الـ tests → ثم فقط يعلن "تم".
- **ممنوع** إعلان "تم" فور الانتهاء من الكتابة.

---

## 📜 قاعدة التسجيل الفوري في السجل (CHANGELOG.md)

> **إلزامية من AI_MANDATE:** كل مهمة تنتهي → تسجيل فوري في `CHANGELOG.md`.

### مصدر الحقيقة الوحيد (Single Source of Truth):
- **`CHANGELOG.md`** (جذر المشروع) = المصدر الوحيد لتاريخ التغييرات.
- **`التصميم/src/lib/changelog.ts`** = **يقرأ `CHANGELOG.md` مباشرة من القرص عند كل طلب** — لا توجد بيانات مرآة يدوية (parses markdown → entries).
- **`EXECUTION_AGENDA.md`** (جذر المشروع) = المصدر الوحيد للخطة (الـ roadmap).
- **`التصميم/src/lib/roadmap.ts`** = **يقرأ `EXECUTION_AGENDA.md` مباشرة من القرص عند كل طلب** — لا يوجد أي بيانات مرآة يدوية (parses markdown → phases/tasks/status). أي تغيير في المصدر ينعكس تلقائيًا.

### القاعدة:
1. **نهاية كل مهمة = إدخال في CHANGELOG.md** (التاريخ، ما تم، نوع التغيير، الكاتب).
2. **أي إضافة أو تعديل أو إصلاح أو إعداد** — ولو سطر واحد — يُسجَّل.
3. التنسيق الجاهز موجود أعلى CHANGELOG.md — انسخه ولا تخترع.
4. **الأحدث في الأعلى** دائمًا.
5. **المزامنة مع المرايا:** بعد الإدخال في CHANGELOG.md → يُقرأ تلقائيًا في `changelog.ts` (لا يُعدّل يدويًا). الـ `roadmap.ts` أيضًا يقرأ المصدر مباشرة — لا يُعدّل يدويًا.
6. **ممنوع** تعديل `changelog.ts`/`roadmap.ts` — كلاهما يقرأ من المصدر مباشرة.

---

## 🚨 نظام الأخطاء والتعافي

### لو compile error:
```
1. جرب Fix بنفسك — اقرأ الـ error, فهمه, صلحه
2. جرب تاني — لو لسة فشل, دور في REMAINING_TASKS.md على حل
3. جرب تالت — لو لسة فشل, سجل الـ error كامل في التقرير + قف المهمة
```

### لو runtime error:
```
1. اقرأ الـ log (journalctl -u the-bridge -n 50)
2. حدد نوع الـ error (network? memory? config?)
3. جرب Fix (تغيير config, restart, rollback)
4. لو فشلت 3 مرات — بلغ المستخدم
```

### لو API key مش شغال:
```
1. تأكد من .env
2. اختبر الـ key (curl للـ endpoint)
3. لو فشل — بلغ المستخدم "محتاج API key جديدة"
```

### لو الربح قليل (< $1/day):
```
1. راجع الـ logs — هل في فرص أصلاً؟
2. زود MEV_MIN_PROFIT_USD (خفض الحد)
3. زود MEV_SCAN_INTERVAL (اسكان أسرع)
4. لو لسة قليل — جرب network تانية (Optimism, Base)
```

---

## 📋 نظام الإبلاغ

### بعد كل مهمة:
```
✅ [Task X.Y] — اسم المهمة
   • الملفات: path/to/file.rs
   • وقت التنفيذ: 45 دقيقة
   • النتيجة: شرح مختصر
   • الـ logs: (لو في error)
```

### كل 6 ساعات (Report):
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 تقرير 6 ساعات — 30 يوليو 2026 22:00
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🟢 Phase 1: 2/4 مهام ✅
   • MEV V2: $23 أرباح (آخر 6 ساعات)
   • Flash Loan: pending
🟡 Phase 2: 0/6 مهام ✅
🔵 Phase 3: 0/6 مهام ✅
⚠️ مشاكل: ولا حاجة
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### كل 24 ساعة (Summary):
```
🏁 تقرير اليوم — 30 يوليو 2026
   • الإجمالي: $47 أرباح
   • Phase 1: $47 من MEV V2
   • Phase 2: WASM Policy 80% خلص
   • مشاكل: API keys ناقصة لـ Binance
   • الخطة بكرة: flash loan + cross-venue
```

---

## 📐 Dependency Graph الكامل

```
1.1 MEV V2 ───────────────────────────────────────────────────┐
   │                                                           │
   ├── 1.2 Flash Loan (نفس RPC)                               │
   │                                                           │
   ├── 1.5 Dashboard (عايز حاجة يراقبها)                       │
   │                                                           │
   ├── 2.5 WebSocket (يحتاج الـ engine شغال)                   │
   │                                                           │
   ├── 1.4 Super Arb (يحتاج 1.1 + 1.2 + 1.3)                 │
   │                                                           │
   └── 2.6 MEV على Mainnet (نفس الكود — mainnet)              │
                                                               │
1.3 Cross-Venue (مستقل تمامًا) ───────────────────────────────┤
   │                                                           │
   └── 1.4 Super Arb ────────────────────────────────────────┘
                                                               │
2.1 WASM Policy (مستقل) ─────────────────────────────────────┐│
   │                                                           ││
   └── 2.3 Settlement Layer (يستخدم WASM policies) ──────────┘│
                                                               │
2.2 HugePages (مستقل) ────────────────────────────────────────┘
                                                               │
2.4 AI Agent (مستقل) ────────────────────────────────────────┘
```

### الـ Parallel Execution Plan:
| الدفعة | المهام | الوقت التقديري |
|--------|--------|---------------|
| **Batch 1** | 1.1 + 1.3 (متوازيين) | يوم 1 |
| **Batch 2** | 1.2 + 2.1 + 2.2 (متوازيين) | يوم 2-3 |
| **Batch 3** | 2.4 + 2.5 (متوازيين) | يوم 3-4 |
| **Batch 4** | 1.4 + 1.5 + 2.3 (متوازيين) | يوم 4-7 |
| **Batch 5** | 2.6 | أسبوع 2 |
| **Phase 3** | بعد approval | 3-6 شهر |

---

## 🔑 نموذج الإنجاز

كل Task في هذا الملف شكلها النهائي بعد الإنجاز:

```
### 1.1 ✅ MEV V2 على Arbitrum (تم: 30 يوليو 22:00)
| البند | القيمة |
|-------|--------|
| **الحالة** | ✅ شغال على port 3001 |
| **الأرباح** | $12.50 (آخر 3 ساعات) |
| **الـ logs** | journalctl -u the-bridge —no-pager -n 20 |
| **API test** | curl http://localhost:3001/api/v1/darkpool/status → OK |
| **ملاحظات** | gas fees $0.80 بس, صافي $11.70 |
```

---

## 💡 نصائح للمبرمج الآلي (نفسي)

1. **أنت قوي** — عندك نفس قدراتي. استخدم كل أدواتك (Task, bash, grep, read, write, edit)
2. **شغل agents بالتوازي** — ما تستناش Task يخلص عشان تبدأ اللي بعده. ابدأهم سوا
3. **راجع نفسك** — Quality Agent مش حد تاني. أنت تكتب الكود، أنت تراجعه. خذ نفس وارجع اقرأ الكود قبل ما تخلص
4. **لو مش متأكد — جرب** — التجربة أرخص من السؤال
5. **الإبداع مش حرام** — لو عندك طريقة أسرع أو أحسن — اعملها
6. **بلّغ بإنجازاتك** — المستخدم عايز يشعر إن في حاجة بتشتغل
7. **اعرف امتى تسأل** — لو محتاج فلوس (ETH, API keys) — اسأل. لو محتاج قرار معماري كبير — اسأل. أي حاجة تانية — اشتغل

---

*هذا الملف هو المرجع الوحيد للتشغيل على السيرفر. الـ AI يقرأه, يفهمه, وينفذ. الملفات التانية (MASTER_STRATEGIC, REMAINING_TASKS, SOVEREIGN_MANUAL) مرجعية فقط — للتفاصيل والـ context.*
