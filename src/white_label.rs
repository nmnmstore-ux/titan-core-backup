use crate::cloud::tenant::{TenantManager, Tier};
use crate::cloud::billing::BillingMeter;
use crate::cloud::apikey::ApiKeyManager;
use crate::prime_brokerage::PrimeBrokerage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteLabelConfig {
    pub brand_name: String,
    pub brand_logo_url: String,
    pub brand_primary_color: String,
    pub domain: String,
    pub custom_fix_port: Option<u16>,
    pub custom_api_port: Option<u16>,
    pub dark_pool_enabled: bool,
    pub fba_enabled: bool,
    pub ghost_enabled: bool,
    pub compliance_zk_enabled: bool,
    pub shariah_enabled: bool,
    pub iso20022_enabled: bool,
    pub dedicated_cores: u32,
    pub monthly_volume_cap: f64,
}

impl Default for WhiteLabelConfig {
    fn default() -> Self {
        Self {
            brand_name: "Exchange".into(),
            brand_logo_url: String::new(),
            brand_primary_color: "#00d4aa".into(),
            domain: "exchange.swiftbridge.io".into(),
            custom_fix_port: None,
            custom_api_port: None,
            dark_pool_enabled: true,
            fba_enabled: true,
            ghost_enabled: false,
            compliance_zk_enabled: true,
            shariah_enabled: false,
            iso20022_enabled: true,
            dedicated_cores: 4,
            monthly_volume_cap: f64::MAX,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteLabelInstance {
    pub tenant_id: Uuid,
    pub config: WhiteLabelConfig,
    pub api_endpoint: String,
    pub fix_endpoint: String,
    pub dashboard_url: String,
    pub active: bool,
    pub total_orders: u64,
    pub total_volume: f64,
    pub provisioned_at: i64,
}

pub struct WhiteLabelExchange {
    instances: Arc<dashmap::DashMap<Uuid, WhiteLabelInstance>>,
    tenants: Arc<TenantManager>,
    billing: Arc<BillingMeter>,
    api_keys: Arc<ApiKeyManager>,
    prime: Arc<PrimeBrokerage>,
    total_deployments: AtomicU64,
}

impl WhiteLabelExchange {
    pub fn new(
        tenants: Arc<TenantManager>,
        billing: Arc<BillingMeter>,
        api_keys: Arc<ApiKeyManager>,
        prime: Arc<PrimeBrokerage>,
    ) -> Self {
        Self {
            instances: Arc::new(dashmap::DashMap::new()),
            tenants,
            billing,
            api_keys,
            prime,
            total_deployments: AtomicU64::new(0),
        }
    }

    pub fn deploy(&self, tenant_id: &Uuid, config: WhiteLabelConfig) -> Result<WhiteLabelInstance, String> {
        let tenant = self.tenants.get_tenant(tenant_id)
            .ok_or("tenant not found")?;

        let plan = self.prime.list_plans().iter()
            .find(|p| p.id == "white_label_exchange")
            .ok_or("white-label plan not found")?;

        if !plan.white_label {
            return Err("white-label not included in current plan".into());
        }
        drop(tenant);

        let api_port = config.custom_api_port.unwrap_or(5000 + self.total_deployments.load(Ordering::Relaxed) as u16);
        let fix_port = config.custom_fix_port.unwrap_or(6000 + self.total_deployments.load(Ordering::Relaxed) as u16);

        let instance = WhiteLabelInstance {
            tenant_id: *tenant_id,
            api_endpoint: format!("https://{}.swiftbridge.io:{}", config.domain.replace('.', "-"), api_port),
            fix_endpoint: format!("tcp://{}.swiftbridge.io:{}", config.domain.replace('.', "-"), fix_port),
            dashboard_url: format!("https://dashboard.{}/admin", config.domain),
            config: config.clone(),
            active: true,
            total_orders: 0,
            total_volume: 0.0,
            provisioned_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        };

        self.instances.insert(*tenant_id, instance.clone());
        self.total_deployments.fetch_add(1, Ordering::Relaxed);
        self.tenants.upgrade_tenant(tenant_id, Tier::Enterprise).ok();

        tracing::info!(
            "White-Label Exchange deployed for tenant {}: {} at {}",
            tenant_id, config.brand_name, instance.api_endpoint
        );

        Ok(instance)
    }

    pub fn get_instance(&self, tenant_id: &Uuid) -> Option<WhiteLabelInstance> {
        self.instances.get(tenant_id).map(|i| i.clone())
    }

    pub fn record_order(&self, tenant_id: &Uuid) {
        if let Some(mut inst) = self.instances.get_mut(tenant_id) {
            inst.total_orders += 1;
        }
        self.billing.record_order(tenant_id);
    }

    pub fn record_volume(&self, tenant_id: &Uuid, volume: f64) {
        if let Some(mut inst) = self.instances.get_mut(tenant_id) {
            inst.total_volume += volume;
        }
    }

    pub fn list_instances(&self) -> Vec<WhiteLabelInstance> {
        self.instances.iter().map(|i| i.clone()).collect()
    }

    pub fn update_config(&self, tenant_id: &Uuid, config: WhiteLabelConfig) -> Result<WhiteLabelInstance, String> {
        let mut inst = self.instances.get_mut(tenant_id).ok_or("instance not found")?;
        let api_port = config.custom_api_port.unwrap_or(5000);
        let fix_port = config.custom_fix_port.unwrap_or(6000);
        inst.config = config;
        inst.api_endpoint = format!("https://{}.swiftbridge.io:{}", inst.config.domain.replace('.', "-"), api_port);
        inst.fix_endpoint = format!("tcp://{}.swiftbridge.io:{}", inst.config.domain.replace('.', "-"), fix_port);
        inst.dashboard_url = format!("https://dashboard.{}/admin", inst.config.domain);
        let result = inst.clone();
        Ok(result)
    }

    pub fn remove_instance(&self, tenant_id: &Uuid) -> bool {
        self.instances.remove(tenant_id).is_some()
    }

    pub fn deployment_count(&self) -> u64 {
        self.total_deployments.load(Ordering::Relaxed)
    }
}
