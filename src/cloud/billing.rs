use crate::cloud::tenant::Tier;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TierPricing {
    pub monthly_price_cents: u64,
    pub per_order_cents: f64,
    pub order_limit: u64,
    pub max_connections: u32,
    pub wasm_hooks: bool,
    pub fix_protocol: bool,
    pub dag_consensus: bool,
}

impl TierPricing {
    pub fn for_tier(tier: &Tier) -> Self {
        match tier {
            Tier::Free => Self {
                monthly_price_cents: 0,
                per_order_cents: 0.0,
                order_limit: 100_000,
                max_connections: 2,
                wasm_hooks: false,
                fix_protocol: false,
                dag_consensus: false,
            },
            Tier::Pro => Self {
                monthly_price_cents: 29_99,
                per_order_cents: 0.001,
                order_limit: 10_000_000,
                max_connections: 50,
                wasm_hooks: false,
                fix_protocol: true,
                dag_consensus: false,
            },
            Tier::Enterprise => Self {
                monthly_price_cents: 999_99,
                per_order_cents: 0.0001,
                order_limit: u64::MAX,
                max_connections: 10_000,
                wasm_hooks: true,
                fix_protocol: true,
                dag_consensus: true,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Invoice {
    pub tenant_id: Uuid,
    pub period_start: i64,
    pub period_end: i64,
    pub total_orders: u64,
    pub monthly_fee_cents: u64,
    pub usage_fee_cents: u64,
    pub total_cents: u64,
    pub paid: bool,
}

#[derive(Debug)]
pub struct BillingMeter {
    pub orders: DashMap<Uuid, u64>,
    pub trades: DashMap<Uuid, u64>,
    pub period_start: Mutex<i64>,
    pub invoices: DashMap<Uuid, Vec<Invoice>>,
}

impl BillingMeter {
    pub fn new() -> Self {
        Self {
            orders: DashMap::new(),
            trades: DashMap::new(),
            period_start: Mutex::new(chrono::Utc::now().timestamp_millis()),
            invoices: DashMap::new(),
        }
    }

    pub fn record_order(&self, tenant_id: &Uuid) {
        *self.orders.entry(*tenant_id).or_insert(0) += 1;
    }

    pub fn record_trade(&self, tenant_id: &Uuid) {
        *self.trades.entry(*tenant_id).or_insert(0) += 1;
    }

    pub fn get_order_count(&self, tenant_id: &Uuid) -> u64 {
        self.orders.get(tenant_id).map_or(0, |v| *v)
    }

    pub fn get_trade_count(&self, tenant_id: &Uuid) -> u64 {
        self.trades.get(tenant_id).map_or(0, |v| *v)
    }

    pub fn generate_invoice(&self, tenant_id: &Uuid, tier: &Tier) -> Invoice {
        let pricing = TierPricing::for_tier(tier);
        let orders = self.get_order_count(tenant_id);
        let now = chrono::Utc::now().timestamp_millis();
        let period_start = *self.period_start.lock().unwrap();

        let usage_cents = (orders as f64 * pricing.per_order_cents * 100.0) as u64;
        let total = pricing.monthly_price_cents + usage_cents;

        let invoice = Invoice {
            tenant_id: *tenant_id,
            period_start,
            period_end: now,
            total_orders: orders,
            monthly_fee_cents: pricing.monthly_price_cents,
            usage_fee_cents: usage_cents,
            total_cents: total,
            paid: false,
        };

        let mut invoices = self.invoices.entry(*tenant_id).or_insert_with(Vec::new);
        invoices.push(invoice.clone());
        invoice
    }

    pub fn reset_period(&self) {
        self.orders.clear();
        self.trades.clear();
        if let Ok(mut ps) = self.period_start.lock() {
            *ps = chrono::Utc::now().timestamp_millis();
        }
    }

    pub fn get_invoices(&self, tenant_id: &Uuid) -> Vec<Invoice> {
        self.invoices.get(tenant_id).map_or_else(Vec::new, |v| v.clone())
    }
}

impl Default for BillingMeter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingSummary {
    pub total_orders_all_time: u64,
    pub total_invoices: usize,
    pub outstanding_cents: u64,
    pub monthly_recurring_cents: u64,
}

impl BillingMeter {
    pub fn global_summary(&self) -> BillingSummary {
        let total_orders: u64 = self.orders.iter().map(|e| *e.value()).sum();
        let total_invoices: usize = self.invoices.iter().map(|e| e.value().len()).sum();
        let outstanding: u64 = self.invoices.iter()
            .flat_map(|e| e.value().clone())
            .filter(|inv| !inv.paid)
            .map(|inv| inv.total_cents)
            .sum();
        let mrr: u64 = self.invoices.iter()
            .flat_map(|e| e.value().clone())
            .filter(|inv| !inv.paid)
            .map(|inv| inv.monthly_fee_cents)
            .sum();

        BillingSummary {
            total_orders_all_time: total_orders,
            total_invoices,
            outstanding_cents: outstanding,
            monthly_recurring_cents: mrr,
        }
    }
}
