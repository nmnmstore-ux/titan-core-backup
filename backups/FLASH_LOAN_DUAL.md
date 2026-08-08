# ⚠️ Flash Loan — نظام ذو نسختين (V1 + V2) — اقرأ قبل أي تعديل/حذف

**لا تحذف أو تعدّل ما يلي إلا بعد فهم هذا الملف.** العملية الجارية هي "سباق مزدوج": نسختين تنفيذيتين معًا لنرى أيّهما يحقق ربحًا فعليًا.

---

## 1. حزمتا الـ Crate (مستقلّتان تمامًا)

| الاسم | المجلد | البنية/التعريف | الاستيراد (use) |
|-------|--------|----------------|-----------------|
| **V1 (Router)** | `flash-loan/` | `FlashLoanRouter` + `AaveV3Provider::new(pool,data)` | `use the_bridge_flash_loan::{...}` |
| **V2 (Manager)** | `flash-loan-v2/` | `FlashLoanManager` + `execute_auto/get_fee/supports_asset` + `FlashLoanProviderTrait` | `use the_bridge_flash_loan_v2::{...}` |

- V1 عائلة قديمة تُدار عبر crate `the-bridge-flash-loan` (package `flash-loan`).
- V2 عائلة Manager-based أُضيفت كـ crate مستقل `the-bridge-flash-loan-v2` (package `flash-loan-v2`).
- **لا تمزج import بينهما** — توقيع الـ trait مختلف كليًا:
  - V1: `supported_tokens()/check_liquidity(addr,amount)`
  - V2: `supports_asset(&str)/get_fee_basis_points(asset)`

## 2. محرّكا التنفيذ في `arbitrage/`

| الملف | المحرك |
|-------|--------|
| `arbitrage/src/flash_loan_arb.rs` | محرك **V1** (يعمل حاليًا) |
| `arbitrage/src/flash_loan_arb_v2.rs` | محرك **V2** (نسخة موازية) |

- `arbitrage/src/lib.rs` يُصرّح الوحدتين: `pub mod flash_loan_arb;` و `pub mod flash_loan_arb_v2;`
- **`arbitrage/Cargo.toml` يضم كلا التبعيتين**:
  - `the-bridge-flash-loan = { path = "../flash-loan" }`
  - `the-bridge-flash-loan-v2 = { path = "../flash-loan-v2" }`

## 3. الوصل إلى الـ API (في `src/main_new.rs`)

| البث | الـ route | الـ metric |
|------|-----------|------------|
| V1 | `/api/v1/flash-loan/status` ... | `the_bridge_flash_loan_*` |
| V2 | `/api/v1/flash-loan-v2/status` ... | `the_bridge_flash_loan_v2_*` |

- `mod flash_loan_api` + `mod flash_loan_api_v2` (الملف الجديد `src/flash_loan_api_v2.rs`).
- كلٍّ spawn مستقل بتأخير 46s (V1) و 48s (V2).

## 4. ⚠️ إزالة "الوهم" (لا تعِدها)

أزلنا المصادر الكاذبة في كلا المحرّكين:
- حُذف `MockProvider` من V1 (كان ينتج "أرباح" مزيفة).
- عداد الرصيد يبدأ `ProfitTracker::new(0.0)` (لا 0.5 افتراضي) في الاثنين.
- **لا تعِد إضافة `MockProvider` ولا `ProfitTracker::new(0.5)`** — فذلك يعيد الوهم.

## 5. نسخ احتياطي

`backups/multi-arb-<timestamp>/` يحفظ كل التعديلات (main_new.rs, lib.rs, Cargo.toml...).
لا تحذف آخر نسخة قبل التأكد من تشغيل binary الجديد.

---

**خلاصة لأي مبرمج جديد: إذا رأيت `flash-loan` و `flash-loan-v2` لا تمزجهما ولا تحذفهما. هذان متعمدان ومتوازيان. الحذف قد يمسح محرك تنفيذ مستقلًا عن قصد.**
