use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use mini_social_feed_domain::{
    Cursor, Direction, EntityId, MediaRef, Page, PageParams, Post, PostId, PostSearchQuery,
    Relation, User, UserId, UserSearchQuery,
};

use crate::error::DataError;
use crate::ports::{GraphStore, MediaStore, PostStore, UserStore};

// ---------------------------------------------------------------------------
// Inner state
// ---------------------------------------------------------------------------

pub(crate) struct Inner {
    pub(crate) users: HashMap<UserId, User>,
    pub(crate) users_by_username: HashMap<String, UserId>,
    pub(crate) users_by_nostr_pubkey: HashMap<String, UserId>,
    pub(crate) api_key_hash_to_user: HashMap<String, UserId>,
    pub(crate) posts: HashMap<PostId, Post>,
    /// parent_id → ordered list of direct-child PostIds (newest first).
    pub(crate) children: HashMap<PostId, Vec<PostId>>,
    /// Global feed order (newest first).
    pub(crate) feed_order: Vec<PostId>,
    /// Relationship edges: (from, to, kind) set.
    pub(crate) edges: HashSet<(EntityId, EntityId, Relation)>,
    /// Index: (from_entity, kind) → [to_entities] for outbound queries.
    pub(crate) outbound: HashMap<(EntityId, Relation), Vec<EntityId>>,
    /// Index: (to_entity, kind) → [from_entities] for inbound queries.
    pub(crate) inbound: HashMap<(EntityId, Relation), Vec<EntityId>>,
    /// In-memory media: id → (bytes, content_type, deleted).
    pub(crate) media: HashMap<String, (Vec<u8>, String, bool)>,
    /// media owner tracking: media_id → user_id.
    pub(crate) media_owners: HashMap<String, UserId>,
    /// per-user total bytes of non-deleted media.
    pub(crate) user_media_bytes: HashMap<UserId, u64>,
    /// avatar media id per user (None = no avatar).
    pub(crate) avatar_ids: HashMap<UserId, Option<String>>,
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MemoryStore {
    pub(crate) inner: Arc<RwLock<Inner>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                users: HashMap::new(),
                users_by_username: HashMap::new(),
                users_by_nostr_pubkey: HashMap::new(),
                api_key_hash_to_user: HashMap::new(),
                posts: HashMap::new(),
                children: HashMap::new(),
                feed_order: Vec::new(),
                edges: HashSet::new(),
                outbound: HashMap::new(),
                inbound: HashMap::new(),
                media: HashMap::new(),
                media_owners: HashMap::new(),
                user_media_bytes: HashMap::new(),
                avatar_ids: HashMap::new(),
            })),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// UserStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl UserStore for MemoryStore {
    async fn create(&self, user: User) -> Result<User, DataError> {
        let mut inner = self.inner.write().await;

        if inner.users_by_username.contains_key(&user.username) {
            return Err(DataError::Duplicate("username taken".into()));
        }

        // Ensure new users start with no avatar and default rate limit tier.
        let mut user = user;
        user.avatar_media_id = None;
        if user.rate_limit_tier.is_none() {
            user.rate_limit_tier = None; // stays None → uses profile default
        }

        // Store user as-is — password_hash is already pre-hashed.
        inner
            .users_by_username
            .insert(user.username.clone(), user.id.clone());
        inner.avatar_ids.insert(user.id.clone(), None);
        inner.users.insert(user.id.clone(), user.clone());
        Ok(user)
    }

    async fn get_by_id(&self, id: &UserId) -> Result<Option<User>, DataError> {
        Ok(self.inner.read().await.users.get(id).cloned())
    }

    async fn get_by_username(&self, name: &str) -> Result<Option<User>, DataError> {
        let inner = self.inner.read().await;
        match inner.users_by_username.get(name) {
            Some(uid) => Ok(inner.users.get(uid).cloned()),
            None => Ok(None),
        }
    }

    async fn get_by_api_key_hash(&self, hash: &str) -> Result<Option<UserId>, DataError> {
        let inner = self.inner.read().await;
        Ok(inner.api_key_hash_to_user.get(hash).cloned())
    }

    async fn set_api_key_hash(&self, user_id: &UserId, hash: String) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;
        let user = inner
            .users
            .get_mut(user_id)
            .ok_or_else(|| DataError::NotFound("user not found".into()))?;
        user.api_key_hash = Some(hash.clone());
        inner.api_key_hash_to_user.insert(hash, user_id.clone());
        Ok(())
    }

    async fn set_nostr_pubkey(&self, user_id: &UserId, pubkey: String) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;

        // Check pubkey isn't already claimed by a different user
        if let Some(existing) = inner.users_by_nostr_pubkey.get(&pubkey) {
            if existing != user_id {
                return Err(DataError::Duplicate(
                    "nostr pubkey already linked to another user".into(),
                ));
            }
            return Ok(()); // already linked to this user — no-op
        }

        let user = inner
            .users
            .get_mut(user_id)
            .ok_or_else(|| DataError::NotFound("user not found".into()))?;

        user.nostr_pubkey = Some(pubkey.clone());
        inner.users_by_nostr_pubkey.insert(pubkey, user_id.clone());
        Ok(())
    }

    async fn set_avatar(
        &self,
        user_id: &UserId,
        media_id: Option<String>,
    ) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;
        let user = inner
            .users
            .get_mut(user_id)
            .ok_or_else(|| DataError::NotFound("user not found".into()))?;
        user.avatar_media_id = media_id.clone();
        inner.avatar_ids.insert(user_id.clone(), media_id);
        Ok(())
    }

    async fn update_profile(
        &self,
        user_id: &UserId,
        bio: Option<String>,
        email: Option<String>,
    ) -> Result<User, DataError> {
        let mut inner = self.inner.write().await;
        let user = inner
            .users
            .get_mut(user_id)
            .ok_or_else(|| DataError::NotFound("user not found".into()))?;

        if let Some(b) = bio {
            user.bio = Some(b);
        }
        if let Some(e) = email {
            user.email = Some(e);
        }
        Ok(user.clone())
    }

    async fn search(&self, query: &UserSearchQuery) -> Result<Vec<User>, DataError> {
        let inner = self.inner.read().await;

        // Today: only exact username match.
        if let Some(ref username) = query.username {
            if let Some(uid) = inner.users_by_username.get(username) {
                if let Some(user) = inner.users.get(uid) {
                    return Ok(vec![user.clone()]);
                }
            }
            return Ok(vec![]);
        }

        // No query fields provided — return empty (future: with pagination, return all).
        Ok(vec![])
    }

    async fn get_space_used(&self, user_id: &UserId) -> Result<u64, DataError> {
        let inner = self.inner.read().await;
        Ok(inner.user_media_bytes.get(user_id).copied().unwrap_or(0))
    }
}

