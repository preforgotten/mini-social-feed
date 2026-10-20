use axum::{extract::State, http::StatusCode, Json};

use mini_social_feed_domain::{
    AuthChallengeResponse, AuthVerifyRequest, LoginRequest, NostrVerifyResponse, RefreshRequest,
    RefreshResponse, UserResponse,
};

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/auth/challenge
// ---------------------------------------------------------------------------

pub async fn request_challenge(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<AuthChallengeResponse>, HttpError> {
    let public_key = body
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HttpError::BadRequest("missing 'public_key' field".into()))?;

    let (challenge_id, challenge_bytes) = state.auth.create_challenge(public_key).await?;

    Ok(Json(AuthChallengeResponse {
        challenge_id,
        challenge_hex: hex::encode(challenge_bytes),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/auth/verify  (stateless — proves key ownership, no token)
// ---------------------------------------------------------------------------

pub async fn verify_challenge(
    State(state): State<AppState>,
    Json(req): Json<AuthVerifyRequest>,
) -> Result<Json<NostrVerifyResponse>, HttpError> {
    state
        .auth
        .verify_challenge(&req.challenge_id, &req.public_key, &req.signature)
        .await?;

    Ok(Json(NostrVerifyResponse { verified: true }))
}

// ---------------------------------------------------------------------------
// POST /api/auth/refresh  (rotate tokens — consumes old refresh, issues new pair)
// ---------------------------------------------------------------------------

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, HttpError> {
    let user_id = state.auth.consume_refresh_token(&req.refresh_token).await?;
    let access_token = state.auth.create_token(&user_id)?;
    let refresh_token = state.auth.create_refresh_token(&user_id).await?;

    Ok(Json(RefreshResponse {
        access_token,
        refresh_token,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/auth/login   (username + password → JWT pair)
// ---------------------------------------------------------------------------

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let user = state.auth.login(&req.username, &req.password).await?;
    let access_token = state.auth.create_token(&user.id.0)?;
    let refresh_token = state.auth.create_refresh_token(&user.id.0).await?;

    let avatar_url = state.resolve_avatar_url(&user).await;
    let mut user_resp = UserResponse::from(&user);
    user_resp.avatar_url = avatar_url;

    Ok(Json(serde_json::json!({
        "user": user_resp,
        "access_token": access_token,
        "refresh_token": refresh_token,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/auth/api-key  (authenticated — generate an API key)
// ---------------------------------------------------------------------------

pub async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, HttpError> {
    let api_key = state.auth.create_api_key(&auth.0).await?;
    Ok(Json(serde_json::json!({"api_key": api_key})))
}

// ---------------------------------------------------------------------------
// POST /api/auth/logout  (authenticated — revoke all refresh tokens)
// ---------------------------------------------------------------------------

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<serde_json::Value>), HttpError> {
    state.auth.revoke_token_family(&auth.0 .0).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"status": "logged out"})),
    ))
}
