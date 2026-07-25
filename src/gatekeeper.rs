use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{Json, IntoResponse, Redirect, Response},
};
use std::sync::Arc;
use tracing::warn;

use crate::{sovereign_fortress::AuditEntry};

pub const PUBLIC_ROUTES: &[&str] = &[
    "/api/v1/health",
    "/ready",
    "/metrics",
    "/api/v1/auth/register",
    "/api/v1/auth/verify",
    "/webhook/payment",
    "/dashboard",
    "/ws/dashboard",
    "/cloud/tenants",
    "/cloud/tenants/",
    "/docs",
    "/api/v1/docs",
    "/api/v1/openapi.json",
    "/trade",
    "/trade/",
    "/api/v1/ai/chat",
    "/api/v1/ai/config",
    "/api/v1/ai/status",
];

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    for public in PUBLIC_ROUTES {
        if path.starts_with(public) {
            return next.run(req).await;
        }
    }

    let headers: &HeaderMap = req.headers();
    let auth_header = match headers.get("Authorization") {
        Some(v) => v.to_str().unwrap_or(""),
        None => "",
    };

    if !auth_header.starts_with("Bearer ") {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "missing or malformed Authorization header"}))).into_response();
    }

    let key = &auth_header[7..];
    if state.api_keys.validate_key(key).is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid API key"}))).into_response();
    }

    next.run(req).await
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let path = req.uri().path().to_string();
    let method = req.method().to_string();
    let tier = if path.starts_with("/api/v1/auth") { "auth" }
        else if path.starts_with("/api/v1/order") { "trading" }
        else if path.starts_with("/webhook") { "webhook" }
        else if path.starts_with("/dashboard") || path.starts_with("/docs") { "public" }
        else if path.starts_with("/admin") || path.starts_with("/cloud") { "admin" }
        else { "default" };

    if !state.rate_limiter.check(&ip, tier) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({
            "error": "rate limit exceeded",
            "tier": tier,
            "retry_after": 1
        }))).into_response();
    }

    let res = next.run(req).await;

    if res.status().as_u16() >= 400 && !path.starts_with("/docs") && !path.starts_with("/ready") && !path.starts_with("/metrics") {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now().timestamp_millis(),
            tenant_id: None,
            action: method,
            resource: path,
            ip,
            status: res.status().as_u16(),
        };
        state.audit_log.log(entry);
    }

    res
}

pub async fn security_headers_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut res = next.run(req).await;

    res.headers_mut().insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        axum::http::HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
    );
    res.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    res.headers_mut().insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    res.headers_mut().insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    res.headers_mut().insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        axum::http::HeaderValue::from_static("camera=(), microphone=(), geolocation=(), interest-cohort="),
    );
    res.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-xss-protection"),
        axum::http::HeaderValue::from_static("0"),
    );

    res
}

pub fn tls_enabled() -> bool {
    let cert = std::env::var("THE_BRIDGE_TLS_CERT").ok();
    let key = std::env::var("THE_BRIDGE_TLS_KEY").ok();
    cert.is_some() && key.is_some()
}

pub async fn tls_redirect_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.method() == axum::http::Method::OPTIONS || req.uri().path() == "/api/v1/health" || !tls_enabled() {
        return next.run(req).await;
    }
    let is_https = req
        .headers()
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "https")
        .unwrap_or(false);

    if !is_https {
        let host = req
            .headers()
            .get("Host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost:3001");
        let uri = req.uri();
        let https_url = format!("https://{}{}", host, uri);
        return Redirect::permanent(&https_url).into_response();
    }
    next.run(req).await
}
