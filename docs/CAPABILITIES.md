# CAPABILITIES — جرد الإمكانيات الفعلية المش موثّقة
## آخر تحديث: 08 أغسطس 2026 (جلسة إصلاح 08-08)

> الغرض: وثيقة مرجعية مشكّلة بواقع التشغيل الفعلي على السيرفر (titan-core)،
> وليست خطة — تُسجّل ما هو **موجود وشغال فعلًا** وما تبقّى مفتوح/معطّل.
> تمت كتابتها بعد فحص مباشر للكود والخدمات يوم 08-08-2026.

---

## 1) الوحدات النشطة والمتحقَّق منها

| الوحدة | الحالة | ملاحظات مرجعية |
|---|---|---|
| **api-server** (الباينري) | شغال — актив، يستمع 3001/4001/4002 | الباينري = `target/release/api-server` (من `src/main_new.rs`)، خدمة `the-bridge.service` |
| **Flash Loan Arb V1** | شغال — scans تعمل | يراقب 5 pools على الإيثيرهيوم، يكتب Status كل دقيقة في الجورنال |
| **Flash Loan Arb V2** | شغال — scans تعمل | نسخة manager (V1/V2 سباق مزدوج وفق FLASH_LOAN_DUAL.md) |
| **MEV V2** (mempool/sandwich) | شغال — scans عالية | `mempool=0` بسبب غياب مصدر mempool حي |
| **Cross-Venue arb** | شغال — pairs=2 | فرق أسعار بين منصتين |
| **SUPER-ARB** (جامع الاستراتيجيات) | شغال | يجمّع: flash / cv / mev / jit / staking / valuation |
| **rpc-proxy.service** | نشط (exited) | مدمج في matching-engine |
| **matching-engine** | شغال — منفصل عن api-server | المنافذ: 8080؛ anvil محلي على 8545 |

الـ systemd units المعروفة: `the-bridge` (api server), `the-bridge-engine`, `rpc-proxy`.

---

## 2) إعدادات المحرك (FlashLoanArbConfig — الافتراضي القابل للتغيير)

| الحقل | القيمة بعد ضبط 08-08 | قبل |
|---|---|---|
| `scan_interval_ms` | **1000** | 2000 |
| `min_profit_usd` | **5.0** | 50.0 |
| `min_profit_bps` | **20** | 15 |
| `max_gas_price_gwei` | 100 | – |
| `max_position_size_eth` | 100 | – |
| `max_concurrent_trades` | 3 | – |
| `slippage_tolerance_bps` | (إفتراضي) | – |
| `enabled_chains` / `enabled_dexes` | يحدّد الـ pools | – |

ملاحظة: القيم منثبتة في `FlashLoanArbConfig::default()` (لا تُقرأ من env) — أي إعادة ضبط = تعديل الكود + build (≈13–20 دقيقة).

---

## 3) المرونة في الـ RPC (جديد — تم تفعيله 08-08)

- **المشكلة السابقة:** `ETH_RPC_URL` = Alchemy، وقد بلغ حد الشهري → **HTTP 429** على كل طلب → ولادة الفرص صفر.
- **الحل المفعّل:** كل طلب `eth_call` في محركات الفلاش يمر الآن عبر
  `rpc_candidates(primary)` → يجرب القائمة بالترتيب:
  1. الأساسي `ETH_RPC_URL`
  2. الاحتياطات `ETH_RPC_FALLBACK` (محفوظة في `.env`)
  3. آخر قطع `publicnode`.
  عند 429/خطأ شبكة → يرتد تلقائيًا للـ next. (تم التحقق: Alchemy=429، publicnode=200)
- **التعديل:** `rpc_candidates` + `rpc_post_json` داخل `flash_loan_arb.rs` و `flash_loan_arb_v2.rs` (استبدال الـ `.post` المباشر).
- **`.env` الحالي:** `ETH_RPC_FALLBACK=https://ethereum-rpc.publicnode.com,https://1rpc.io/eth,https://eth.llamarpc.com`
- ⚠️ `1rpc.io` بيحدد الاستخدام أيضًا (−32001)؛ الـ publicnode هو الموثوق حاليًا.

