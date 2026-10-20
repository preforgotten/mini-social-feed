use serde::{Deserialize, Serialize};

/// Maximum post content size in bytes (16 KB).
pub const MAX_CONTENT_SIZE: usize = 16 * 1024;

/// Maximum username length.
pub const MAX_USERNAME_LEN: usize = 64;

/// Maximum bio length.
pub const MAX_BIO_LEN: usize = 512;

// ---------------------------------------------------------------------------
// Domain identifiers
// ---------------------------------------------------------------------------

/// User identifier — a UUIDv7 (time-sortable, 32 hex chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub String);

impl UserId {
    pub fn new() -> Self {
        UserId(uuid::Uuid::now_v7().as_simple().to_string())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

/// Post identifier — a UUIDv7 (time-sortable, 32 hex chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostId(pub String);

impl PostId {
    pub fn new() -> Self {
        PostId(uuid::Uuid::now_v7().as_simple().to_string())
    }
}

impl Default for PostId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Domain entities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub password_hash: String,
    pub api_key_hash: Option<String>,
    pub bio: Option<String>,
    pub nostr_pubkey: Option<String>,
    pub email: Option<String>,
    pub avatar_media_id: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: PostId,
    pub author_id: UserId,
    pub content: String,
    pub parent_id: Option<PostId>,
    pub media_ids: Vec<String>,
    pub created_at: i64,
    pub nostr_event_id: Option<String>,
    pub nostr_sig: Option<String>,
}

// ---------------------------------------------------------------------------
// Graph / relationship types
// ---------------------------------------------------------------------------

/// A node in the entity graph — users, posts, and future entities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityId {
    User(UserId),
    Post(PostId),
}

/// A labeled, directed edge between two entities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Relation {
    /// Post → Post: thread parent-child.
    Reply,
    /// User → User: unidirectional subscription (no consent required).
    Subscribe,
    /// User → User: pending bidirectional connection request.
    ConnectRequest,
    /// User → User: accepted bidirectional connection.
    Connected,
    /// User → User: explicit block.
    Block,
    /// Post → User: tagged / mentioned.
    Mention,
}

/// Traversal direction for graph queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    Outbound,
    Inbound,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterUserRequest {
    pub username: String,
    pub password: String,
    pub bio: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub content: String,
    pub parent_id: Option<String>,
    pub media_ids: Option<Vec<String>>,
    pub nostr_event_id: Option<String>,
    pub nostr_sig: Option<String>,
    pub nostr_ts: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AuthVerifyRequest {
    pub challenge_id: String,
    pub signature: String,
    pub public_key: String,
}

#[derive(Debug, Deserialize)]
pub struct LinkNostrRequest {
    pub challenge_id: String,
    pub signature: String,
    pub public_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchUsersRequest {
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchPostsRequest {
    pub content_contains: Option<String>,
    pub author_id: Option<String>,
    pub author_username: Option<String>,
    pub parent_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub bio: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub bio: Option<String>,
    pub nostr_pubkey: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub created_at: i64,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.0.clone(),
            username: u.username.clone(),
            bio: u.bio.clone(),
            nostr_pubkey: u.nostr_pubkey.clone(),
            email: u.email.clone(),
            avatar_url: None,
            created_at: u.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostResponse {
    pub id: String,
    pub author_id: String,
    pub author_username: String,
    pub author_avatar_url: Option<String>,
    pub content: String,
    pub parent_id: Option<String>,
    pub media: Vec<MediaRef>,
    pub created_at: i64,
    pub nostr_event_id: Option<String>,
    pub nostr_sig: Option<String>,
}

impl PostResponse {
    pub fn from_post(
        post: &Post,
        author_username: String,
        author_avatar_url: Option<String>,
        media: Vec<MediaRef>,
    ) -> Self {
        Self {
            id: post.id.0.clone(),
            author_id: post.author_id.0.clone(),
            author_username,
            author_avatar_url,
            content: post.content.clone(),
            parent_id: post.parent_id.as_ref().map(|pid| pid.0.clone()),
            media,
            created_at: post.created_at,
            nostr_event_id: post.nostr_event_id.clone(),
            nostr_sig: post.nostr_sig.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadResponse {
    pub post: PostResponse,
    pub replies: Vec<ThreadResponse>,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthChallengeResponse {
    pub challenge_id: String,
    pub challenge_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrVerifyResponse {
    pub verified: bool,
}

// ---------------------------------------------------------------------------
// Media types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaId(pub String);

impl MediaId {
    pub fn new() -> Self {
        MediaId(uuid::Uuid::now_v7().as_simple().to_string())
    }
}

impl Default for MediaId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRef {
    pub id: String,
    pub url: String,
    pub backend: String,
    pub content_type: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// Event types (pub/sub)
// ---------------------------------------------------------------------------

/// Lightweight domain event emitted by the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemEvent {
    #[serde(rename = "post.created")]
    PostCreated {
        post_id: String,
        author_id: String,
        created_at: i64,
    },
    #[serde(rename = "reply.received")]
    ReplyReceived {
        post_id: String,
        parent_id: String,
        author_id: String,
        created_at: i64,
    },
    #[serde(rename = "user.registered")]
    UserRegistered { user_id: String, username: String },
}
