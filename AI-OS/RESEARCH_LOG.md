# RESEARCH_LOG — سجل الأبحاث {{PROJECT_NAME}}

> **إلزامي — جزء من الدستور التشغيلي (مرجع: RESEARCH_PROTOCOL.md).**
> كل بحث عميق L3 يُسجَّل هنا — هذا **البرهان** على أن البحث الفعلي حدث، وقابل للتحقق من المستخدم.
> **الترتيب:** الأحدث في الأعلى.

---

## 2026-08-03 — تفعيل Flash Loan Engine (من on-demand إلى running loop)

**المستوى:** L3 — تحليل بنية التشغيل وتكامل المحركات

**السؤال:** لماذا Flash Loan Engine كان idle (running=0) على الرغم من أن الـ FlashLoanAPI مربوط في main_new.rs؟

**التحليل:**
- `FlashLoanAPI` (src/flash_loan_api.rs) ينشئ `FlashLoanArbitrageEngine` في `new()` لكنه لم يستدعِ `.run()`.
- `FlashLoanArbitrageEngine` (arbitrage/src/flash_loan_arb.rs:713) يمتلك `.run()` method لكنه لم يُستدعَ.
- بينما MEV Protection API (src/mev_api.rs:78) يحتوي على `.start()` method صريح يستدعي `self.engine.run().await`، وتم ربطه بـ `tokio::spawn` مع 44s delay.
- الفجوة: FlashLoanAPI لم يحتوي على `.start()` method، ولم يتم استدعاؤه في main_new.rs.

**الحل:**
1. إضافة `.start()` method لـ `FlashLoanAPI` يستدعي `self.engine.run().await`.
2. إضافة `tokio::spawn` في main_new.rs مع 46s delay (توحيد نمط التأخير مع بقية المحركات: MEV=44s، Direction Supervisor=42s).
3. إعادة بناء وإعادة تشغيل الـ server.

**النتيجة:**
- `the_bridge_flash_loan_running 1` (كانت 0).
- `the_bridge_flash_loan_total_scans 20+` (كانت 0).
- `the_bridge_flash_loan_pool_count 1` (MockFlash provider).
- 3 providers نشطة: AaveV3، UniswapV3، MockFlash.

**المصدر:** تحقق live عبر curl /metrics على port 3001.

---

## صيغة الإدخال (نسخ ولا اختراع)

```markdown
## YYYY-MM-DD — [اسم القرار/الموضوع]

**المستوى:** L3 — بحث عميق كامل
**لماذا:** [متى/أين يُطبق هذا القرار في المشروع]

### المصادر (المفحوصة فعليًا)
- [المصدر 1 — المؤسسة] — [رابط/مرجع] — [التاريخ]
- [المصدر 2] — [رابط/مرجع] — [التاريخ]

### أحدث ما توصل إليه العالم
- [نقطة 1: ما الجديد؟]
- [نقطة 2: كيف يُعمل الآن؟]

### مقارنة البدائل
- [البديل أ] — [لماذا/لماذا لا]
- [البديل ب] — [لماذا/لماذا لا]

### الخلاصة (القرار)
- **[القرار]** — [سبب محدد]
- الفجوات: [ما لم يُعثر عليه مصدر حاسم له — صراحة]
```

---

## 📒 السجل

## 2026-08-03 — اختيار ترخيص النسخة المفتوحة (AGPL v3 + ترخيص تجاري)

**المستوى:** L3 — بحث عميق كامل
**لماذا:** تحويل THE-BRIDGE إلى Open Source (AGPL v3) مع ترخيص مزدوج — قرار قانوني/ترخيصي (RESEARCH_PROTOCOL §1.2 أمني + §1.3 نمو/ربح).

### المصادر (المفحوصة فعليًا)
- OSSAlt — "Open Core vs Source Available Business Models 2026" — ossalt.com — 2026-03-29
- OSSAlt — "OSS Licensing: MIT vs Apache vs AGPL 2026" — ossalt.com — 2026-03-29
- Grafana Labs — "Q&A with CEO about licensing changes" / grafana.com/licensing — 2021
- MinIO — "From Open Source to Free and Open Source — AGPLv3" — min.io — 2021
- FOSSA — "AGPL 3.0 vs Apache 2.0 compatibility" — fossa.com
- Promise Legal — "Open Source Licensing for Startups" — promise.legal — 2026-06-15

### أحدث ما توصل إليه العالم
- الترخيص المزدوج **AGPL v3 + تجاري** = النموذج المثبت لمشاريع SaaS/البنية التحتية (Grafana, MinIO, Mattermost, Bitwarden, Nextcloud).
- AGPL معتمد من OSI (على عكس SSPL/BSL = ليست open source حقيقية) → يحفظ شرعية "المفتوح".
- AGPL يغلق ثغرة SaaS (استخدام عبر الشبكة يفرض إتاحة المصدر) → يحمي الربح من استغلال شركات الكبرى.

### مقارنة البدائل
- **MIT/Apache** — أقصى انتشار لكنها تسمح للشركات الكبرى (AWS) بالاستغلال دون مساهمة (فشل نموذجي على نطاق واسع).
- **BSL/SSPL (source-available)** — تحمي لكنها **ليست open source** (OSI) وتُحدث انشقاقات مجتمعية (OpenTofu, Valkey, OpenSearch).
- **AGPL + تجاري** — يوازن بين المجتمع والربح؛ القرار الأفضل لبنية تحتية موجّهة للشركات. ← **الأفضل.**

### الخلاصة (القرار)
- **AGPL v3 + ترخيص تجاري** — لأن العميل الأساسي شركات ستدفع لتجنب copyleft، لأنه يحفظ الشرعية مع منع استغلال SaaS، ولأنه النموذج المثبت (Grafana/MinIO).
- الفجوات: النص القانوني لا يُعدّ بديلاً عن استشارة محامٍ؛ قرار "الترخيص التجاري 100%" والحدود الدقيقة يبقى للمستخدم.

### قواعد إلزامية
1. **كل بحث L3 = إدخال هنا.** لا استثناء — حتى القرارات الصغيرة.
2. **المصادر بتاريخ** — أي مصدر بلا تاريخ = غير مكتمل.
3. **لا تحذف إدخالًا قديمًا** — السجل تاريخي؛ الأقدم يبقى كمرجع.
4. **الصدق في الفجوات** — لو لم تجد مصدرًا حاسمًا، اكتبه (الدستور §2.1).
5. **قبل "تم" لأي مهمة** — تحقق أن بحثها مسجّل هنا (Audit Gate).
