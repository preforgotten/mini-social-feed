use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Cursor — opaque position token for pagination
// ---------------------------------------------------------------------------

/// An opaque cursor encoding a sort-key position (timestamp + id).
/// The client echoes it back as `before` or `after` to navigate a result set.
///
/// Encoded as base64url of (created_at as big-endian u64) || (id as raw bytes).
/// This means it's safe as a URL query parameter and doesn't leak the sort internals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(pub String);

impl Cursor {
    /// Create a cursor from a timestamp and UUID (both hex strings).
    /// The UUID is expected to be a UUIDv7 (32 hex chars).
    pub fn from_time_id(ts: i64, id: &str) -> Self {
        let mut buf = Vec::with_capacity(8 + id.len() / 2);
        buf.extend_from_slice(&ts.to_be_bytes());
        // Decode hex id to raw bytes
        let id_bytes = hex::decode(id).unwrap_or_default();
        buf.extend_from_slice(&id_bytes);
        Self(base64_url(&buf))
    }

    /// Decode a cursor back to (timestamp, id_hex).
    /// Returns None if the cursor is malformed.
    pub fn to_time_id(&self) -> Option<(i64, String)> {
        let raw = base64_url_decode(&self.0)?;
        if raw.len() < 8 + 16 {
            return None;
        }
        let ts = i64::from_be_bytes(raw[..8].try_into().ok()?);
        let id = hex::encode(&raw[8..]);
        Some((ts, id))
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// PageParams — general-purpose pagination parameters
// ---------------------------------------------------------------------------

/// Cursor-based pagination parameters. Works for any data type.
///
/// Query string: `?page_size=30&before=abc123&since=1700000000`
#[derive(Debug, Clone, Deserialize)]
pub struct PageParams {
    /// Fetch items after this cursor ("load newer than this").
    pub after: Option<Cursor>,

    /// Fetch items before this cursor ("load older than this" / next page).
    /// This is the primary pagination direction (oldest-first in reverse-chronological feeds).
    pub before: Option<Cursor>,

    /// Number of items per page. Default 20, capped at 100.
    /// Also accepts `limit` as an alias (REST convention).
    #[serde(alias = "limit")]
    pub page_size: Option<usize>,

    /// Optional time window — return only items created at or after this Unix timestamp.
    pub since: Option<i64>,

    /// Optional time window — return only items created at or before this Unix timestamp.
    pub until: Option<i64>,
}

impl PageParams {
    /// Resolve the effective page size with defaults and bounds.
    pub fn page_size(&self) -> usize {
        self.page_size.unwrap_or(20).min(100)
    }
}

// ---------------------------------------------------------------------------
// Page<T> — generic paginated response envelope
// ---------------------------------------------------------------------------

/// A page of results with cursor navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// The items in this page.
    pub items: Vec<T>,

    /// Total number of items matching the query (may be approximate).
    pub total: usize,

    /// The effective page size used for this page.
    pub page_size: usize,

    /// Cursor of the first item in this page. `Some` if page is non-empty.
    /// Client uses this as `after` to fetch newer items.
    pub first: Option<Cursor>,

    /// Cursor of the last item in this page. `Some` if page is non-empty.
    /// Client uses this as `before` to fetch the next (older) page.
    pub last: Option<Cursor>,
}

impl<T> Page<T> {
    /// Create an empty page.
    pub fn empty(page_size: usize) -> Self {
        Self {
            items: vec![],
            total: 0,
            page_size,
            first: None,
            last: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Query objects for search
// ---------------------------------------------------------------------------

/// Query object for user search — designed to grow with richer filters over
/// time without breaking the HTTP contract.
#[derive(Debug, Clone, Deserialize)]
pub struct UserSearchQuery {
    /// Exact username match.
    pub username: Option<String>,
    // Future fields (all optional, additive):
    // pub username_contains: Option<String>,
    // pub bio_contains: Option<String>,
    // pub has_nostr: Option<bool>,
    // pub created_after: Option<i64>,
}

/// Query object for post search.
#[derive(Debug, Clone, Deserialize)]
pub struct PostSearchQuery {
    /// Substring match against post content.
    pub content_contains: Option<String>,
    /// Filter to posts by this user id (UUID).
    pub author_id: Option<String>,
    /// Filter to posts by a user whose username matches exactly.
    pub author_username: Option<String>,
    /// Filter to replies under this parent post id.
    pub parent_id: Option<String>,
    /// Pagination — limit for the number of results. Capped at 100.
    pub limit: Option<usize>,
    /// Pagination offset. Deprecated — prefer cursor-based PageParams on list endpoints.
    pub offset: Option<usize>,
    // Future: pub created_after, pub created_before, pub has_signature, …
}

// ---------------------------------------------------------------------------
// Cursor encoding helpers (internal)
// ---------------------------------------------------------------------------

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn base64_url_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .ok()
}
