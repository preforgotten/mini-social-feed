use std::collections::HashMap;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use mini_social_feed_app::AppError;

// ---------------------------------------------------------------------------
// Hashing helper
// ---------------------------------------------------------------------------

fn hash_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

struct TokenEntry {
    user_id: String,
    family_id: String,
    order: u64,
    expires_at: i64,
}

struct FamilyState {
    last_consumed: u64,
    revoked: bool,
}

struct Inner {
    /// hashed_token → TokenEntry
    tokens: HashMap<String, TokenEntry>,
    families: HashMap<String, FamilyState>,
}

impl Default for RefreshStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RefreshStore {
    inner: Arc<RwLock<Inner>>,
}

impl RefreshStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                tokens: HashMap::new(),
                families: HashMap::new(),
            })),
        }
    }

    /// Store a new refresh token (hashed). Returns the raw token to the caller.
    pub async fn insert(
        &self,
        family_id: String,
        order: u64,
        user_id: String,
    ) -> Result<String, AppError> {
        let raw = uuid::Uuid::new_v4().to_string();
        let hashed = hash_token(&raw);
        let expires_at = chrono::Utc::now().timestamp() + 604800; // 7 days
        let mut inner = self.inner.write().await;
        inner.tokens.insert(
            hashed,
            TokenEntry {
                user_id,
                family_id,
                order,
                expires_at,
            },
        );
        Ok(raw)
    }

    /// Consume a raw refresh token (hashed before lookup). Detects stale-token
    /// attacks via family ordering.
    pub async fn take(&self, raw: &str) -> Result<String, AppError> {
        let hashed = hash_token(raw);
        let mut inner = self.inner.write().await;

        let now = chrono::Utc::now().timestamp();
        inner.tokens.retain(|_, e| e.expires_at > now);

        let entry = inner.tokens.remove(&hashed).ok_or(AppError::InvalidToken)?;

        let family = inner
            .families
            .entry(entry.family_id.clone())
            .or_insert(FamilyState {
                last_consumed: 0,
                revoked: false,
            });

        if family.revoked {
            inner.tokens.retain(|_, e| e.family_id != entry.family_id);
            return Err(AppError::TokenFamilyRevoked);
        }

        if entry.order < family.last_consumed {
            family.revoked = true;
            inner.tokens.retain(|_, e| e.family_id != entry.family_id);
            return Err(AppError::TokenFamilyRevoked);
        }

        family.last_consumed = entry.order;
        Ok(entry.user_id)
    }

    /// Revoke an entire token family by family_id.
    pub async fn revoke_family(&self, family_id: &str) {
        let mut inner = self.inner.write().await;
        if let Some(family) = inner.families.get_mut(family_id) {
            family.revoked = true;
        }
        inner.tokens.retain(|_, e| e.family_id != family_id);
    }
}
