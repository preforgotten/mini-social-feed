use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use mini_social_feed_domain::{
    Direction, DomainError, EntityId, LinkNostrRequest, Page, PageParams, PostResponse,
    RegisterUserRequest, Relation, SystemEvent, UpdateProfileRequest, UserId, UserResponse,
    MAX_BIO_LEN, MAX_USERNAME_LEN,
};

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/users/register
// ---------------------------------------------------------------------------

pub async fn register_user(
    State(state): State<AppState>,
    Json(req): Json<RegisterUserRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), HttpError> {
    let name = req.username.trim();
    if name.is_empty() {
        return Err(DomainError::UsernameEmpty.into());
    }
    if name.len() > MAX_USERNAME_LEN {
        return Err(DomainError::UsernameTooLong {
            len: name.len(),
            max: MAX_USERNAME_LEN,
        }
        .into());
    }

    if let Some(ref bio) = req.bio {
        if bio.len() > MAX_BIO_LEN {
            return Err(DomainError::BioTooLong {
                len: bio.len(),
                max: MAX_BIO_LEN,
            }
            .into());
        }
    }

    let created = state.auth.register(name, &req.password).await?;

    // If bio or email were provided, update the profile
    if req.bio.is_some() || req.email.is_some() {
        state
            .users
            .update_profile(&created.id, req.bio, req.email)
            .await?;
    }

    let access_token = state.auth.create_token(&created.id.0)?;
    let refresh_token = state.auth.create_refresh_token(&created.id.0).await?;

    let _ = state
        .event
        .emit(SystemEvent::UserRegistered {
            user_id: created.id.0.clone(),
            username: created.username.clone(),
        })
        .await;

    // Re-fetch user to get updated bio/email
    let updated = state.users.get_by_id(&created.id).await?.unwrap_or(created);

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "user": UserResponse::from(&updated),
            "access_token": access_token,
            "refresh_token": refresh_token,
        })),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/users/:id
// ---------------------------------------------------------------------------

pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<UserResponse>, HttpError> {
    let uid = UserId(user_id);
    let user = state
        .users
        .get_by_id(&uid)
        .await?
        .ok_or_else(|| HttpError::NotFound("user not found".into()))?;
    let avatar_url = state.resolve_avatar_url(&user).await;
    let mut resp = UserResponse::from(&user);
    resp.avatar_url = avatar_url;
    Ok(Json(resp))
}

// ---------------------------------------------------------------------------
// GET /api/users/:id/posts
// ---------------------------------------------------------------------------

pub async fn get_user_posts(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
    Query(params): Query<PageParams>,
) -> Result<Json<Page<PostResponse>>, HttpError> {
    let uid = UserId(user_id);

    let profile = state
        .users
        .get_by_id(&uid)
        .await?
        .ok_or_else(|| HttpError::NotFound("user not found".into()))?;

    let page = state.posts.get_posts(&params, Some(&uid)).await?;

    let avatar_url = state.resolve_avatar_url(&profile).await;
    let mut items = Vec::with_capacity(page.items.len());
    for post in &page.items {
        let media = if let Some(ref store) = state.media_local {
            store
                .resolve_batch(&post.media_ids)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };
        items.push(PostResponse::from_post(
            post,
            profile.username.clone(),
            avatar_url.clone(),
            media,
        ));
    }

    Ok(Json(Page {
        items,
        total: page.total,
        page_size: params.page_size(),
        first: page.first,
        last: page.last,
    }))
}

// ---------------------------------------------------------------------------
// PUT /api/users/me  (authenticated — update profile)
// ---------------------------------------------------------------------------

pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<UserResponse>, HttpError> {
    if let Some(ref bio) = req.bio {
        if bio.len() > MAX_BIO_LEN {
            return Err(DomainError::BioTooLong {
                len: bio.len(),
                max: MAX_BIO_LEN,
            }
            .into());
        }
    }

    let updated = state
        .users
        .update_profile(&auth.0, req.bio, req.email)
        .await?;

    let avatar_url = state.resolve_avatar_url(&updated).await;
    let mut resp = UserResponse::from(&updated);
    resp.avatar_url = avatar_url;
    Ok(Json(resp))
}

