use crate::cloud::billing::{BillingMeter, Invoice};
use crate::cloud::tenant::{Tenant, TenantManager, Tier};
use crate::cloud::apikey::ApiKeyManager;
use crate::dark_pool_orchestrator::DarkPoolManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerPlan {
    pub id: String,
    pub name: String,
    pub monthly_price_cents: u64,
    pub per_trade_bps: u64,
    pub dark_pool_access: bool,
    pub fba_access: bool,
    pub ghost_access: bool,
    pub smart_routing: bool,
    pub fix_protocol: bool,
    pub dedicated_cores: u32,
    pub max_monthly_volume: f64,
    pub max_connections: u32,
    pub api_rate_limit: u64,
    pub iso20022_export: bool,
    pub shariah_filter: bool,
    pub compliance_zk: bool,
    pub white_label: bool,
}

impl BrokerPlan {
    pub fn default_plans() -> Vec<Self> {
        vec![
            BrokerPlan {
                id: "dark_pool_lite".into(),
                name: "Dark Pool Lite".into(),
                monthly_price_cents: 999_99,
                per_trade_bps: 3,
                dark_pool_access: true,
                fba_access: false,
                ghost_access: false,
                smart_routing: false,
                fix_protocol: false,
                dedicated_cores: 0,
                max_monthly_volume: 50_000_000.0,
                max_connections: 5,
                api_rate_limit: 500,
                iso20022_export: false,
                shariah_filter: false,
                compliance_zk: false,
                white_label: false,
            },
            BrokerPlan {
                id: "dark_pool_pro".into(),
                name: "Dark Pool Pro".into(),
                monthly_price_cents: 4_999_99,
                per_trade_bps: 2,
                dark_pool_access: true,
                fba_access: true,
                ghost_access: false,
                smart_routing: true,
                fix_protocol: true,
                dedicated_cores: 2,
                max_monthly_volume: 500_000_000.0,
                max_connections: 50,
                api_rate_limit: 5000,
                iso20022_export: true,
                shariah_filter: true,
                compliance_zk: true,
                white_label: false,
            },
            BrokerPlan {
                id: "prime_enterprise".into(),
                name: "Prime Enterprise".into(),
                monthly_price_cents: 19_999_99,
                per_trade_bps: 1,
                dark_pool_access: true,
                fba_access: true,
                ghost_access: true,
                smart_routing: true,
                fix_protocol: true,
                dedicated_cores: 8,
                max_monthly_volume: f64::MAX,
                max_connections: 10_000,
                api_rate_limit: 50_000,
                iso20022_export: true,
                shariah_filter: true,
                compliance_zk: true,
                white_label: true,
            },
            BrokerPlan {
                id: "white_label_exchange".into(),
                name: "White-Label Exchange".into(),
                monthly_price_cents: 49_999_99,
                per_trade_bps: 0,
                dark_pool_access: true,
                fba_access: true,
                ghost_access: true,
                smart_routing: true,
                fix_protocol: true,
                dedicated_cores: 16,
                max_monthly_volume: f64::MAX,
                max_connections: 100_000,
                api_rate_limit: 200_000,
                iso20022_export: true,
                shariah_filter: true,
                compliance_zk: true,
                white_label: true,
            },
        ]
    }
}

pub struct PrimeBrokerage {
    plans: Vec<BrokerPlan>,
    tenants: Arc<TenantManager>,
    billing: Arc<BillingMeter>,
    api_keys: Arc<ApiKeyManager>,
    dark_pool: Arc<DarkPoolManager>,
    total_revenue_cents: AtomicU64,
    active_subscriptions: AtomicU64,
}

impl PrimeBrokerage {
    pub fn new(
        tenants: Arc<TenantManager>,
        billing: Arc<BillingMeter>,
        api_keys: Arc<ApiKeyManager>,
        dark_pool: Arc<DarkPoolManager>,
    ) -> Self {
        Self {
            plans: BrokerPlan::default_plans(),
            tenants,
            billing,
            api_keys,
            dark_pool,
            total_revenue_cents: AtomicU64::new(0),
            active_subscriptions: AtomicU64::new(0),
        }
    }

