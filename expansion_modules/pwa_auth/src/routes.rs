use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::AuthConfig;
use crate::jwt::{generate_token_pair, refresh_access_token};
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::webauthn::WebAuthnManager;

#[derive(Clone)]
pub struct AppState {
    pub webauthn: Arc<WebAuthnManager>,
    pub sessions: Arc<InMemorySessionStore>,
    pub config: AuthConfig,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/webauthn/register/start", post(start_registration))
        .route("/webauthn/register/finish", post(finish_registration))
        .route("/webauthn/login/start", post(start_login))
        .route("/webauthn/login/finish", post(finish_login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/session", get(session_info))
        .with_state(state)
}

pub fn cors_layer(origins: &[String]) -> tower_http::cors::CorsLayer {
    let mut allow_origin: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();
    if allow_origin.is_empty() {
        allow_origin.push(HeaderValue::from_static("*"));
    }
    tower_http::cors::CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::HeaderName::from_static("x-passkey"),
        ])
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
}

#[derive(Debug, Deserialize)]
pub struct RegistrationStartRequest {
    pub user_id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegistrationStartResponse {
    pub challenge_id: String,
    pub challenge: String,
    pub rp_id: String,
    pub rp_name: String,
    pub user_id: String,
    pub origin: String,
}

#[derive(Debug, Deserialize)]
pub struct RegistrationFinishRequest {
    pub challenge_id: String,
    pub client_data_json: String,
    pub attestation_object: String,
    pub credential_id: String,
    pub public_key: String,
}

#[derive(Debug, Serialize)]
pub struct RegistrationFinishResponse {
    pub success: bool,
    pub credential_id: String,
}

pub async fn start_registration(
    State(state): State<AppState>,
    Json(req): Json<RegistrationStartRequest>,
) -> impl IntoResponse {
    match state.webauthn.start_registration(&req.user_id).await {
        Ok(resp) => Json(serde_json::json!({
            "challenge_id": resp.challenge_id,
            "challenge": resp.challenge,
            "rp_id": resp.rp_id,
            "rp_name": resp.rp_name,
            "user_id": resp.user_id,
            "origin": state.webauthn.get_origin(),
            "timeout_ms": state.config.challenge_ttl_seconds * 1000,
        }))
        .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

pub async fn finish_registration(
    State(state): State<AppState>,
    Json(req): Json<RegistrationFinishRequest>,
) -> impl IntoResponse {
    let client_data_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.client_data_json)
        .unwrap_or_default();
    let attestation_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.attestation_object)
        .unwrap_or_default();
    let public_key_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.public_key)
        .unwrap_or_default();

    match state
        .webauthn
        .finish_registration(
            &req.challenge_id,
            &client_data_bytes,
            &attestation_bytes,
            &req.credential_id,
            &public_key_bytes,
        )
        .await
    {
        Ok(resp) => Json(serde_json::json!({
            "success": true,
            "credential_id": resp.credential_id,
        }))
        .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginStartRequest {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginFinishRequest {
    pub challenge_id: String,
    pub authenticator_data: String,
    pub signature: String,
    pub client_data_json: String,
}

pub async fn start_login(
    State(state): State<AppState>,
    Json(req): Json<LoginStartRequest>,
) -> impl IntoResponse {
    match state.webauthn.start_login(&req.user_id).await {
        Ok(resp) => Json(serde_json::json!({
            "challenge_id": resp.challenge_id,
            "challenge": resp.challenge,
            "rp_id": resp.rp_id,
            "allow_credentials": resp.allow_credentials.iter().map(|c| serde_json::json!({
                "id": c.id,
                "type": "public-key",
                "transports": ["usb", "internal"],
            })).collect::<Vec<_>>(),
            "timeout_ms": state.config.challenge_ttl_seconds * 1000,
        }))
        .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

pub async fn finish_login(
    State(state): State<AppState>,
    Json(req): Json<LoginFinishRequest>,
) -> impl IntoResponse {
    let auth_data_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.authenticator_data)
        .unwrap_or_default();
    let signature_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.signature)
        .unwrap_or_default();
    let client_data_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&req.client_data_json)
        .unwrap_or_default();

    match state
        .webauthn
        .finish_login(&req.challenge_id, &auth_data_bytes, &signature_bytes, &client_data_bytes)
        .await
    {
        Ok(resp) => {
            if resp.success {
                let session = Session::new(
                    &resp.user_id,
                    &resp.credential_id,
                    state.config.token_ttl_seconds,
                );
                let _ = state.sessions.create(session.clone()).await;

                let tokens = generate_token_pair(
                    &session.user_id,
                    "user",
                    vec!["read".to_string(), "write".to_string(), "trade".to_string()],
                    &state.config.jwt_secret,
                    state.config.token_ttl_seconds,
                    state.config.refresh_ttl_seconds,
                );

                match tokens {
                    Ok(tokens) => Json(serde_json::json!({
                        "success": true,
                        "access_token": tokens.access_token,
                        "refresh_token": tokens.refresh_token,
                        "expires_in": tokens.expires_in,
                        "token_type": "Bearer",
                        "session_id": session.id,
                    }))
                    .into_response(),
                    Err(e) => error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("token generation failed: {}", e),
                    ),
                }
            } else {
                error_response(StatusCode::UNAUTHORIZED, "authentication failed")
            }
        }
        Err(e) => error_response(StatusCode::UNAUTHORIZED, &e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: usize,
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> impl IntoResponse {
    match refresh_access_token(
        &req.refresh_token,
        &state.config.jwt_secret,
        state.config.token_ttl_seconds,
    ) {
        Ok(tokens) => Json(RefreshResponse {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
        })
        .into_response(),
        Err(_) => error_response(StatusCode::UNAUTHORIZED, "invalid refresh token"),
    }
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub session_id: String,
}

pub async fn logout(
    State(state): State<AppState>,
    Json(req): Json<LogoutRequest>,
) -> impl IntoResponse {
    let destroyed = state.sessions.destroy(&req.session_id).await;
    Json(serde_json::json!({
        "success": destroyed,
    }))
    .into_response()
}

pub async fn session_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let auth_header = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok());
    match auth_header.and_then(|h| h.strip_prefix("Bearer ")) {
        Some(token) => match crate::jwt::verify_token(token, &state.config.jwt_secret) {
            Ok(claims) => Json(serde_json::json!({
                "authenticated": true,
                "user": {
                    "id": claims.sub,
                    "role": claims.role,
                    "permissions": claims.permissions,
                },
            }))
            .into_response(),
            Err(_) => error_response(StatusCode::UNAUTHORIZED, "invalid token"),
        },
        None => Json(serde_json::json!({
            "authenticated": false,
            "session": null,
        }))
        .into_response(),
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "success": false,
        "error": message,
    });
    (status, Json(body)).into_response()
}

#[allow(dead_code)]
pub fn build_state(config: AuthConfig) -> AppState {
    let webauthn = Arc::new(WebAuthnManager::new(
        config.rp_id.clone(),
        config.rp_name.clone(),
        config.origin.clone(),
        config.challenge_ttl_seconds,
    ));
    let sessions = Arc::new(InMemorySessionStore::new(300));
    AppState {
        webauthn,
        sessions,
        config,
    }
}
