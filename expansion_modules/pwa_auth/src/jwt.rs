use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub exp: usize,
    pub iat: usize,
    pub token_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: usize,
}

pub fn generate_token(
    user_id: &str,
    role: &str,
    permissions: Vec<String>,
    secret: &[u8],
    ttl_seconds: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::seconds(ttl_seconds as i64);

    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        permissions,
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
        token_type: "access".to_string(),
    };

    let header = Header::new(Algorithm::HS256);
    encode(&header, &claims, &EncodingKey::from_secret(secret))
}

pub fn generate_refresh_token(
    user_id: &str,
    secret: &[u8],
    ttl_seconds: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::seconds(ttl_seconds as i64);

    let claims = Claims {
        sub: user_id.to_string(),
        role: "refresh".to_string(),
        permissions: vec![],
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
        token_type: "refresh".to_string(),
    };

    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret))
}

pub fn verify_token(token: &str, secret: &[u8]) -> Result<Claims, jsonwebtoken::errors::Error> {
    let validation = Validation::new(Algorithm::HS256);
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

pub fn refresh_access_token(
    refresh_token: &str,
    secret: &[u8],
    access_ttl: u64,
) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let claims = verify_token(refresh_token, secret)?;

    if claims.token_type != "refresh" {
        return Err(jsonwebtoken::errors::ErrorKind::InvalidToken.into());
    }

    let access = generate_token(
        &claims.sub,
        &claims.role,
        claims.permissions,
        secret,
        access_ttl,
    )?;
    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh_token.to_string(),
        expires_in: access_ttl as usize,
    })
}

pub fn generate_token_pair(
    user_id: &str,
    role: &str,
    permissions: Vec<String>,
    secret: &[u8],
    access_ttl: u64,
    refresh_ttl: u64,
) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let access = generate_token(user_id, role, permissions.clone(), secret, access_ttl)?;
    let refresh = generate_refresh_token(user_id, secret, refresh_ttl)?;

    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh,
        expires_in: access_ttl as usize,
    })
}

pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}