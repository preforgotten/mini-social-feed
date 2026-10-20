use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use mini_social_feed_app::AppError;

// ---------------------------------------------------------------------------
// Inner
// ---------------------------------------------------------------------------

struct Entry {
    bytes: [u8; 32],
    public_key: String,
    expires_at: i64,
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ChallengeStore {
    inner: Arc<RwLock<HashMap<String, Entry>>>,
}

impl ChallengeStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, id: String, public_key: String, bytes: [u8; 32]) {
        let expires_at = chrono::Utc::now().timestamp() + 300; // 5 min
        let mut inner = self.inner.write().await;
        inner.insert(
            id,
            Entry {
                bytes,
                public_key,
                expires_at,
            },
        );
    }

    /// Consume and verify a challenge. Returns the challenge bytes on success.
    pub async fn take(&self, id: &str, expected_pubkey: &str) -> Result<[u8; 32], AppError> {
        let mut inner = self.inner.write().await;

        // Evict expired
        let now = chrono::Utc::now().timestamp();
        inner.retain(|_, e| e.expires_at > now);

        let entry = inner.remove(id).ok_or(AppError::BadRequest(
            "challenge expired or not found".into(),
        ))?;

        if entry.public_key != expected_pubkey {
            return Err(AppError::BadRequest("public key mismatch".into()));
        }

        Ok(entry.bytes)
    }
}

impl Default for ChallengeStore {
    fn default() -> Self {
        Self::new()
    }
}
