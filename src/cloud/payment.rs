use crate::cloud::billing::BillingMeter;
use crate::cloud::tenant::{Tenant, TenantManager, Tier};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentWebhook {
    pub provider: PaymentProvider,
    pub event: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Stripe,
    Paddle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    pub id: String,
    pub tier: Tier,
    pub stripe_price_id: String,
    pub paddle_price_id: String,
    pub monthly_cents: u64,
}

impl SubscriptionPlan {
    pub fn default_plans() -> Vec<Self> {
        vec![
            SubscriptionPlan {
                id: "free".into(),
                tier: Tier::Free,
                stripe_price_id: "price_free".into(),
                paddle_price_id: "pri_free".into(),
                monthly_cents: 0,
            },
            SubscriptionPlan {
                id: "pro".into(),
                tier: Tier::Pro,
                stripe_price_id: "price_pro_monthly".into(),
                paddle_price_id: "pri_pro_monthly".into(),
                monthly_cents: 2999,
            },
            SubscriptionPlan {
                id: "enterprise".into(),
                tier: Tier::Enterprise,
                stripe_price_id: "price_ent_monthly".into(),
                paddle_price_id: "pri_ent_monthly".into(),
                monthly_cents: 99999,
            },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub id: String,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub amount_cents: u64,
    pub currency: String,
    pub status: String,
    pub timestamp: i64,
}

pub struct PaymentProcessor {
    plans: Vec<SubscriptionPlan>,
}

impl PaymentProcessor {
    pub fn new() -> Self {
        PaymentProcessor {
            plans: SubscriptionPlan::default_plans(),
        }
    }

    pub fn handle_webhook(
        &self,
        webhook: &PaymentWebhook,
        billing: &BillingMeter,
        tenants: &TenantManager,
    ) -> Result<PaymentEvent, String> {
        match webhook.provider {
            PaymentProvider::Stripe => self.handle_stripe(&webhook.event, &webhook.payload, billing, tenants),
            PaymentProvider::Paddle => self.handle_paddle(&webhook.event, &webhook.payload, billing, tenants),
        }
    }

    fn handle_stripe(
        &self,
        event: &str,
        payload: &serde_json::Value,
        _billing: &BillingMeter,
        tenants: &TenantManager,
    ) -> Result<PaymentEvent, String> {
        let customer_id = payload.get("customer")
            .and_then(|c| c.as_str())
            .ok_or("missing customer")?;
        let tenant = tenants.find_by_stripe_id(customer_id)
            .ok_or("tenant not found for stripe customer")?;

        match event {
            "invoice.paid" => {
                let amount = payload.get("amount_paid")
                    .and_then(|a| a.as_u64())
                    .unwrap_or(0);
                Ok(PaymentEvent {
                    id: payload.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                    tenant_id: tenant.id,
                    event_type: "invoice.paid".into(),
                    amount_cents: amount,
                    currency: payload.get("currency").and_then(|c| c.as_str()).unwrap_or("usd").into(),
                    status: "paid".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            "customer.subscription.updated" => {
                let new_tier = payload.get("items")
                    .and_then(|i| i.get("data"))
                    .and_then(|d| d.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("price"))
                    .and_then(|p| p.get("product"))
                    .and_then(|p| p.as_str())
                    .and_then(|product_id| self.tier_from_stripe_product(product_id))
                    .ok_or("could not determine new tier from subscription")?;
                tenants.upgrade_tenant(&tenant.id, new_tier).ok();
                Ok(PaymentEvent {
                    id: payload.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                    tenant_id: tenant.id,
                    event_type: "subscription.updated".into(),
                    amount_cents: 0,
                    currency: "usd".into(),
                    status: "updated".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            "customer.subscription.deleted" => {
                tenants.upgrade_tenant(&tenant.id, Tier::Free).ok();
                Ok(PaymentEvent {
                    id: payload.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                    tenant_id: tenant.id,
                    event_type: "subscription.deleted".into(),
                    amount_cents: 0,
                    currency: "usd".into(),
                    status: "cancelled".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            _ => Err(format!("unhandled stripe event: {}", event)),
        }
    }

    fn handle_paddle(
        &self,
        event: &str,
        payload: &serde_json::Value,
        _billing: &BillingMeter,
        tenants: &TenantManager,
    ) -> Result<PaymentEvent, String> {
        let customer_id = payload.get("customer")
            .and_then(|c| c.as_str())
            .ok_or("missing customer")?;
        let tenant = tenants.find_by_stripe_id(customer_id)
            .ok_or("tenant not found for paddle customer")?;

        match event {
            "transaction.completed" | "subscription.updated" => {
                Ok(PaymentEvent {
                    id: payload.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                    tenant_id: tenant.id,
                    event_type: event.into(),
                    amount_cents: payload.get("amount").and_then(|a| a.as_u64()).unwrap_or(0),
                    currency: payload.get("currency").and_then(|c| c.as_str()).unwrap_or("usd").into(),
                    status: "completed".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            "subscription.cancelled" => {
                tenants.upgrade_tenant(&tenant.id, Tier::Free).ok();
                Ok(PaymentEvent {
                    id: payload.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                    tenant_id: tenant.id,
                    event_type: "subscription.cancelled".into(),
                    amount_cents: 0,
                    currency: "usd".into(),
                    status: "cancelled".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
            _ => Err(format!("unhandled paddle event: {}", event)),
        }
    }

    fn tier_from_stripe_product(&self, product_id: &str) -> Option<Tier> {
        match product_id {
            "prod_free" => Some(Tier::Free),
            "prod_pro" => Some(Tier::Pro),
            "prod_enterprise" => Some(Tier::Enterprise),
            _ => None,
        }
    }

    pub fn plans(&self) -> &[SubscriptionPlan] {
        &self.plans
    }
}

trait TenantFinder {
    fn find_by_stripe_id(&self, stripe_id: &str) -> Option<Tenant>;
}

impl TenantFinder for TenantManager {
    fn find_by_stripe_id(&self, _stripe_id: &str) -> Option<Tenant> {
        None
    }
}
