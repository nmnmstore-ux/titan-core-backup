use crate::types::DisclosureLevel;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tier {
    Free,
    Pro,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineStatus {
    Provisioning,
    Running,
    Draining,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstance {
    pub id: Uuid,
    pub status: EngineStatus,
    pub orders_served: u64,
    pub last_heartbeat: i64,
    pub host: String,
    pub port: u16,
    pub pinned_cores: Vec<u32>,
}

impl EngineInstance {
    pub fn new(host: String, port: u16, pinned_cores: Vec<u32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: EngineStatus::Provisioning,
            orders_served: 0,
            last_heartbeat: chrono::Utc::now().timestamp_millis(),
            host,
            port,
            pinned_cores,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetrics {
    pub total_orders: u64,
    pub total_trades: u64,
    pub total_volume_f64: f64,
    pub period_start: i64,
}

impl UsageMetrics {
    pub fn new() -> Self {
        Self {
            total_orders: 0,
            total_trades: 0,
            total_volume_f64: 0.0,
            period_start: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub tier: Tier,
    pub engine: Option<EngineInstance>,
    pub usage: UsageMetrics,
    pub created_at: i64,
    pub active: bool,
    pub api_key_prefix: String,
    pub lei: Option<String>,
    pub jurisdiction: Option<String>,
    pub disclosure_level: DisclosureLevel,
    pub dedicated_cores: Vec<u32>,
    pub balance: f64,
    pub locked_balance: f64,
}

impl Tenant {
    pub fn new(name: String, email: String, tier: Tier) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let prefix = hex::encode(&Uuid::new_v4().as_bytes()[..4]);
        Self {
            id: Uuid::new_v4(),
            name,
            email,
            tier,
            engine: None,
            usage: UsageMetrics::new(),
            created_at: now,
            active: true,
            api_key_prefix: prefix,
            lei: None,
            jurisdiction: None,
            disclosure_level: DisclosureLevel::Public,
            dedicated_cores: Vec::new(),
            balance: 10000.0,
            locked_balance: 0.0,
        }
    }

    pub fn monthly_order_limit(&self) -> u64 {
        match self.tier {
            Tier::Free => 100_000,
            Tier::Pro => 10_000_000,
            Tier::Enterprise => u64::MAX,
        }
    }

    pub fn max_connections(&self) -> u32 {
        match self.tier {
            Tier::Free => 2,
            Tier::Pro => 50,
            Tier::Enterprise => 10_000,
        }
    }
}

pub struct TenantManager {
    pub tenants: DashMap<Uuid, Tenant>,
    by_email: DashMap<String, Uuid>,
    by_prefix: DashMap<String, Uuid>,
}

impl TenantManager {
    pub fn new() -> Self {
        Self {
            tenants: DashMap::new(),
            by_email: DashMap::new(),
            by_prefix: DashMap::new(),
        }
    }

    pub fn create_tenant(&self, name: String, email: String, tier: Tier) -> Result<Tenant, String> {
        if self.by_email.contains_key(&email) {
            return Err("email already registered".into());
        }
        let tenant = Tenant::new(name, email, tier);
        let prefix = tenant.api_key_prefix.clone();
        let id = tenant.id;
        self.tenants.insert(id, tenant);
        let t = self.tenants.get(&id).unwrap();
        self.by_email.insert(t.email.clone(), t.id);
        self.by_prefix.insert(prefix, t.id);
        let cloned = Tenant::clone(&t);
        Ok(cloned)
    }

    pub fn get_tenant(&self, id: &Uuid) -> Option<dashmap::mapref::one::Ref<'_, Uuid, Tenant>> {
        self.tenants.get(id)
    }

    pub fn get_tenant_mut(&self, id: &Uuid) -> Option<dashmap::mapref::one::RefMut<'_, Uuid, Tenant>> {
        self.tenants.get_mut(id)
    }

    pub fn get_tenant_by_email(&self, email: &str) -> Option<dashmap::mapref::one::Ref<'_, Uuid, Tenant>> {
        self.by_email.get(email).and_then(|id| self.tenants.get(&id))
    }

    pub fn get_tenant_by_prefix(&self, prefix: &str) -> Option<dashmap::mapref::one::Ref<'_, Uuid, Tenant>> {
        self.by_prefix.get(prefix).and_then(|id| self.tenants.get(&id))
    }

    pub fn delete_tenant(&self, id: &Uuid) -> bool {
        if let Some((_, tenant)) = self.tenants.remove(id) {
            self.by_email.remove(&tenant.email);
            self.by_prefix.remove(&tenant.api_key_prefix);
            true
        } else {
            false
        }
    }

    pub fn list_tenants(&self) -> Vec<Tenant> {
        self.tenants.iter().map(|r| Tenant::clone(&r)).collect()
    }

    pub fn tenant_count(&self) -> usize {
        self.tenants.len()
    }

    pub fn upgrade_tenant(&self, id: &Uuid, new_tier: Tier) -> Result<Tenant, String> {
        let mut tenant = self.tenants.get_mut(id).ok_or("tenant not found")?;
        tenant.tier = new_tier.clone();
        Ok(tenant.clone())
    }

    pub fn assign_engine(&self, tenant_id: &Uuid, host: String, port: u16) -> Result<(), String> {
        let mut tenant = self.tenants.get_mut(tenant_id).ok_or("tenant not found")?;
        let cores = if tenant.tier == Tier::Enterprise {
            vec![0, 1]
        } else {
            Vec::new()
        };
        tenant.engine = Some(EngineInstance::new(host, port, cores));
        Ok(())
    }

    pub fn deposit(&self, id: &Uuid, amount: f64) -> Result<f64, String> {
        let mut tenant = self.tenants.get_mut(id).ok_or("tenant not found")?;
        tenant.balance += amount;
        Ok(tenant.balance)
    }

    pub fn withdraw(&self, id: &Uuid, amount: f64) -> Result<f64, String> {
        let mut tenant = self.tenants.get_mut(id).ok_or("tenant not found")?;
        let available = tenant.balance - tenant.locked_balance;
        if amount > available {
            return Err("insufficient available balance".into());
        }
        tenant.balance -= amount;
        Ok(tenant.balance)
    }

    pub fn lock_balance(&self, id: &Uuid, amount: f64) -> Result<(), String> {
        let mut tenant = self.tenants.get_mut(id).ok_or("tenant not found")?;
        if amount > tenant.balance - tenant.locked_balance {
            return Err("insufficient balance to lock".into());
        }
        tenant.locked_balance += amount;
        Ok(())
    }

    pub fn unlock_balance(&self, id: &Uuid, amount: f64) -> Result<(), String> {
        let mut tenant = self.tenants.get_mut(id).ok_or("tenant not found")?;
        tenant.locked_balance = (tenant.locked_balance - amount).max(0.0);
        Ok(())
    }

    pub fn settle_trade(&self, id: &Uuid, buy_amount: f64, sell_amount: f64, volume: f64) {
        if let Some(mut tenant) = self.tenants.get_mut(id) {
            tenant.locked_balance = (tenant.locked_balance - buy_amount).max(0.0);
            tenant.balance += sell_amount;
            tenant.usage.total_orders = tenant.usage.total_orders.wrapping_add(1);
            tenant.usage.total_trades = tenant.usage.total_trades.wrapping_add(1);
            tenant.usage.total_volume_f64 += volume;
        }
    }

    pub fn get_balance(&self, id: &Uuid) -> (f64, f64) {
        match self.tenants.get(id) {
            Some(t) => (t.balance, t.locked_balance),
            None => (0.0, 0.0),
        }
    }

    pub fn record_usage(&self, id: &Uuid, orders: u64, trades: u64, volume: f64) {
        if let Some(mut tenant) = self.tenants.get_mut(id) {
            tenant.usage.total_orders = tenant.usage.total_orders.wrapping_add(orders);
            tenant.usage.total_trades = tenant.usage.total_trades.wrapping_add(trades);
            tenant.usage.total_volume_f64 += volume;
            if let Some(ref mut engine) = tenant.engine {
                engine.orders_served = engine.orders_served.wrapping_add(orders);
                engine.last_heartbeat = chrono::Utc::now().timestamp_millis();
            }
        }
    }
}