---

## 4) فجوة التنفيذ (المعوّق للربح الفعلي)

- المحرك يكتشف و يرتب الفرص (`opportunity_queue`)، **لكن التنفيذ الفعلي للصفقة يتطلب**:
  - `flash_loan_contract` — عقد Deployment
  - `arbitrage_contract` — عقد تنفيذ
  - `wallet_address` + `private_key` — محفظة ممولة (من `.env`؟)
- حاليًا هذه القيم = `None` في `FlashLoanArbConfig::default()` → **الوضع الحالي: كشف فقط، مفيش تنفيذ حي** ما لم يوقّف مفعّل في الطبقة التشغيلية (لا توجد حاليًا).
- **خطوة الربح الفعلية التالية** = توصيل عقد + محفظة + مفاتيح في الإعدادات (أو جهاز التتنبيه اللي بيرسم من `.env`).

---

## 5) اعتمادِ الحمايات/الصيانة (الجديدة)

| الآلية | أين | ماذا |
|---|---|---|
| **cleanup.sh (حارس القرص)** | `deploy/cleanup.sh` | ينضّف «زبالة بناء» المشروع فقط + `/tmp` + جورنال + صور Docker مفكوكة؛ لا يمس السورس/.env/الـ binary النشط |
| **cron الجهاز** | crontab بتاع المستخدم، كل 15 د | وضع `--auto`: ينفضّ تلقائيًا لو الفاضي < 6GB، ويقف أثناء بناء جاري |
| **journald cap** | `/etc/systemd/journald.conf` → `SystemMaxUse=500M` | يحدّ نمو `/var/log` |
| **health.sh** | cron كل 5 د | فحص صحة (كان موجود) |

---

## 6) سجل الخلاصة لهذه الجلسة (08-08)

1. إصلاح **bug العنوان** `format!("{:?}", token)` → `hex::encode` في V1 و V2 (إلزام الجامع).
2. استرجاع **V2 من V1 السليم** (توازن أقواس: `depth=0`)؛ النسخة الخارجية محفوظة (`flash_loan_arb_v2.rs.modified-0822.bak`).
3. إعادة بناء `api-server` و restart (لو أنه 19 دقيقة).
4. تفعيل **RPC fallback** في الكود + تحديث `ETH_RPC_FALLBACK` في `.env`.
5. **توسيع قرص GCP** partition 50G→100G + resize2fs (87% → 44%) — مساحة تانية ~54G حرّة.
6. **hecleanup** للحماية (بند 5 أعلاه).
7. **ضبط الحساسية** (بند 2): عتبات أدنى + مسح أسرع.

---

## 7) أمور لم تُوثق بعد / تُسجّل هنا مؤقتًا

- **وحدات القياس في الفرصة**: `expected_profit_usd` و `net_profit_usd` تُحسب بوحدات "bps" (مبسّطة) وليست دولارًا حقيقيًا — تبسيط متعمد حاليًا، يحتاج تهيئة مستقبلية.
- **Creش الآمن للـ pools**: الممسوح يعتمد على `slot0()/liquidity()` (UniswapV3). أي فينيو غير-و3 لا يُكتشف حاليًا → فرص Cross-venue تحتاج أسعار حقيقية من مصادر أخرى.
- **مصدر mempool** للملّي: متوقف (لا WS حي) → المعطّلات الـ MEV `analyzed=0`.
- **config-manager** موجود تحت `A-core-infrastructure/config-manager` — لم يُدمج بعد في سلسلة ضبط الـ api-server.

---

*للإضافة/التعديل: عدّل هذا الملف بإضافة بند جديد فوق وتاريخ التحديث. لا يتم الكتابة فوق محتوى EXECUTION_AGENDA.md (الملكية الرئيسية للـ AI الآخر).*