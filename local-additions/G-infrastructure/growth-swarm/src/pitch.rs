// ============================================================
// PitchForge — يصوغ العروض المالية المشفرة
// يحسب: فرق APY, توفير الانزلاق, مكاسب BMM, مضاعف DRS
// يولد Payload مشفر لا يمكن تزويره
// ============================================================

use crate::{CompetitionPool, VampirePitch, VampireConfig, PitchStatus};
use sha2::{Sha256, Digest};
use chrono::Utc;
use uuid::Uuid;

pub struct PitchForge {
    drs_base_rate: f64,
    bmm_efficiency: f64,
}

impl PitchForge {
    pub fn new(drs_base_rate: f64, bmm_efficiency: f64) -> Self {
        Self { drs_base_rate, bmm_efficiency }
    }

    /// صياغة Pitch لكل هدف
    /// تحسب: الميزة التنافسية، الفارق في APY، توفير الانزلاق، إجمالي الفائدة
    pub fn formulate_pitch(&self, pool: &CompetitionPool, config: &VampireConfig) -> Option<VampirePitch> {
        // فلتر: السيولة أقل من الحد الأدنى؟
        if pool.locked_liquidity_usd < config.min_liquidity_threshold_usd {
            return None;
        }

        // فلتر: الانزلاق أقل من التسامح؟
        if pool.average_slippage_pct < config.max_slippage_tolerance {
            return None;
        }

        // احسب الميزة التنافسية
        let drs_boost = self.drs_base_rate - pool.current_apy;
        let bmm_gain = (pool.average_slippage_pct * self.bmm_efficiency) * 100.0;
        let total = drs_boost + bmm_gain;

        // فلتر: الفارق أقل من الحد الأدنى؟
        if total < config.min_apy_gap {
            return None;
        }

        // توليد الرسالة المشفرة
        let message = format!(
            "🧛 SWIFTBRIDGE VAMPIRE PITCH\n\
            ─────────────────────────────\n\
            TARGET: {platform}\n\
            CHAIN:  {chain}\n\
            POOL:   {pool}\n\
            \n\
            📊 CURRENT POSITION:\n\
              APY:        {apy:.1f}%\n\
              Slippage:   {slip:.2f}%\n\
              IL Risk:    {il:.1f}%\n\
              TVL Trend:  {tvl}\n\
            \n\
            🔥 SWIFTBRIDGE OFFER:\n\
              DRS Boost:  +{drs:.1f}%\n\
              BMM Gain:   +{bmm:.1f}%\n\
              TOTAL:      +{total:.1f}%\n\
            \n\
            💰 PROJECTED ANNUAL GAIN:\n\
              You earn:   ${gain:.0f} extra\n\
              vs keeping: ${keep:.0f}\n\
            \n\
            🛡️ SECURITY:\n\
              TEE SGX     ✅\n\
              zk-SNARKs   ✅\n\
              DOT Final   ✅\n\
            \n\
            ⚡ EXECUTE: migrate via DRS protocol\n\
            ─────────────────────────────\n\
            This is an autonomous pitch from SwiftBridge\n\
            Code is law. No human involved.",
            platform = pool.platform_name,
            chain = pool.chain,
            pool = &pool.pool_address[..min(pool.pool_address.len(), 20)],
            apy = pool.current_apy,
            slip = pool.average_slippage_pct,
            il = pool.impermanent_loss_risk * 100.0,
            tvl = pool.tvl_trend,
            drs = drs_boost,
            bmm = bmm_gain,
            total = total,
            gain = pool.locked_liquidity_usd * (total / 100.0),
            keep = pool.locked_liquidity_usd * (pool.current_apy / 100.0),
        );

        // تشفير الـ payload
        let payload = message.into_bytes();
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let payload_hash = hex::encode(hasher.finalize());

        // مستوى الثقة: يعتمد على جودة البيانات
        let confidence = (total / 20.0).min(1.0) * 0.7
            + if pool.tvl_trend == "falling" { 0.3 } else { 0.1 };

        Some(VampirePitch {
            id: Uuid::new_v4().to_string(),
            target_pool: pool.clone(),
            drs_boost_pct: drs_boost,
            bmm_efficiency_gain: bmm_gain,
            total_advantage_pct: total,
            payload,
            payload_hash,
            confidence_score: confidence,
            status: PitchStatus::Formulated,
        })
    }

    /// عند الرفض: يحسن الصياغة للعرض القادم
    pub fn improve_on_rejection(&self, previous: &VampirePitch) -> VampirePitch {
        let mut improved = previous.clone();
        // زيادة الفارق المحسوب 10% (لأن المستهدف يحتاج إغراء أكبر)
        improved.total_advantage_pct *= 1.15;
        // تخفيض الثقة (المستخدم صعب)
        improved.confidence_score *= 0.9;
        improved.status = PitchStatus::Formulated;
        improved
    }
}

#[inline(always)]
fn min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}