    pub fn subscribe(&self, tenant_id: &Uuid, plan_id: &str) -> Result<(Tenant, String), String> {
        let plan = self.plans.iter().find(|p| p.id == plan_id)
            .ok_or_else(|| format!("plan '{}' not found", plan_id))?;

        let tier = match plan_id {
            "white_label_exchange" => Tier::Enterprise,
            "prime_enterprise" => Tier::Enterprise,
            "dark_pool_pro" => Tier::Pro,
            _ => Tier::Pro,
        };

        self.tenants.upgrade_tenant(tenant_id, tier.clone()).map_err(|e| e.to_string())?;
        let api_key = self.api_keys.create_key(*tenant_id).map_err(|e| e.to_string())?;

        self.active_subscriptions.fetch_add(1, Ordering::Relaxed);
        self.total_revenue_cents.fetch_add(plan.monthly_price_cents, Ordering::Relaxed);

        let tenant = self.tenants.get_tenant(tenant_id)
            .ok_or("tenant not found after subscribe")?.clone();

        let msg = format!(
            "Subscribed to '{}' at ${:.2}/mo. API Key: {}. Dark Pool: {}.",
            plan.name,
            plan.monthly_price_cents as f64 / 100.0,
            api_key.1,
            if plan.dark_pool_access { "Active" } else { "Not included" }
        );

        Ok((tenant, msg))
    }

    pub fn get_plan_for_tenant(&self, tenant: &Tenant) -> Option<BrokerPlan> {
        match tenant.tier {
            Tier::Enterprise => {
                self.plans.iter().find(|p| p.id == "prime_enterprise").cloned()
            }
            Tier::Pro => {
                self.plans.iter().find(|p| p.id == "dark_pool_pro").cloned()
            }
            Tier::Free => None,
        }
    }

    pub fn check_dark_pool_access(&self, tenant_id: &Uuid) -> Result<(), String> {
        let tenant = self.tenants.get_tenant(tenant_id)
            .ok_or("tenant not found")?;
        let plan = self.get_plan_for_tenant(&tenant)
            .ok_or("no active subscription")?;
        if !plan.dark_pool_access {
            return Err("dark pool not included in your plan".into());
        }
        Ok(())
    }

    pub fn calculate_trade_fee(&self, tenant_id: &Uuid, trade_volume: f64) -> Result<u64, String> {
        let tenant = self.tenants.get_tenant(tenant_id)
            .ok_or("tenant not found")?;
        let plan = self.get_plan_for_tenant(&tenant)
            .ok_or("no active subscription")?;
        let fee_cents = (trade_volume * plan.per_trade_bps as f64 / 10000.0 * 100.0) as u64;
        Ok(fee_cents)
    }

    pub fn generate_monthly_invoice(&self, tenant_id: &Uuid) -> Result<Invoice, String> {
        let invoice = self.billing.generate_invoice(tenant_id, &Tier::Enterprise);
        Ok(invoice)
    }

    pub fn list_plans(&self) -> &[BrokerPlan] {
        &self.plans
    }

    pub fn revenue_summary(&self) -> PrimeRevenueSummary {
        let invoices = self.billing.global_summary();
        let active = self.active_subscriptions.load(Ordering::Relaxed);
        PrimeRevenueSummary {
            active_subscriptions: active,
            monthly_recurring_cents: invoices.monthly_recurring_cents,
            total_revenue_cents: self.total_revenue_cents.load(Ordering::Relaxed),
            outstanding_cents: invoices.outstanding_cents,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PrimeRevenueSummary {
    pub active_subscriptions: u64,
    pub monthly_recurring_cents: u64,
    pub total_revenue_cents: u64,
    pub outstanding_cents: u64,
}
