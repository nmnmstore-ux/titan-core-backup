# THE-BRIDGE — سجل المخاطر
## Risk Register — Tech + Strategic Risks

**التاريخ:** 6 أغسطس 2026
**المصدر:** MASTER_STRATEGIC_AND_EXECUTION.md §10 (إدارة المخاطر) + §19 (التدقيق والثغرات)
**منهج التقييم:** Likelihood (1-5) × Impact (1-5) = Score (2 = منخفض، 3-7 = متوسط، 8-12 = مرتفع، 15+ = حرج)

---

## 1. المقياس والمنهج

| البعد | 1 | 2 | 3 | 4 | 5 |
|-------|---|---|---|---|---|
| **الاحتمالية (L)** | نادر | غير محتمل | ممكن | محتمل جدًا | شبه مؤكد |
| **الأثر (I)** | ضئيل | محدود | معتدل | كبير | حرج/كارثي |
| **النتيجة** | ≤3 منخفضة | 4-7 متوسطة | 8-12 عالية | 13-15 حرجة | — |

---

## 2. المخاطر التقنية (من §10.1 + §19.1)

| ID | الفئة | الوصف | L | I | النتيجة | التخفيف (Mitigation) | المالك | الحالة |
|----|-------|--------|:-:|:-:|:------:|----------------------|--------|--------|
| **TR-01** | الذاكرة/الأداء | **NUMA allocation bug** — `allocate_on_node` يرجع heap عادي بدل NUMA-pinned memory → false locality يخنق الأداء (`numa.rs:94-120`) | 4 | 4 | **16 🔴** | Fix `allocate_on_node` ليرجوع NUMA-pinned memory + benchmark قياس المسار | Architect (Core) | ⏳ قيد التنفيذ |
| **TR-02** | الذاكرة/التسرب | **HugepageAllocator leak** — ~32GB/ساعة تسريب (`numa.rs:251`) → نفاد ذاكرة خلال أيام | 4 | 5 | **20 🔴** | إضافة `Drop` implementation + اختبار لإيقاف التسريب + مراقبة `/proc/meminfo` | Rust Core Engineer | ⏳ قيد التنفيذ |
| **TR-03** | تشغيل خاطئ | **CPU_SET UB** — hand-rolled CPU_SET السلوك غير محدد (`numa.rs:404-417`) | 5 | 5 | **25 🔴** | استخدام `libc::CPU_SET` موثوق بدل hand-rolling + اختبار التزام | Rust Core Engineer | ⏳ قيد التنفيذ |
| **TR-04** | عقود ذكية | **UnilateralRecovery.sol bugs (5)** — signature replay, timestamp manipulation, missing nonce → فقدان أموال المستخدم | 2 | 5 | **10 🟠** | Fix الأرجح 5 bugs (تم ✅ في الجلسة) + Foundry tests + audit خارجي قبل deploy | Smart Contract Dev | ✅ مُصلَح — ننشر الاختبار |
| **TR-05** | التكامل/الجودة | **مفيش Integration Testing** — order حقيقي من API لـ settlement غير مختبر end-to-end | 4 | 4 | **16 🔴** | إضافة test end-to-end واحد على الأقل (API → engine → DOT) | Q (Quality Agent) | ⏳ قيد التنفيذ |
| **TR-06** | النشر | **Smart Contracts non-deploy** — العقود غير منشورة على testnet | 3 | 4 | **12 🟠** | Install Foundry + Deploy على testnet + verify | DevOps | ⏳ قيد التنفيذ |
| **TR-07** | الخصوصية | **Ghost Protocol غير مربوط** — cloak.rs لا يبدأ في matching pipeline | 3 | 3 | **9 🟠** | ربط cloak.rs بمطابقة pipeline + اختبار cloaking | Ghost Lead | ⏳ قيد التنفيذ |
| **TR-08** | الخصوصية/الامتثال | **Dual Track غير مكتوب** — Compliant + Ghost معًا | 3 | 3 | **9 🟠** | كتابة المسار المزدوج (Compliant + Ghost في كيان واحد) | Ghost + Compliance | ⏳ مخطط |
| **TR-09** | الأمان | **Weak TEE attestation** — SgxDcapEnclave يعود خطأ / software-only mock | 3 | 5 | **15 🔴** | ربط SGX SDK الحقيقي (Technical Debt T-003, Q4 2026) | Security Lead | ⏳ Q4 2026 |
| **TR-10** | الشبكة | **DAG gossip stubs** — multi-node غير قابل للاختبار | 3 | 3 | **9 🟠** | تنفيذ gossip حقيقي متعدد العقد (Technical Debt T-002, Q3 2026) | Consensus Lead | ⏳ Q3 2026 |
| **TR-11** | التشغيل | **Performance gap** — 76–78K TPS مقابل هدف 1.5M+ (لماء الهدف 5%) | 4 | 4 | **16 🔴** | NUMA (TR-01) + HugePages (TR-02) + Direction Isolation + benchmark | Architect | ⏳ Phase 2-3 |

