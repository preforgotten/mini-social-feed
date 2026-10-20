use axum::{extract::State, Json};

use mini_social_feed_domain::{PostResponse, SearchPostsRequest, SearchUsersRequest, UserResponse};
use mini_social_feed_domain::{PostSearchQuery, UserSearchQuery};

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

pub async fn search_users(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<SearchUsersRequest>,
) -> Result<Json<Vec<UserResponse>>, HttpError> {
    let results = state
        .users
        .search(&UserSearchQuery {
            username: req.username,
        })
        .await?;
    let items: Vec<UserResponse> = results.iter().map(UserResponse::from).collect();
    Ok(Json(items))
}

pub async fn search_posts(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<SearchPostsRequest>,
) -> Result<Json<Vec<PostResponse>>, HttpError> {
    let results = state
        .posts
        .search_posts(&PostSearchQuery {
            content_contains: req.content_contains,
            author_id: req.author_id,
            author_username: req.author_username,
            parent_id: req.parent_id,
            limit: req.limit,
            offset: req.offset,
        })
        .await?;
    let mut items = Vec::with_capacity(results.len());
    for post in &results {
        let profile = state
            .users
            .get_by_id(&post.author_id)
            .await?
            .ok_or_else(|| HttpError::NotFound("author not found".into()))?;
        let media = if let Some(ref store) = state.media_local {
            store
                .resolve_batch(&post.media_ids)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };
        let author_avatar_url = state.resolve_avatar_url(&profile).await;
        items.push(PostResponse::from_post(
            post,
            profile.username,
            author_avatar_url,
            media,
        ));
    }
    Ok(Json(items))
}
