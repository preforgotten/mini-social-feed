use async_trait::async_trait;
use mini_social_feed_domain::{
    Direction, EntityId, MediaRef, Page, PageParams, Post, PostId, PostSearchQuery, Relation, User,
    UserId, UserSearchQuery,
};

use crate::error::DataError;

// ---------------------------------------------------------------------------
// UserStore — user persistence (no auth, no hashing)
// ---------------------------------------------------------------------------

/// Store and retrieve user profiles. All passwords arrive pre-hashed.
/// Authentication (login, password verification) is NOT a data concern.
#[async_trait]
pub trait UserStore: Send + Sync + 'static {
    /// Create a user with a pre-hashed password. Fails if username is taken.
    async fn create(&self, user: User) -> Result<User, DataError>;

    /// Look up a user by their internal id.
    async fn get_by_id(&self, id: &UserId) -> Result<Option<User>, DataError>;

    /// Look up a user by username (unique).
    async fn get_by_username(&self, name: &str) -> Result<Option<User>, DataError>;

    /// Look up a user by their hashed API key.
    async fn get_by_api_key_hash(&self, hash: &str) -> Result<Option<UserId>, DataError>;

    /// Store an API key hash against the user. Returns the raw key.
    async fn set_api_key_hash(&self, user_id: &UserId, hash: String) -> Result<(), DataError>;

    /// Set or clear the user's avatar (media_id). Pass None to remove.
    async fn set_avatar(&self, user_id: &UserId, media_id: Option<String>)
        -> Result<(), DataError>;

    /// Link a Nostr pubkey to a user profile. Fails if pubkey is already
    /// linked to a different user.
    async fn set_nostr_pubkey(&self, user_id: &UserId, pubkey: String) -> Result<(), DataError>;

    /// Update mutable profile fields (bio, email). Returns the updated user.
    async fn update_profile(
        &self,
        user_id: &UserId,
        bio: Option<String>,
        email: Option<String>,
    ) -> Result<User, DataError>;

    /// Search users by query criteria.
    async fn search(&self, query: &UserSearchQuery) -> Result<Vec<User>, DataError>;

    /// Total bytes of non-deleted media stored for this user.
    /// Default returns 0 for backends that don't track space.
    async fn get_space_used(&self, _user_id: &UserId) -> Result<u64, DataError> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// PostStore — content persistence
// ---------------------------------------------------------------------------

/// Store and query post content.
#[async_trait]
pub trait PostStore: Send + Sync + 'static {
    async fn create_post(&self, post: Post) -> Result<Post, DataError>;

    async fn get_post(&self, id: &PostId) -> Result<Option<Post>, DataError>;

    /// Cursor-based page of posts, newest-first. This is the primary method
    /// for feed and user-post listings. Supports `before`/`after` cursors,
    /// `page_size`, and time-window filters.
    async fn get_posts(
        &self,
        params: &PageParams,
        author_id: Option<&UserId>,
    ) -> Result<Page<Post>, DataError> {
        // Default: fetch all, slice in memory. Backends should override.
        let _ = (params, author_id);
        Ok(Page::empty(params.page_size()))
    }

    /// Single user's posts, newest-first (legacy).
    /// New code should use `get_posts` with an `author_id` filter.
    async fn get_user_posts(
        &self,
        user_id: &UserId,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Post>, usize), DataError>;

    /// All posts in a thread (root + descendants, BFS order).
    async fn get_thread_posts(&self, root_id: &PostId) -> Result<Vec<Post>, DataError>;

    /// Search posts. The implementation translates the query into a result set.
    async fn search_posts(&self, query: &PostSearchQuery) -> Result<Vec<Post>, DataError>;
}

// ---------------------------------------------------------------------------
// GraphStore — entity relationship management
// ---------------------------------------------------------------------------

/// Entity-graph relationship management.
///
/// All entity relationships flow through this port — reply trees, subscriptions,
/// connections, blocks, and mentions.
#[async_trait]
pub trait GraphStore: Send + Sync + 'static {
    /// Create a directed edge: `from` relates to `to` with `kind`.
    /// Idempotent — calling twice with the same arguments is a no-op.
    async fn relate(&self, from: &EntityId, to: &EntityId, kind: Relation)
        -> Result<(), DataError>;

    /// Remove a directed edge.
    async fn unrelate(
        &self,
        from: &EntityId,
        to: &EntityId,
        kind: Relation,
    ) -> Result<(), DataError>;

    /// All entities related to `entity` by `kind` in the given direction.
    async fn related(
        &self,
        entity: &EntityId,
        kind: Relation,
        dir: Direction,
    ) -> Result<Vec<EntityId>, DataError>;

    /// Check if an edge exists.
    async fn check_relation(
        &self,
        a: &EntityId,
        b: &EntityId,
        kind: Relation,
    ) -> Result<bool, DataError>;
}

// ---------------------------------------------------------------------------
// MediaStore — binary content storage
// ---------------------------------------------------------------------------

/// Media storage — upload and serve binary content.
///
/// Implementations: LocalMediaStore (disk), S3MediaStore (cloud).
#[async_trait]
pub trait MediaStore: Send + Sync + 'static {
    /// Upload a file. Returns a MediaRef with the URL for serving.
    async fn upload(
        &self,
        user_id: &UserId,
        data: Vec<u8>,
        filename: &str,
        content_type: &str,
    ) -> Result<MediaRef, DataError>;

    /// Serve the actual file bytes and content type.
    async fn get_file(&self, id: &str) -> Result<Option<(Vec<u8>, String)>, DataError> {
        let _ = id;
        Ok(None)
    }

    /// Soft-delete a media file. Sets deleted flag; posts show tombstone.
    /// Only the owner (uploader) can delete.
    async fn delete(&self, user_id: &UserId, id: &str) -> Result<(), DataError>;

    /// Resolve a media reference to its display URL.
    async fn get_url(&self, id: &str) -> Result<Option<String>, DataError>;

    /// Batch-resolve media IDs to MediaRefs for a post.
    async fn resolve_batch(&self, ids: &[String]) -> Result<Vec<MediaRef>, DataError> {
        let mut refs = Vec::new();
        for id in ids {
            if let Some(url) = self.get_url(id).await? {
                refs.push(MediaRef {
                    id: id.to_string(),
                    url,
                    backend: String::new(),
                    content_type: String::new(),
                    size_bytes: 0,
                    deleted: false,
                });
            }
        }
        Ok(refs)
    }
}