// ---------------------------------------------------------------------------
// POST /api/users/me/nostr  (authenticated — link a Nostr pubkey)
// ---------------------------------------------------------------------------

pub async fn link_nostr(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<LinkNostrRequest>,
) -> Result<(StatusCode, Json<UserResponse>), HttpError> {
    state
        .auth
        .verify_challenge(&req.challenge_id, &req.public_key, &req.signature)
        .await?;

    state
        .users
        .set_nostr_pubkey(&auth.0, req.public_key)
        .await?;

    let user = state
        .users
        .get_by_id(&auth.0)
        .await?
        .ok_or_else(|| HttpError::NotFound("user not found".into()))?;

    let avatar_url = state.resolve_avatar_url(&user).await;
    let mut resp = UserResponse::from(&user);
    resp.avatar_url = avatar_url;
    Ok((StatusCode::OK, Json(resp)))
}

// ---------------------------------------------------------------------------
// POST /api/users/me/avatar  (authenticated — upload profile picture)
// ---------------------------------------------------------------------------

pub async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<UserResponse>, HttpError> {
    // Read user for existing avatar clean-up
    let current_user = state
        .users
        .get_by_id(&auth.0)
        .await?
        .ok_or_else(|| HttpError::NotFound("user not found".into()))?;

    // Read first file from multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| HttpError::BadRequest(e.to_string()))?
        .ok_or_else(|| HttpError::BadRequest("no file in request".into()))?;

    let content_type = field.content_type().unwrap_or("image/jpeg").to_string();

    let data = field
        .bytes()
        .await
        .map_err(|e| HttpError::BadRequest(e.to_string()))?
        .to_vec();

    // Validate against avatar policy
    if let Some(ref policy) = state.media_local_policy {
        // Content type check
        if !policy
            .avatar
            .allowed_types
            .iter()
            .any(|t| t == &content_type)
        {
            return Err(HttpError::BadRequest(format!(
                "content type not allowed for avatar: {content_type}"
            )));
        }

        // File size check
        if data.len() as u64 > policy.avatar.max_file_size {
            return Err(HttpError::BadRequest(format!(
                "avatar too large: {} bytes (max {})",
                data.len(),
                policy.avatar.max_file_size
            )));
        }

        // Per-user quota check
        let used = state.users.get_space_used(&auth.0).await?;
        let new_size = data.len() as u64;
        if used + new_size > policy.max_space_per_user {
            return Err(HttpError::PayloadTooLarge(format!(
                "storage quota exceeded: {used} used + {new_size} new > {} max",
                policy.max_space_per_user
            )));
        }
    }

    // Upload to local media store
    let store = state
        .media_local
        .as_ref()
        .ok_or_else(|| HttpError::Internal("local media backend not enabled".into()))?;

    let media = store.upload(&auth.0, data, "avatar", &content_type).await?;

    // Set avatar on user
    state
        .users
        .set_avatar(&auth.0, Some(media.id.clone()))
        .await?;

    // Soft-delete previous avatar media if it had one
    if let Some(ref old_id) = current_user.avatar_media_id {
        if old_id != &media.id {
            let _ = store.delete(&auth.0, old_id).await;
        }
    }

    // Return updated user with avatar_url
    let user = state
        .users
        .get_by_id(&auth.0)
        .await?
        .ok_or_else(|| HttpError::NotFound("user not found".into()))?;

    let avatar_url = state.resolve_avatar_url(&user).await;
    let mut resp = UserResponse::from(&user);
    resp.avatar_url = avatar_url;
    Ok(Json(resp))
}

// ---------------------------------------------------------------------------
// GET /api/users/:id/followers  (auth-gated, public data)
// ---------------------------------------------------------------------------

pub async fn get_user_followers(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let ids = state
        .graph
        .related(
            &EntityId::User(UserId(user_id.clone())),
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
// GET /api/users/:id/following  (auth-gated, public data)
// ---------------------------------------------------------------------------

pub async fn get_user_following(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let ids = state
        .graph
        .related(
            &EntityId::User(UserId(user_id)),
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
