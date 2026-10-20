use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use mini_social_feed_app::AuthPort;
use mini_social_feed_data::{RateLimitConfig, TieredLimits};
use mini_social_feed_domain::UserId;
use mini_social_feed_infra::RateLimiter;
use serde_json::json;
use tower::{Layer, Service};

// ---------------------------------------------------------------------------
// Glue trait — implement on your AppState
// ---------------------------------------------------------------------------

pub trait HasAuthPort {
    fn auth_port(&self) -> &dyn AuthPort;
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

/// Injects the authenticated user id into a handler.
pub struct AuthUser(pub UserId);

impl<S> FromRequestParts<S> for AuthUser
where
    S: HasAuthPort + Send + Sync,
{
    type Rejection = Response;

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        state: &'life1 S,
    ) -> Pin<Box<dyn Future<Output = Result<Self, Self::Rejection>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let auth = state.auth_port();

        Box::pin(async move {
            let h = header.ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "missing authorization header"})),
                )
                    .into_response()
            })?;

            let token = h.strip_prefix("Bearer ").ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid authorization format"})),
                )
                    .into_response()
            })?;

            // Try API key auth first, then fall back to JWT.
            if let Ok(user_id) = auth.verify_api_key(token).await {
                return Ok(AuthUser(user_id));
            }

            let user_id_str = auth.verify_token(token).map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid or expired token"})),
                )
                    .into_response()
            })?;

            Ok(AuthUser(UserId(user_id_str)))
        })
    }
}

// ---------------------------------------------------------------------------
// Rate limiting — tower Layer + Service
// ---------------------------------------------------------------------------

/// Tower Layer that wraps a router in a rate-limiting middleware.
///
/// Classifies requests into actions (login, post_create, read, etc.),
/// builds a bucket key from the auth token or client IP, and enforces
/// the configured tiered limits.
#[derive(Clone)]
pub struct RateLimitLayer {
    limiter: Arc<RateLimiter>,
    config: Arc<RateLimitConfig>,
}

impl RateLimitLayer {
    pub fn new(limiter: Arc<RateLimiter>, config: Arc<RateLimitConfig>) -> Self {
        Self { limiter, config }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            limiter: self.limiter.clone(),
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    limiter: Arc<RateLimiter>,
    config: Arc<RateLimitConfig>,
}

impl<S, B> Service<axum::http::Request<B>> for RateLimitService<S>
where
    S: Service<axum::http::Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<B>) -> Self::Future {
        let limiter = self.limiter.clone();
        let config = self.config.clone();
        let mut inner = self.inner.clone();

        let method = req.method().clone();
        let path = req.uri().path().to_string();

        // Extract user_id from auth header for key building
        let auth_header = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Box::pin(async move {
            if !config.enabled {
                return inner.call(req).await;
            }

            let action = classify_action(&method, &path);

            // Build key: try user id from token, fall back to IP
            let key = if let Some(ref h) = auth_header {
                if let Some(token) = h.strip_prefix("Bearer ") {
                    // Try to extract user_id from the token by decoding the JWT
                    // We parse it ourselves to avoid needing AuthPort here
                    if let Some(user_id) = extract_user_id_from_jwt(token) {
                        // Determine if agent (has API key pattern) or human
                        let is_api_key = token.len() >= 32 && !token.contains('.');
                        let scope = if is_api_key { "agent" } else { "human" };
                        format!("{}:{}:{}", scope, user_id, action)
                    } else {
                        // Can't decode — use IP
                        format!("ip:unknown:{}", action)
                    }
                } else {
                    format!("ip:unknown:{}", action)
                }
            } else {
                format!("ip:unknown:{}", action)
            };

            // Determine the profile (public / human / agent) and tier
            let is_agent = key.starts_with("agent:");
            let is_authenticated = key.starts_with("human:") || is_agent;

            let limit: Option<mini_social_feed_data::ActionLimit> = if is_authenticated {
                let profile: &TieredLimits = if is_agent {
                    &config.agent
                } else {
                    &config.human
                };
                let tier_name = "minimal"; // default — we can't look up user tier here
                profile
                    .tiers
                    .get(tier_name)
                    .and_then(|t| t.actions.get(action).copied())
            } else {
                config.public.actions.get(action).copied()
            };

            if let Some(lim) = limit {
                if !limiter.check(&key, &lim).await {
                    let retry = limiter.retry_after(&key, &lim).await;
                    return Ok((
                        StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", retry.to_string())],
                        Json(json!({
                            "error": "rate limit exceeded",
                            "retry_after_secs": retry,
                        })),
                    )
                        .into_response());
                }
            }

            inner.call(req).await
        })
    }
}

/// Classify a request into a rate-limit action.
fn classify_action(method: &Method, path: &str) -> &'static str {
    match (method, path) {
        (&Method::POST, p) if p == "/api/auth/login" => "login",
        (&Method::POST, p) if p == "/api/users/register" => "register",
        (&Method::POST, p) if p == "/api/posts" => "post_create",
        (&Method::POST, p) if p == "/api/media" => "media_upload",
        (&Method::GET, _) => "read",
        (_, p) if p.starts_with("/api/search/") => "read",
        _ => "write",
    }
}

/// Extract user_id from a JWT token without verification.
/// This is for rate-limit key building only — actual auth happens in AuthUser.
fn extract_user_id_from_jwt(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = parts[1];
    let decoded = base64_url_decode(payload)?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json.get("sub").and_then(|v| v.as_str().map(String::from))
}

fn base64_url_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    // Standard base64url → standard base64
    let s = s.replace('-', "+").replace('_', "/");
    // Pad to multiple of 4
    let padded = match s.len() % 4 {
        2 => format!("{s}=="),
        3 => format!("{s}="),
        _ => s,
    };
    base64::engine::general_purpose::STANDARD
        .decode(padded)
        .ok()
}
