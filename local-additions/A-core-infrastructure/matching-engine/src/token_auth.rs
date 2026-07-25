use base64::Engine;
use hmac::{Hmac, Mac};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    pub token: String,
    pub tenant_id: Uuid,
    pub expires_at: u64,
}

pub struct TokenAuth {
    secret: Vec<u8>,
    refresh_store: RwLock<HashMap<String, RefreshToken>>,
}

impl TokenAuth {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
            refresh_store: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_access_token(&self, tenant_id: Uuid, tier: &str) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let claims = JwtClaims {
            sub: tenant_id.to_string(),
            exp: now + 900,      // 15 minutes
            iat: now,
            tier: tier.to_string(),
        };
        self.encode(&claims)
    }

    pub fn create_refresh_token(&self, tenant_id: Uuid) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let token = Uuid::new_v4().to_string();
        let rt = RefreshToken {
            token: token.clone(),
            tenant_id,
            expires_at: now + 7 * 86400, // 7 days
        };
        self.refresh_store.write().insert(token.clone(), rt);
        token
    }

    pub fn validate_access_token(&self, token: &str) -> Option<JwtClaims> {
        self.decode(token).ok().filter(|c| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            c.exp > now
        })
    }

    pub fn rotate_refresh_token(&self, token: &str) -> Option<(String, String, Uuid)> {
        let store = self.refresh_store.read();
        let rt = store.get(token)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if rt.expires_at <= now {
            return None;
        }
        let tenant_id = rt.tenant_id;
        drop(store);
        self.refresh_store.write().remove(token);
        let tier = "pro";
        let new_access = self.create_access_token(tenant_id, tier);
        let new_refresh = self.create_refresh_token(tenant_id);
        Some((new_access, new_refresh, tenant_id))
    }

    fn encode(&self, claims: &JwtClaims) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let payload_json = serde_json::to_string(claims).unwrap_or_default();
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
        let message = format!("{}.{}", header, payload);
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC key");
        mac.update(message.as_bytes());
        let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{}.{}.{}", header, payload, sig)
    }

    fn decode(&self, token: &str) -> Result<JwtClaims, String> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err("invalid token format".into());
        }
        let message = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2]).map_err(|e| format!("sig decode: {}", e))?;

        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|e| format!("HMAC: {}", e))?;
        mac.update(message.as_bytes());
        mac.verify_slice(&sig_bytes).map_err(|_| "invalid signature".to_string())?;

        let payload_json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1]).map_err(|e| format!("payload decode: {}", e))?;
        serde_json::from_slice(&payload_json).map_err(|e| format!("json: {}", e))
    }
}
