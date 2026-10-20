use async_trait::async_trait;
use mini_social_feed_domain::{Page, PageParams, Post, SystemEvent, User, UserId};

use crate::error::AppError;

/// Authentication / token services.
///
/// This trait owns the full auth lifecycle: registration, login, API keys,
/// JWT access tokens, refresh tokens, and Nostr challenge verification.
/// Hashing and persistence are delegated to the data layer internals.
#[async_trait]
pub trait AuthPort: Send + Sync + 'static {
    /// Register a new user with username and password. Fails if username is taken.
    async fn register(&self, username: &str, password: &str) -> Result<User, AppError>;

    /// Authenticate with username + password. Returns the user on success.
    async fn login(&self, username: &str, password: &str) -> Result<User, AppError>;

    /// Generate an API key for programmatic access. Returns the raw key once.
    async fn create_api_key(&self, user_id: &UserId) -> Result<String, AppError>;

    /// Validate an API key, returning the user id on success.
    async fn verify_api_key(&self, key: &str) -> Result<UserId, AppError>;

    /// Issue a JWT for the given user id (15-min access token).
    fn create_token(&self, user_id: &str) -> Result<String, AppError>;

    /// Validate a JWT, returning the user id on success.
    fn verify_token(&self, token: &str) -> Result<String, AppError>;

    /// Issue a refresh token (opaque, stored server-side, 7-day TTL).
    async fn create_refresh_token(&self, user_id: &str) -> Result<String, AppError>;

    /// Consume a refresh token, returning the user_id and destroying the token.
    async fn consume_refresh_token(&self, token: &str) -> Result<String, AppError>;

    /// Revoke all refresh tokens for a user (logout).
    async fn revoke_token_family(&self, user_id: &str) -> Result<(), AppError>;

    /// Create a challenge for Nostr key-proof (returns id + 32-byte nonce).
    async fn create_challenge(&self, public_key: &str) -> Result<(String, [u8; 32]), AppError>;

    /// Verify a challenge signature. Consumes the challenge on success.
    async fn verify_challenge(
        &self,
        challenge_id: &str,
        pubkey: &str,
        sig: &str,
    ) -> Result<(), AppError>;
}

/// Feed generation — ranking, aggregation, filtering.
#[async_trait]
pub trait FeedPort: Send + Sync + 'static {
    /// Get a page of posts. `viewer` is the authenticated user (None for global).
    /// Returns a cursor-based page for client-driven pagination.
    async fn get_feed(
        &self,
        viewer: Option<&UserId>,
        params: &PageParams,
    ) -> Result<Page<Post>, AppError>;
}

/// Event bus — pub/sub for system events.
///
/// Communication modes (broadcast, pub/sub, direct) emit events through this
/// port. Agents and notification services consume them via polling or SSE.
#[async_trait]
pub trait EventPort: Send + Sync + 'static {
    /// Publish an event into the bus.
    async fn emit(&self, event: SystemEvent) -> Result<(), AppError>;

    /// Subscribe to all events, receiving new ones as they arrive.
    /// Returns a receiver handle that can be awaited or converted to a stream.
    async fn subscribe(&self) -> Result<EventReceiver, AppError>;
}

/// Handle for receiving events from the bus.
pub struct EventReceiver {
    rx: tokio::sync::broadcast::Receiver<SystemEvent>,
}

impl EventReceiver {
    pub fn new(rx: tokio::sync::broadcast::Receiver<SystemEvent>) -> Self {
        Self { rx }
    }

    pub async fn recv(&mut self) -> Result<SystemEvent, tokio::sync::broadcast::error::RecvError> {
        self.rx.recv().await
    }

    pub fn try_recv(&mut self) -> Result<SystemEvent, tokio::sync::broadcast::error::TryRecvError> {
        self.rx.try_recv()
    }

    /// Consume this receiver and return a Stream adapter.
    pub fn into_stream(self) -> tokio_stream::wrappers::BroadcastStream<SystemEvent> {
        tokio_stream::wrappers::BroadcastStream::new(self.rx)
    }
}
