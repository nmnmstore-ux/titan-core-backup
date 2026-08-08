use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{FiatError, Result};

type HmacSha256 = Hmac<Sha256>;

pub fn verify_stripe_signature(
    payload: &[u8],
    signature: &str,
    secret: &str,
    tolerance_secs: Option<u64>,
) -> Result<bool> {
    let tolerance = tolerance_secs.unwrap_or(300);

    let parts: Vec<&str> = signature.split(',').collect();
    let mut timestamp = String::new();
    let mut signature_hash = String::new();

    for part in parts {
        if let Some(rest) = part.strip_prefix("t=") {
            timestamp = rest.to_string();
        } else if let Some(rest) = part.strip_prefix("v1=") {
            signature_hash = rest.to_string();
        }
    }

    if timestamp.is_empty() || signature_hash.is_empty() {
        return Err(FiatError::InvalidRequest(
            "invalid stripe signature format".to_string(),
        ));
    }

    let ts = timestamp
        .parse::<i64>()
        .map_err(|_| FiatError::InvalidRequest("invalid timestamp".to_string()))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| FiatError::Internal("system time error".to_string()))?
        .as_secs() as i64;

    if (now - ts).abs() > tolerance as i64 {
        return Err(FiatError::SignatureFailed);
    }

    let payload_str = format!("{}:{}", ts, std::str::from_utf8(payload).unwrap_or(""));
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| FiatError::Internal("hmac init failed".to_string()))?;
    mac.update(payload_str.as_bytes());
    let expected_hash = hex::encode(mac.finalize().into_bytes());

    Ok(expected_hash == signature_hash)
}

type HmacSha512 = Hmac<sha2::Sha512>;

pub fn verify_banxa_signature(
    payload: &[u8],
    signature: &str,
    secret: &str,
    timestamp: u64,
) -> Result<bool> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| FiatError::Internal("system time error".to_string()))?
        .as_secs();

    if now > timestamp && now - timestamp > 300 {
        return Err(FiatError::SignatureFailed);
    }
    if timestamp > now && timestamp - now > 300 {
        return Err(FiatError::SignatureFailed);
    }

    let payload_str = std::str::from_utf8(payload).unwrap_or("");
    let signature_payload = format!("{}{}", timestamp, payload_str);

    let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
        .map_err(|_| FiatError::Internal("hmac init failed".to_string()))?;
    mac.update(signature_payload.as_bytes());
    let expected_hash = hex::encode(mac.finalize().into_bytes());

    Ok(expected_hash == signature)
}

pub fn extract_stripe_event_type(payload: &serde_json::Value) -> Option<String> {
    payload["type"].as_str().map(|s| s.to_string())
}

pub fn extract_stripe_data(payload: &serde_json::Value) -> Option<String> {
    payload["data"]["object"]["id"].as_str().map(|s| s.to_string())
}

pub fn extract_banxa_event_type(payload: &serde_json::Value) -> Option<String> {
    payload["data"]["type"].as_str().map(|s| s.to_string())
}

pub fn extract_banxa_order_id(payload: &serde_json::Value) -> Option<String> {
    payload["data"]["order_id"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| payload["data"]["order"]["id"].as_str().map(|s| s.to_string()))
}

pub fn parse_stripe_event(raw_payload: &[u8]) -> Result<StripeWebhookEvent> {
    let value: serde_json::Value = serde_json::from_slice(raw_payload)
        .map_err(|e| FiatError::Serialization(e.to_string()))?;

    let event_type = value["type"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let data = value["data"]["object"].clone();

    Ok(StripeWebhookEvent {
        event_type,
        data,
    })
}

pub fn parse_banxa_event(raw_payload: &[u8]) -> Result<BanxaWebhookEvent> {
    let value: serde_json::Value = serde_json::from_slice(raw_payload)
        .map_err(|e| FiatError::Serialization(e.to_string()))?;

    let event_type = value["event"]["type"]
        .as_str()
        .or_else(|| value["type"].as_str())
        .unwrap_or("unknown")
        .to_string();

    let order_id = value["data"]["order_id"]
        .as_str()
        .or_else(|| value["data"]["order"]["id"].as_str())
        .unwrap_or("")
        .to_string();

    let status = value["data"]["status"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    Ok(BanxaWebhookEvent {
        event_type,
        order_id,
        status,
        data: value,
    })
}

pub struct StripeWebhookEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

pub struct BanxaWebhookEvent {
    pub event_type: String,
    pub order_id: String,
    pub status: String,
    pub data: serde_json::Value,
}

#[allow(dead_code)]
pub fn hash_payload(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hex::encode(hasher.finalize())
}
