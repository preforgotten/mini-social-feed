use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mini_social_feed_domain::{
    CreatePostRequest, DomainError, EntityId, Page, PageParams, Post, PostId, PostResponse,
    Relation, SystemEvent, ThreadResponse, UserId, MAX_CONTENT_SIZE,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/posts
// ---------------------------------------------------------------------------

pub async fn create_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<PostResponse>), HttpError> {
    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(DomainError::ContentTooLarge {
            size: req.content.len(),
            max: MAX_CONTENT_SIZE,
        }
        .into());
    }

    // If Nostr fields are present, verify them
    if let (Some(ref event_id), Some(ref sig), Some(ts)) =
        (&req.nostr_event_id, &req.nostr_sig, req.nostr_ts)
    {
        let user = state
            .users
            .get_by_id(&auth.0)
            .await?
            .ok_or_else(|| HttpError::NotFound("user not found".into()))?;

        let pubkey = user.nostr_pubkey.ok_or_else(|| {
            HttpError::BadRequest("cannot sign a nostr event without a linked nostr pubkey".into())
        })?;

        // Build Nostr tags from parent
        let tags: Vec<Vec<String>> = match &req.parent_id {
            Some(pid) => {
                let parent = state
                    .posts
                    .get_post(&PostId(pid.clone()))
                    .await?
                    .ok_or_else(|| HttpError::NotFound("parent post not found".into()))?;
                let parent_author = state
                    .users
                    .get_by_id(&parent.author_id)
                    .await?
                    .ok_or_else(|| HttpError::NotFound("parent author not found".into()))?;
                vec![
                    vec!["e".into(), pid.clone()],
                    vec!["p".into(), parent_author.nostr_pubkey.unwrap_or_default()],
                ]
            }
            None => vec![],
        };

        // Verify event ID
        let computed_id = compute_nostr_event_id(&pubkey, ts as u64, &tags, &req.content);
        if &computed_id != event_id {
            return Err(HttpError::BadRequest("nostr event id mismatch".into()));
        }

        // Verify Schnorr signature
        mini_social_feed_infra::schnorr_verify(
            &pubkey,
            hex::decode(event_id)
                .map_err(|_| HttpError::BadRequest("invalid event id hex".into()))?
                .as_slice(),
            sig,
        )?;
    }

    let post = Post {
        id: PostId::new(),
        author_id: auth.0,
        content: req.content,
        parent_id: req.parent_id.map(PostId),
        media_ids: req.media_ids.clone().unwrap_or_default(),
        created_at: chrono::Utc::now().timestamp(),
        nostr_event_id: req.nostr_event_id,
        nostr_sig: req.nostr_sig,
    };

    let created = state.posts.create_post(post).await?;

    // Populate graph relationship for reply threads
    if let Some(ref pid) = created.parent_id {
        state
            .graph
            .relate(
                &EntityId::Post(created.id.clone()),
                &EntityId::Post(pid.clone()),
                Relation::Reply,
            )
            .await?;

        let _ = state
            .event
            .emit(SystemEvent::ReplyReceived {
                post_id: created.id.0.clone(),
                parent_id: pid.0.clone(),
                author_id: created.author_id.0.clone(),
                created_at: created.created_at,
            })
            .await;
    } else {
        let _ = state
            .event
            .emit(SystemEvent::PostCreated {
                post_id: created.id.0.clone(),
                author_id: created.author_id.0.clone(),
                created_at: created.created_at,
            })
            .await;
    }

    let author_profile = state
        .users
        .get_by_id(&created.author_id)
        .await?
        .ok_or_else(|| HttpError::NotFound("author not found".into()))?;

    let media = if let Some(ref store) = state.media_local {
        store
            .resolve_batch(&created.media_ids)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };
    let author_avatar_url = state.resolve_avatar_url(&author_profile).await;

    Ok((
        StatusCode::CREATED,
        Json(PostResponse::from_post(
            &created,
            author_profile.username,
            author_avatar_url,
            media,
        )),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/posts — global feed
// ---------------------------------------------------------------------------

pub async fn get_feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<PageParams>,
) -> Result<Json<Page<PostResponse>>, HttpError> {
    let page = state.feed.get_feed(Some(&auth.0), &params).await?;

    let mut items = Vec::with_capacity(page.items.len());
    for post in &page.items {
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

    Ok(Json(Page {
        items,
        total: page.total,
        page_size: params.page_size(),
        first: page.first,
        last: page.last,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/posts/:id
// ---------------------------------------------------------------------------

pub async fn get_post(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(post_id): Path<String>,
) -> Result<Json<PostResponse>, HttpError> {
    let pid = PostId(post_id);
    let post = state
        .posts
        .get_post(&pid)
        .await?
        .ok_or_else(|| HttpError::NotFound("post not found".into()))?;
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
    Ok(Json(PostResponse::from_post(
        &post,
        profile.username,
        author_avatar_url,
        media,
    )))
}

// ---------------------------------------------------------------------------
// GET /api/posts/:id/thread
// ---------------------------------------------------------------------------

pub async fn get_thread(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(post_id): Path<String>,
) -> Result<Json<ThreadResponse>, HttpError> {
    let pid = PostId(post_id);
    let posts = state.posts.get_thread_posts(&pid).await?;

    if posts.is_empty() {
        return Err(HttpError::NotFound("post not found".into()));
    }

    let root = &posts[0];
    let descendants: Vec<&Post> = posts.iter().skip(1).collect();

    // Resolve usernames and avatar URLs for all authors in the thread
    let mut usernames: HashMap<UserId, String> = HashMap::new();
    let mut avatar_urls: HashMap<UserId, String> = HashMap::new();
    let mut media_map: HashMap<String, Vec<mini_social_feed_domain::MediaRef>> = HashMap::new();
    for post in &posts {
        if !usernames.contains_key(&post.author_id) {
            let profile = state
                .users
                .get_by_id(&post.author_id)
                .await?
                .ok_or_else(|| HttpError::NotFound("author not found".into()))?;
            usernames.insert(post.author_id.clone(), profile.username.clone());
            let avatar = state.resolve_avatar_url(&profile).await;
            if let Some(url) = avatar {
                avatar_urls.insert(post.author_id.clone(), url);
            }
        }
        // Resolve media for each post
        let media = if let Some(ref store) = state.media_local {
            store
                .resolve_batch(&post.media_ids)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };
        media_map.insert(post.id.0.clone(), media);
    }

    let thread = build_thread_tree(root, &descendants, &usernames, &avatar_urls, &media_map);

    Ok(Json(thread))
}

// ---------------------------------------------------------------------------
// Nostr helpers
// ---------------------------------------------------------------------------

/// Compute a Nostr event ID: SHA-256 of the serialized event tuple.
/// Event: [0, pubkey_hex, created_at, 1, tags, content]
fn compute_nostr_event_id(
    pubkey_hex: &str,
    created_at: u64,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let ev = json!([0, pubkey_hex, created_at, 1, tags, content]);
    let serialized = serde_json::to_string(&ev).unwrap();
    hex::encode(Sha256::digest(serialized.as_bytes()))
}

// ---------------------------------------------------------------------------
// Tree builder
// ---------------------------------------------------------------------------

fn build_thread_tree(
    root: &Post,
    all: &[&Post],
    usernames: &HashMap<UserId, String>,
    avatar_urls: &HashMap<UserId, String>,
    media_map: &HashMap<String, Vec<mini_social_feed_domain::MediaRef>>,
) -> ThreadResponse {
    let replies: Vec<ThreadResponse> = all
        .iter()
        .filter(|p| p.parent_id.as_ref() == Some(&root.id))
        .map(|child| build_thread_tree(child, all, usernames, avatar_urls, media_map))
        .collect();

    let author_username = usernames.get(&root.author_id).cloned().unwrap_or_default();
    let author_avatar_url = avatar_urls.get(&root.author_id).cloned();
    let media = media_map.get(&root.id.0).cloned().unwrap_or_default();

    ThreadResponse {
        post: PostResponse::from_post(root, author_username, author_avatar_url, media),
        replies,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mini_social_feed_domain::PostId;

    fn make_post(id: &str, parent_id: Option<&str>) -> Post {
        Post {
            id: PostId(id.into()),
            author_id: UserId("author".into()),
            content: String::new(),
            parent_id: parent_id.map(|s| PostId(s.into())),
            media_ids: vec![],
            created_at: 0,
            nostr_event_id: None,
            nostr_sig: None,
        }
    }

    fn make_usernames() -> HashMap<UserId, String> {
        let mut m = HashMap::new();
        m.insert(UserId("author".into()), "author_user".into());
        m
    }

    fn empty_maps() -> (
        HashMap<UserId, String>,
        HashMap<String, Vec<mini_social_feed_domain::MediaRef>>,
    ) {
        (HashMap::new(), HashMap::new())
    }

    #[test]
    fn single_post_has_no_replies() {
        let root = make_post("r", None);
        let usernames = make_usernames();
        let (avatar_urls, media_map) = empty_maps();
        let tree = build_thread_tree(&root, &[], &usernames, &avatar_urls, &media_map);
        assert_eq!(tree.post.id, "r");
        assert!(tree.replies.is_empty());
    }

    #[test]
    fn nested_replies_build_tree() {
        let p1 = make_post("1", None);
        let p2 = make_post("2", Some("1"));
        let p3 = make_post("3", Some("1"));
        let p4 = make_post("4", Some("2"));
        let all: Vec<&Post> = vec![&p2, &p3, &p4];
        let usernames = make_usernames();
        let (avatar_urls, media_map) = empty_maps();

        let tree = build_thread_tree(&p1, &all, &usernames, &avatar_urls, &media_map);
        assert_eq!(tree.replies.len(), 2);
        let c2 = &tree.replies[0];
        assert_eq!(c2.post.id, "2");
        assert_eq!(c2.replies.len(), 1);
        assert_eq!(c2.replies[0].post.id, "4");
        let c3 = &tree.replies[1];
        assert_eq!(c3.post.id, "3");
        assert!(c3.replies.is_empty());
    }
}
