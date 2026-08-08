use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use std::sync::Arc;

use crate::config::AuthConfig;
use crate::jwt::{verify_token, Claims};

#[derive(Debug, Clone)]
pub struct VerifiedUser {
    pub user_id: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuthState {
    pub config: AuthConfig,
}

impl AuthState {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }
}

impl From<Claims> for VerifiedUser {
    fn from(claims: Claims) -> Self {
        Self {
            user_id: claims.sub,
            role: claims.role,
            permissions: claims.permissions,
        }
    }
}

impl<S> FromRequestParts<S> for VerifiedUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        let auth_header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing auth header"))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "invalid auth header"))?;

        let extension = parts
            .extensions
            .get::<Arc<AuthState>>()
            .ok_or((StatusCode::UNAUTHORIZED, "auth state not configured"))?;

        let claims = verify_token(token, &extension.config.jwt_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))?;

        Ok(claims.into())
    }
}

pub async fn require_role(
    required_role: &str,
    user: &VerifiedUser,
) -> Result<(), (StatusCode, &'static str)> {
    if user.role == required_role || user.role == "admin" {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "insufficient permissions"))
    }
}
