use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use mini_social_feed_domain::{Direction, EntityId, Relation, UserId, UserResponse};

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/relationships/:id/subscribe
// ---------------------------------------------------------------------------

pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let target = UserId(target_id);
    if state.users.get_by_id(&target).await?.is_none() {
        return Err(HttpError::NotFound("user not found".into()));
    }
    state
        .graph
        .relate(
            &EntityId::User(auth.0),
            &EntityId::User(target),
            Relation::Subscribe,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/relationships/:id/subscribe
// ---------------------------------------------------------------------------

pub async fn unsubscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state
        .graph
        .unrelate(
            &EntityId::User(auth.0),
            &EntityId::User(UserId(target_id)),
            Relation::Subscribe,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/relationships/:id/connect  — send connection request
// ---------------------------------------------------------------------------

pub async fn connect_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let target = UserId(target_id);
    if state.users.get_by_id(&target).await?.is_none() {
        return Err(HttpError::NotFound("user not found".into()));
    }
    let already = state
        .graph
        .check_relation(
            &EntityId::User(auth.0.clone()),
            &EntityId::User(target.clone()),
            Relation::Connected,
        )
        .await?;
    if already {
        return Ok(StatusCode::NO_CONTENT);
    }
    state
        .graph
        .relate(
            &EntityId::User(auth.0),
            &EntityId::User(target),
            Relation::ConnectRequest,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/relationships/:id/accept  — accept a connection request
// ---------------------------------------------------------------------------

pub async fn connect_accept(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(requester_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let requester = UserId(requester_id);
    let pending = state
        .graph
        .check_relation(
            &EntityId::User(requester.clone()),
            &EntityId::User(auth.0.clone()),
            Relation::ConnectRequest,
        )
        .await?;
    if !pending {
        return Err(HttpError::NotFound("no pending connection request".into()));
    }
    state
        .graph
        .unrelate(
            &EntityId::User(requester.clone()),
            &EntityId::User(auth.0.clone()),
            Relation::ConnectRequest,
        )
        .await?;
    state
        .graph
        .relate(
            &EntityId::User(requester.clone()),
            &EntityId::User(auth.0.clone()),
            Relation::Connected,
        )
        .await?;
    state
        .graph
        .relate(
            &EntityId::User(auth.0),
            &EntityId::User(requester),
            Relation::Connected,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/relationships/:id/connect  — remove connection
// ---------------------------------------------------------------------------

pub async fn connect_remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(other_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let other = UserId(other_id);
    state
        .graph
        .unrelate(
            &EntityId::User(auth.0.clone()),
            &EntityId::User(other.clone()),
            Relation::Connected,
        )
        .await?;
    state
        .graph
        .unrelate(
            &EntityId::User(other),
            &EntityId::User(auth.0),
            Relation::Connected,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/relationships/:id/block
// ---------------------------------------------------------------------------

pub async fn block(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    let target = UserId(target_id);
    if state.users.get_by_id(&target).await?.is_none() {
        return Err(HttpError::NotFound("user not found".into()));
    }
    state
        .graph
        .relate(
            &EntityId::User(auth.0),
            &EntityId::User(target),
            Relation::Block,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/relationships/:id/block
// ---------------------------------------------------------------------------

pub async fn unblock(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state
        .graph
        .unrelate(
            &EntityId::User(auth.0),
            &EntityId::User(UserId(target_id)),
            Relation::Block,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/relationships/subscribers  — who subscribes to me
// ---------------------------------------------------------------------------

pub async fn list_subscribers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let ids = state
        .graph
        .related(
            &EntityId::User(auth.0.clone()),
            Relation::Subscribe,
            Direction::Inbound,
        )
        .await?;
    let mut users = Vec::new();
    for id in ids {
        if let EntityId::User(uid) = id {
            if let Some(user) = state.users.get_by_id(&uid).await? {
                users.push(UserResponse::from(&user));
            }
        }
    }
    Ok(Json(users))
}

// ---------------------------------------------------------------------------
// GET /api/relationships/subscriptions  — who I subscribe to
// ---------------------------------------------------------------------------

pub async fn list_subscriptions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let ids = state
        .graph
        .related(
            &EntityId::User(auth.0.clone()),
            Relation::Subscribe,
            Direction::Outbound,
        )
        .await?;
    let mut users = Vec::new();
    for id in ids {
        if let EntityId::User(uid) = id {
            if let Some(user) = state.users.get_by_id(&uid).await? {
                users.push(UserResponse::from(&user));
            }
        }
    }
    Ok(Json(users))
}

// ---------------------------------------------------------------------------
// GET /api/relationships/connections  — mutual connections
// ---------------------------------------------------------------------------

pub async fn list_connections(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let ids = state
        .graph
        .related(
            &EntityId::User(auth.0.clone()),
            Relation::Connected,
            Direction::Outbound,
        )
        .await?;
    let mut users = Vec::new();
    for id in ids {
        if let EntityId::User(uid) = id {
            if let Some(user) = state.users.get_by_id(&uid).await? {
                users.push(UserResponse::from(&user));
            }
        }
    }
    Ok(Json(users))
}