---

## 3. المخاطر الاستراتيجية (Strategic)

| ID | الفئة | الوصف | L | I | النتيجة | الأثر | المالك | الحالة |
|----|--------|--------|:--:|:--:|:--:|-----------|--------|:---:|
| SR-01 | تنظيمي | **رفض حكومات للتبنى** — بنوك مركزية تحجب / تشدد على النظام | 3 | 5 | 15 🔴 | Sovereign Tier + CBDC Ready + إثبات speed/security عبر API صغير | Business Dev | ⏳ |
| SR-02 | تنظيمي | **Friction تنظيمي فوري من Crypto / AML** — تشريعات مشددة | 3 | 4 | 12 🟠 | Progressive Disclosure KYC + Compliance Engine + zk-KYC | Compliance/Legal | ⏳ |
| SR-03 | تنظيمي | **عدم توافق الشريعة** — الدول الإسلامية الرئيسية | 2 | 3 | 6 🟠 | فلتر الشريعة الإسلامي (Shariah Compliance) | Shariah Lead | ⏳ |
| SR-04 | تنظيمي | **GDPR / خصوصية بيانات** — عقوبات | 2 | 4 | 8 🟠 | Compliance Engine + encryption + Ghost 4 Layers | Compliance | ⏳ |
| SR-05 | الاعتماد على جهات | **الاعتماد على البنوك المركزية** — تبني بطيء يوقف الطموح | 4 | 4 | 16 🔴 | موازية: دخل ذاتي من Arbitrage/Matching + تبني تدريجي | Founder + BD | ⏳ |
| SR-06 | الفريق | **Key-person risk** — الاعتماد على المؤسس / مهندس واحد هو الخبير | 3 | 5 | 15 🔴 | توثيق المعرفة + تدريب 2-3 + آليان متزامن | Founder | ⏳ |
| SR-07 | السوق | **تراجع في التبني / عدم الوصول للسيولة** — لا نمو بلا سيولة | 3 | 5 | 15 🔴 | Arbitrage Magnet + DRS 35% Rebate + 4 Disclosure | BD | ⏳ |
| SR-08 | السوق | **تقليد المنافسين** (Binance/NASDAQ يرفعون الخصوصية) | 3 | 3 | 9 🟠 | سرعة + غرافة المرافق المتناقضة (Compliance+Ghost) | Strategy | ⏳ |
| SR-09 | مالي | **نقص السيولة المال/رأس المال التشغيلي** | 3 | 4 | 12 🟠 | خطة runway (انظر RESOURCE_REQUIREMENTS) + دخل سلبي مبكر | Founder/CFO | ⏳ |
| SR-10 | مالي | **حادث أمني / اختراق** — خرق الثقة | 2 | 5 | 10 🟠 | Kill Switch + hot migration + TEE + $10M bug bounty | Security Lead | ⏳ |
| SR-11 | تشغيلي | **نقطة فشل واحدة** — انقطاع الكهربا / Chab network cut | 2 | 4 | 8 🟠 | Mesh Network (libp2p) + K8s multi-cloud 3 مناطق | DevOps | ⏳ |

---

## 4. خريطة المخاطر (Risk Heat Map)

```
   الأثر │
    5    │ TR-03 SR-01 SR-06        TR-02 TR-11 TR-05
         │ SR-05 SR-07  ★ ★ ★ ★
    4    │ TR-04                       TR-01
         │ TR-06        TR-09 SR-02   TR-05 TR-11
    3    │ SR-03 SR-09 SR-10  TR-07 TR-08
         │                TR-10 SR-04 SR-08 SR-11
    2    │
         │
         └───────────┬───────────┬────────────▶
                      احتمالية
        منخفضة    متوسطة       عالية
```

> **الأولويات الحرجة (Score ≥ 15):** TR-02 (Hugepage leak), TR-03 (CPU_SET UB), TR-11 (Performance gap), TR-01 (NUMA), TR-09 (Weak TEE), SR-01, SR-05, SR-06, SR-07.

---

## 5. سجل التحديثات

| التاريخ | التغيير |
|---------|---------|
| 2026-08-06 | إنشاء سجل المخاطر الكامل (فجوة §13.1) — 11 تقني + 11 استراتيجي |

---

*يُحدَّث عند أي تغيير في الكود أو السوق أو الفريق. المخاطر التقنية من §10.1/§19، الاستراتيجية من §10.2/10.3.*