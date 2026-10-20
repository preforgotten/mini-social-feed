use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use mini_social_feed_app::{AppError, AuthPort};
use mini_social_feed_data::UserStore;
use mini_social_feed_domain::{User, UserId};

use super::challenge::ChallengeStore;
use super::crypto;
use super::refresh::RefreshStore;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: usize,
    exp: usize,
}

struct Family {
    family_id: String,
    next_order: u64,
}

#[derive(Clone)]
pub struct JwtService {
    secret: String,
    access_expiration_secs: usize,
    pub challenge_store: ChallengeStore,
    refresh_store: RefreshStore,
    families: Arc<RwLock<HashMap<String, Family>>>,
    user_store: Arc<dyn UserStore>,
}

impl JwtService {
    pub fn new(secret: String, user_store: Arc<dyn UserStore>) -> Self {
        Self {
            secret,
            access_expiration_secs: 900,
            challenge_store: ChallengeStore::new(),
            refresh_store: RefreshStore::new(),
            families: Arc::new(RwLock::new(HashMap::new())),
            user_store,
        }
    }
}

#[async_trait]
impl AuthPort for JwtService {
    // ---------------------------------------------------------------------------
    // New: register, login, API keys
    // ---------------------------------------------------------------------------

    async fn register(&self, username: &str, password: &str) -> Result<User, AppError> {
        let password_hash = crypto::hash_password(password)?;
        let user = User {
            id: UserId::new(),
            username: username.to_string(),
            password_hash,
            api_key_hash: None,
            bio: None,
            nostr_pubkey: None,
            email: None,
            avatar_media_id: None,
            rate_limit_tier: None,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.user_store.create(user).await.map_err(AppError::from)
    }

    async fn login(&self, username: &str, password: &str) -> Result<User, AppError> {
        let user = self
            .user_store
            .get_by_username(username)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !crypto::verify_password(password, &user.password_hash)? {
            return Err(AppError::Unauthorized);
        }
        Ok(user)
    }

    async fn create_api_key(&self, user_id: &UserId) -> Result<String, AppError> {
        let raw_key = uuid::Uuid::new_v4().to_string();
        let hash = hex::encode(Sha256::digest(raw_key.as_bytes()));
        self.user_store
            .set_api_key_hash(user_id, hash)
            .await
            .map_err(AppError::from)?;
        Ok(raw_key)
    }

    async fn verify_api_key(&self, key: &str) -> Result<UserId, AppError> {
        let hash = hex::encode(Sha256::digest(key.as_bytes()));
        self.user_store
            .get_by_api_key_hash(&hash)
            .await?
            .ok_or(AppError::Unauthorized)
    }

    // ---------------------------------------------------------------------------
    // Access tokens (sync)
    // ---------------------------------------------------------------------------

    fn create_token(&self, user_id: &str) -> Result<String, AppError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .as_secs() as usize;

        let claims = Claims {
            sub: user_id.to_string(),
            iat: now,
            exp: now + self.access_expiration_secs,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| AppError::Internal(e.to_string()))
    }

    fn verify_token(&self, token: &str) -> Result<String, AppError> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AppError::InvalidToken)?;

        Ok(data.claims.sub)
    }

    // ---------------------------------------------------------------------------
    // Refresh tokens
    // ---------------------------------------------------------------------------

    async fn create_refresh_token(&self, user_id: &str) -> Result<String, AppError> {
        let mut families = self.families.write().await;

        let family = families
            .entry(user_id.to_string())
            .or_insert_with(|| Family {
                family_id: uuid::Uuid::new_v4().to_string(),
                next_order: 1,
            });

        let order = family.next_order;
        family.next_order += 1;

        self.refresh_store
            .insert(family.family_id.clone(), order, user_id.to_string())
            .await
    }

    async fn consume_refresh_token(&self, token: &str) -> Result<String, AppError> {
        self.refresh_store.take(token).await
    }

    async fn revoke_token_family(&self, user_id: &str) -> Result<(), AppError> {
        let families = self.families.read().await;
        if let Some(family) = families.get(user_id) {
            self.refresh_store.revoke_family(&family.family_id).await;
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Nostr challenges
    // ---------------------------------------------------------------------------

    async fn create_challenge(&self, public_key: &str) -> Result<(String, [u8; 32]), AppError> {
        let pk_bytes = hex::decode(public_key)
            .map_err(|_| AppError::BadRequest("invalid public key hex".into()))?;
        if pk_bytes.len() != 32 && pk_bytes.len() != 33 {
            return Err(AppError::BadRequest(
                "public key must be 32 or 33 bytes".into(),
            ));
        }

        let challenge = crypto::generate_challenge();
        let id = uuid::Uuid::new_v4().to_string();

        self.challenge_store
            .insert(id.clone(), public_key.to_string(), challenge)
            .await;

        Ok((id, challenge))
    }

    async fn verify_challenge(
        &self,
        challenge_id: &str,
        public_key: &str,
        signature_hex: &str,
    ) -> Result<(), AppError> {
        let challenge_bytes = self.challenge_store.take(challenge_id, public_key).await?;

        crypto::schnorr_verify(public_key, &challenge_bytes, signature_hex)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mini_social_feed_data::DataError;
    use mini_social_feed_domain::UserSearchQuery;

    /// Minimal stub UserStore for unit tests that don't exercise auth methods.
    struct StubUserStore;

    #[async_trait]
    impl UserStore for StubUserStore {
        async fn create(&self, _user: User) -> Result<User, DataError> {
            unimplemented!()
        }
        async fn get_by_id(&self, _id: &UserId) -> Result<Option<User>, DataError> {
            unimplemented!()
        }
        async fn get_by_username(&self, _name: &str) -> Result<Option<User>, DataError> {
            unimplemented!()
        }
        async fn get_by_api_key_hash(&self, _hash: &str) -> Result<Option<UserId>, DataError> {
            unimplemented!()
        }
        async fn set_api_key_hash(
            &self,
            _user_id: &UserId,
            _hash: String,
        ) -> Result<(), DataError> {
            unimplemented!()
        }
        async fn set_nostr_pubkey(
            &self,
            _user_id: &UserId,
            _pubkey: String,
        ) -> Result<(), DataError> {
            unimplemented!()
        }
        async fn set_avatar(
            &self,
            _user_id: &UserId,
            _media_id: Option<String>,
        ) -> Result<(), DataError> {
            unimplemented!()
        }
        async fn update_profile(
            &self,
            _user_id: &UserId,
            _bio: Option<String>,
            _email: Option<String>,
        ) -> Result<User, DataError> {
            unimplemented!()
        }
        async fn search(&self, _query: &UserSearchQuery) -> Result<Vec<User>, DataError> {
            unimplemented!()
        }
    }

    #[test]
    fn create_and_verify_access_token() {
        let svc = JwtService::new("test-secret".into(), Arc::new(StubUserStore));
        let token = svc.create_token("user-1").unwrap();
        let user_id = svc.verify_token(&token).unwrap();
        assert_eq!(user_id, "user-1");
    }

    #[test]
    fn invalid_token_rejected() {
        let svc = JwtService::new("test-secret".into(), Arc::new(StubUserStore));
        assert!(svc.verify_token("garbage").is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let svc_a = JwtService::new("secret-a".into(), Arc::new(StubUserStore));
        let svc_b = JwtService::new("secret-b".into(), Arc::new(StubUserStore));
        let token = svc_a.create_token("user-1").unwrap();
        assert!(svc_b.verify_token(&token).is_err());
    }
}