// ---------------------------------------------------------------------------
// PostStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl PostStore for MemoryStore {
    async fn create_post(&self, post: Post) -> Result<Post, DataError> {
        let mut inner = self.inner.write().await;

        let post_id = post.id.clone();
        let parent_id = post.parent_id.clone();

        inner.posts.insert(post_id.clone(), post.clone());
        inner.feed_order.insert(0, post_id.clone());

        if let Some(ref pid) = parent_id {
            inner
                .children
                .entry(pid.clone())
                .or_default()
                .insert(0, post_id);
        }

        Ok(post)
    }

    async fn get_post(&self, id: &PostId) -> Result<Option<Post>, DataError> {
        Ok(self.inner.read().await.posts.get(id).cloned())
    }

    async fn get_user_posts(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Post>, usize), DataError> {
        let inner = self.inner.read().await;
        let user_ids: Vec<PostId> = inner
            .feed_order
            .iter()
            .filter(|id| {
                inner
                    .posts
                    .get(id)
                    .map(|p| &p.author_id == user_id)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        let total = user_ids.len();
        let posts: Vec<Post> = user_ids
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|id| inner.posts.get(id).cloned())
            .collect();
        Ok((posts, total))
    }

    async fn get_posts(
        &self,
        params: &PageParams,
        author_id: Option<&UserId>,
    ) -> Result<Page<Post>, DataError> {
        let inner = self.inner.read().await;
        let page_size = params.page_size();

        // Build sorted list of matching posts (newest first)
        let matching: Vec<&Post> = inner
            .feed_order
            .iter()
            .filter_map(|id| inner.posts.get(id))
            .filter(|p| {
                if let Some(aid) = author_id {
                    if &p.author_id != aid {
                        return false;
                    }
                }
                if let Some(since) = params.since {
                    if p.created_at < since {
                        return false;
                    }
                }
                if let Some(until) = params.until {
                    if p.created_at > until {
                        return false;
                    }
                }
                true
            })
            .collect();

        let total = matching.len();

        // Apply cursor filtering
        let start_idx = if let Some(ref cursor) = params.before {
            // Items older than cursor → find cursor position, take items after it
            cursor
                .to_time_id()
                .and_then(|(ts, id)| {
                    matching
                        .iter()
                        .position(|p| p.created_at == ts && p.id.0 == id)
                })
                .map(|idx| idx + 1)
                .unwrap_or(0)
        } else if let Some(ref cursor) = params.after {
            // Items newer than cursor → take items before it, then reverse
            let end_idx = cursor
                .to_time_id()
                .and_then(|(ts, id)| {
                    matching
                        .iter()
                        .position(|p| p.created_at == ts && p.id.0 == id)
                })
                .unwrap_or(matching.len());
            // Take page_size items before cursor, then reverse for newest-first
            let slice_start = end_idx.saturating_sub(page_size);
            let slice: Vec<&Post> = matching[slice_start..end_idx]
                .iter()
                .rev()
                .copied()
                .collect();
            let first_cursor = slice
                .first()
                .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));
            let last_cursor = slice
                .last()
                .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));
            let items: Vec<Post> = slice.into_iter().cloned().collect();
            return Ok(Page {
                items,
                total,
                page_size,
                first: first_cursor,
                last: last_cursor,
            });
        } else {
            0
        };

        let end_idx = (start_idx + page_size).min(matching.len());
        let slice = &matching[start_idx..end_idx];

        let first_cursor = slice
            .first()
            .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));
        let last_cursor = slice
            .last()
            .map(|p| Cursor::from_time_id(p.created_at, &p.id.0));

        let items: Vec<Post> = slice.iter().map(|&p| p.clone()).collect();

        Ok(Page {
            items,
            total,
            page_size,
            first: first_cursor,
            last: last_cursor,
        })
    }

    async fn get_thread_posts(&self, root_id: &PostId) -> Result<Vec<Post>, DataError> {
        let inner = self.inner.read().await;
        let mut result = Vec::new();
        let mut queue: VecDeque<PostId> = VecDeque::new();
        queue.push_back(root_id.clone());

        while let Some(id) = queue.pop_front() {
            if let Some(post) = inner.posts.get(&id) {
                result.push(post.clone());
                if let Some(kids) = inner.children.get(&id) {
                    for kid_id in kids.iter() {
                        queue.push_back(kid_id.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    async fn search_posts(&self, query: &PostSearchQuery) -> Result<Vec<Post>, DataError> {
        let inner = self.inner.read().await;

        let limit = query.limit.unwrap_or(20).min(100);
        let offset = query.offset.unwrap_or(0);

        // Resolve author_username to author_id if provided
        let author_id_filter: Option<UserId> = query
            .author_username
            .as_ref()
            .and_then(|name| inner.users_by_username.get(name).cloned());

        let mut results: Vec<&Post> = inner
            .feed_order
            .iter()
            .filter_map(|id| inner.posts.get(id))
            .filter(|p| {
                if let Some(ref c) = query.content_contains {
                    if !p.content.contains(c.as_str()) {
                        return false;
                    }
                }
                if let Some(ref aid) = query.author_id {
                    if p.author_id.0 != *aid {
                        return false;
                    }
                }
                if let Some(ref aid_filter) = author_id_filter {
                    if p.author_id != *aid_filter {
                        return false;
                    }
                }
                if let Some(ref pid) = query.parent_id {
                    if p.parent_id.as_ref().map(|id| &id.0) != Some(pid) {
                        return false;
                    }
                }
                true
            })
            .collect();

        let total = results.len();
        let items = results
            .drain(offset..total.min(offset + limit))
            .cloned()
            .collect();

        Ok(items)
    }
}

// ---------------------------------------------------------------------------
// GraphStore impl
// ---------------------------------------------------------------------------

#[async_trait]
impl GraphStore for MemoryStore {
    async fn relate(
        &self,
        from: &EntityId,
        to: &EntityId,
        kind: Relation,
    ) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;
        let key = (from.clone(), to.clone(), kind.clone());
        if inner.edges.contains(&key) {
            return Ok(()); // idempotent
        }
        inner.edges.insert(key);

        // Update outbound index
        let out_key = (from.clone(), kind.clone());
        inner
            .outbound
            .entry(out_key)
            .or_default()
            .insert(0, to.clone());

        // Update inbound index
        let in_key = (to.clone(), kind);
        inner
            .inbound
            .entry(in_key)
            .or_default()
            .insert(0, from.clone());

        Ok(())
    }

    async fn unrelate(
        &self,
        from: &EntityId,
        to: &EntityId,
        kind: Relation,
    ) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;
        let key = (from.clone(), to.clone(), kind.clone());
        inner.edges.remove(&key);

        let out_key = (from.clone(), kind.clone());
        if let Some(vec) = inner.outbound.get_mut(&out_key) {
            vec.retain(|e| e != to);
        }

        let in_key = (to.clone(), kind);
        if let Some(vec) = inner.inbound.get_mut(&in_key) {
            vec.retain(|e| e != from);
        }

        Ok(())
    }

    async fn related(
        &self,
        entity: &EntityId,
        kind: Relation,
        dir: Direction,
    ) -> Result<Vec<EntityId>, DataError> {
        let inner = self.inner.read().await;
        let key = (entity.clone(), kind);
        let result = match dir {
            Direction::Outbound => inner.outbound.get(&key).cloned(),
            Direction::Inbound => inner.inbound.get(&key).cloned(),
        };
        Ok(result.unwrap_or_default())
    }

    async fn check_relation(
        &self,
        a: &EntityId,
        b: &EntityId,
        kind: Relation,
    ) -> Result<bool, DataError> {
        let inner = self.inner.read().await;
        Ok(inner.edges.contains(&(a.clone(), b.clone(), kind)))
    }
}

// ---------------------------------------------------------------------------
// MediaStore impl (in-memory)
// ---------------------------------------------------------------------------

#[async_trait]
impl MediaStore for MemoryStore {
    async fn upload(
        &self,
        user_id: &UserId,
        data: Vec<u8>,
        _filename: &str,
        content_type: &str,
    ) -> Result<MediaRef, DataError> {
        let mut inner = self.inner.write().await;
        let id = uuid::Uuid::new_v4().as_simple().to_string();
        let url = format!("/api/media/{}", id);

        let size = data.len() as u64;
        inner
            .media
            .insert(id.clone(), (data.clone(), content_type.to_string(), false));
        inner.media_owners.insert(id.clone(), user_id.clone());
        *inner.user_media_bytes.entry(user_id.clone()).or_insert(0) += size;

        Ok(MediaRef {
            id,
            url,
            backend: "memory".into(),
            content_type: content_type.to_string(),
            size_bytes: size,
            deleted: false,
        })
    }

    async fn delete(&self, user_id: &UserId, id: &str) -> Result<(), DataError> {
        let mut inner = self.inner.write().await;

        // Check ownership
        let owner = inner
            .media_owners
            .get(id)
            .cloned()
            .ok_or_else(|| DataError::NotFound("media not found".into()))?;
        if &owner != user_id {
            return Err(DataError::BadRequest("not the media owner".into()));
        }

        if let Some(entry) = inner.media.get_mut(id) {
            entry.2 = true; // mark deleted
            let size = entry.0.len() as u64;
            if let Some(bytes) = inner.user_media_bytes.get_mut(&owner) {
                *bytes = bytes.saturating_sub(size);
            }
        }
        Ok(())
    }

    async fn get_url(&self, id: &str) -> Result<Option<String>, DataError> {
        let inner = self.inner.read().await;
        match inner.media.get(id) {
            Some((_, _, deleted)) if !deleted => Ok(Some(format!("/api/media/{}", id))),
            _ => Ok(None),
        }
    }

    async fn resolve_batch(&self, ids: &[String]) -> Result<Vec<MediaRef>, DataError> {
        let inner = self.inner.read().await;
        let mut refs = Vec::new();
        for id in ids {
            if let Some((data, ct, deleted)) = inner.media.get(id) {
                refs.push(MediaRef {
                    id: id.clone(),
                    url: if *deleted {
                        String::new()
                    } else {
                        format!("/api/media/{}", id)
                    },
                    backend: "memory".into(),
                    content_type: ct.clone(),
                    size_bytes: data.len() as u64,
                    deleted: *deleted,
                });
            }
        }
        Ok(refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_social_feed_domain::{Direction, EntityId, Relation, User};
    use sha2::{Digest, Sha256};

    fn make_user(id: &str, name: &str) -> User {
        User {
            id: UserId(id.into()),
            username: name.into(),
            password_hash: "prehashed".into(),
            api_key_hash: None,
            bio: None,
            nostr_pubkey: None,
            email: None,
            avatar_media_id: None,
            rate_limit_tier: None,
            created_at: 0,
        }
    }

    fn make_post(id: &str, author_id: &str, content: &str, parent_id: Option<&str>) -> Post {
        Post {
            id: PostId(id.into()),
            author_id: UserId(author_id.into()),
            content: content.into(),
            parent_id: parent_id.map(|p| PostId(p.into())),
            media_ids: vec![],
            created_at: 100,
            nostr_event_id: None,
            nostr_sig: None,
        }
    }

    // ── UserStore tests ──

    #[tokio::test]
    async fn create_and_get_user() {
        let store = MemoryStore::new();
        let user = make_user("u1", "alice");
        let created = store.create(user).await.unwrap();
        assert_eq!(created.username, "alice");

        let found = store
            .get_by_id(&UserId("u1".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.username, "alice");

        let by_name = store.get_by_username("alice").await.unwrap().unwrap();
        assert_eq!(by_name.id.0, "u1");
    }

    #[tokio::test]
    async fn duplicate_username_rejected() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();
        let err = store.create(make_user("u2", "alice")).await.unwrap_err();
        assert!(matches!(err, DataError::Duplicate(_)));
    }

    #[tokio::test]
    async fn set_and_get_api_key_hash() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();

        let hash = hex::encode(Sha256::digest(b"test-key"));
        store
            .set_api_key_hash(&UserId("u1".into()), hash.clone())
            .await
            .unwrap();

        let found = store.get_by_api_key_hash(&hash).await.unwrap().unwrap();
        assert_eq!(found.0, "u1");

        let not_found = store
            .get_by_api_key_hash(&hex::encode(Sha256::digest(b"bad-key")))
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn set_nostr_pubkey() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();

        store
            .set_nostr_pubkey(&UserId("u1".into()), "npub1".into())
            .await
            .unwrap();
        let user = store
            .get_by_id(&UserId("u1".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.nostr_pubkey.as_deref(), Some("npub1"));
    }

    #[tokio::test]
    async fn set_nostr_duplicate_rejected() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();
        store.create(make_user("u2", "bob")).await.unwrap();

        store
            .set_nostr_pubkey(&UserId("u1".into()), "npub1".into())
            .await
            .unwrap();
        let err = store
            .set_nostr_pubkey(&UserId("u2".into()), "npub1".into())
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::Duplicate(_)));
    }

    #[tokio::test]
    async fn update_user_profile() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();

        let updated = store
            .update_profile(
                &UserId("u1".into()),
                Some("new bio".into()),
                Some("e@x.com".into()),
            )
            .await
            .unwrap();
        assert_eq!(updated.bio.as_deref(), Some("new bio"));
        assert_eq!(updated.email.as_deref(), Some("e@x.com"));
        assert_eq!(updated.username, "alice");
    }

    // ── PostStore tests ──

    #[tokio::test]
    async fn create_and_get_post() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();

        let post = make_post("p1", "u1", "hello", None);
        store.create_post(post).await.unwrap();

        let found = store.get_post(&PostId("p1".into())).await.unwrap().unwrap();
        assert_eq!(found.content, "hello");
    }

    #[tokio::test]
    async fn thread_returns_descendants() {
        let store = MemoryStore::new();
        store.create(make_user("u1", "alice")).await.unwrap();

        let root = make_post("r", "u1", "r", None);
        let c1 = make_post("c1", "u1", "c1", Some("r"));
        let c2 = make_post("c2", "u1", "c2", Some("c1"));
        store.create_post(root).await.unwrap();
        store.create_post(c1).await.unwrap();
        store.create_post(c2).await.unwrap();

        let thread = store.get_thread_posts(&PostId("r".into())).await.unwrap();
        assert_eq!(thread.len(), 3);
        assert_eq!(thread[0].id.0, "r");
        assert_eq!(thread[1].id.0, "c1");
        assert_eq!(thread[2].id.0, "c2");
    }

    // ── GraphStore tests ──

    #[tokio::test]
    async fn relate_and_query_reply_edges() {
        let store = MemoryStore::new();
        let parent = EntityId::Post(PostId("p1".into()));
        let child = EntityId::Post(PostId("c1".into()));

        store
            .relate(&child, &parent, Relation::Reply)
            .await
            .unwrap();

        // Outbound: child → parent (child relates to parent)
        let out = store
            .related(&child, Relation::Reply, Direction::Outbound)
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], parent);

        // Inbound: parent ← child
        let inbound = store
            .related(&parent, Relation::Reply, Direction::Inbound)
            .await
            .unwrap();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0], child);

        assert!(store
            .check_relation(&child, &parent, Relation::Reply)
            .await
            .unwrap());
        assert!(!store
            .check_relation(&parent, &child, Relation::Reply)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn relate_is_idempotent() {
        let store = MemoryStore::new();
        let a = EntityId::User(UserId("u1".into()));
        let b = EntityId::User(UserId("u2".into()));

        store.relate(&a, &b, Relation::Subscribe).await.unwrap();
        store.relate(&a, &b, Relation::Subscribe).await.unwrap();

        let subs = store
            .related(&a, Relation::Subscribe, Direction::Outbound)
            .await
            .unwrap();
        assert_eq!(subs.len(), 1, "duplicate relate should be a no-op");
    }

    #[tokio::test]
    async fn unrelate_removes_edge() {
        let store = MemoryStore::new();
        let a = EntityId::User(UserId("u1".into()));
        let b = EntityId::User(UserId("u2".into()));

        store.relate(&a, &b, Relation::Block).await.unwrap();
        assert!(store.check_relation(&a, &b, Relation::Block).await.unwrap());

        store.unrelate(&a, &b, Relation::Block).await.unwrap();
        assert!(!store.check_relation(&a, &b, Relation::Block).await.unwrap());
    }

    #[tokio::test]
    async fn related_empty_for_no_edges() {
        let store = MemoryStore::new();
        let entity = EntityId::Post(PostId("nobody".into()));
        let out = store
            .related(&entity, Relation::Reply, Direction::Outbound)
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    // ── MediaStore tests ──

    #[tokio::test]
    async fn upload_and_get_media() {
        let store = MemoryStore::new();
        let user_id = UserId("u1".into());

        let media_ref = store
            .upload(&user_id, b"hello world".to_vec(), "test.txt", "text/plain")
            .await
            .unwrap();

        assert_eq!(media_ref.content_type, "text/plain");
        assert_eq!(media_ref.size_bytes, 11);
        assert!(!media_ref.deleted);

        let url = store.get_url(&media_ref.id).await.unwrap().unwrap();
        assert!(url.contains(&media_ref.id));

        // Non-existent media
        let missing = store.get_url("nonexistent").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn delete_media() {
        let store = MemoryStore::new();
        let owner = UserId("u1".into());
        let other = UserId("u2".into());

        let media_ref = store
            .upload(&owner, b"data".to_vec(), "f.txt", "text/plain")
            .await
            .unwrap();

        // Wrong owner can't delete
        let err = store.delete(&other, &media_ref.id).await.unwrap_err();
        assert!(matches!(err, DataError::BadRequest(_)));

        // Owner can delete
        store.delete(&owner, &media_ref.id).await.unwrap();

        // Deleted media returns None
        let url = store.get_url(&media_ref.id).await.unwrap();
        assert!(url.is_none());
    }

    #[tokio::test]
    async fn resolve_batch() {
        let store = MemoryStore::new();
        let user_id = UserId("u1".into());

        let m1 = store
            .upload(&user_id, b"a".to_vec(), "a.txt", "text/plain")
            .await
            .unwrap();
        let m2 = store
            .upload(&user_id, b"bb".to_vec(), "b.txt", "text/plain")
            .await
            .unwrap();

        store.delete(&user_id, &m2.id).await.unwrap();

        let refs = store
            .resolve_batch(&[m1.id.clone(), m2.id.clone(), "missing".into()])
            .await
            .unwrap();

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].id, m1.id);
        assert!(!refs[0].deleted);
        assert_eq!(refs[1].id, m2.id);
        assert!(refs[1].deleted);
        assert!(refs[1].url.is_empty());
    }
}
