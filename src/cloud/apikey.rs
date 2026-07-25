use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKey {
    pub key_id: Uuid,
    pub tenant_id: Uuid,
    pub prefix: String,
    pub key_hash: Vec<u8>,
    pub created_at: i64,
    pub expires_at: i64,
    pub active: bool,
}

impl ApiKey {
    pub fn new(tenant_id: Uuid, secret: &[u8]) -> Result<Self, String> {
        let key_id = Uuid::new_v4();
        let prefix = hex::encode(&key_id.as_bytes()[..4]);
        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| format!("HMAC key init: {}", e))?;
        mac.update(prefix.as_bytes());
        mac.update(tenant_id.as_bytes());
        let key_hash = mac.finalize().into_bytes().to_vec();

        Ok(Self {
            key_id,
            tenant_id,
            prefix,
            key_hash,
            created_at: chrono::Utc::now().timestamp_millis(),
            expires_at: chrono::Utc::now().timestamp_millis() + 365 * 24 * 3600 * 1000,
            active: true,
        })
    }

    pub fn verify(&self, secret: &[u8]) -> Result<bool, String> {
        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|e| format!("HMAC key init: {}", e))?;
        mac.update(self.prefix.as_bytes());
        mac.update(self.tenant_id.as_bytes());
        Ok(mac.verify_slice(&self.key_hash).is_ok())
    }

    pub fn full_key(&self, _secret: &[u8]) -> String {
        format!("tb_{}_{}", self.prefix, hex::encode(self.key_hash.as_slice()))
    }
}

pub struct ApiKeyManager {
    keys: dashmap::DashMap<Uuid, ApiKey>,
    by_prefix: dashmap::DashMap<String, Uuid>,
    master_secret: Vec<u8>,
}

impl ApiKeyManager {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            keys: dashmap::DashMap::new(),
            by_prefix: dashmap::DashMap::new(),
            master_secret: secret.to_vec(),
        }
    }

    pub fn create_key(&self, tenant_id: Uuid) -> Result<(ApiKey, String), String> {
        let key = ApiKey::new(tenant_id, &self.master_secret)?;
        let full = key.full_key(&self.master_secret);
        let prefix = key.prefix.clone();
        let key_id = key.key_id;
        self.keys.insert(key_id, key);
        let stored = self.keys.get(&key_id)
            .ok_or_else(|| "ApiKey not found after insert".to_string())?;
        let stored = ApiKey::clone(&stored);
        self.by_prefix.insert(prefix, stored.key_id);
        Ok((stored.clone(), full))
    }

    pub fn validate_key(&self, raw_key: &str) -> Option<ApiKey> {
        let parts: Vec<&str> = raw_key.split('_').collect();
        if parts.len() < 3 || parts[0] != "tb" {
            return None;
        }
        let prefix = parts[1];
        let key_id = self.by_prefix.get(prefix)?;
        let key = self.keys.get(&*key_id)?;
        if !key.active {
            return None;
        }
        if chrono::Utc::now().timestamp_millis() > key.expires_at {
            return None;
        }
        Some(key.clone())
    }

    pub fn revoke_key(&self, key_id: &Uuid) -> Result<(), String> {
        if let Some(mut key) = self.keys.get_mut(key_id) {
            key.active = false;
            Ok(())
        } else {
            Err("key not found".into())
        }
    }

    pub fn list_keys_for_tenant(&self, tenant_id: &Uuid) -> Vec<ApiKey> {
        self.keys.iter()
            .filter(|e| e.tenant_id == *tenant_id)
            .map(|e| e.clone())
            .collect()
    }
}
