use std::sync::Arc;

use mini_social_feed_app::{AppError, AuthPort, EventPort, FeedPort};
use mini_social_feed_data::{
    Config, GraphStore, LocalMediaStore, MediaPolicy, MediaStore, MemoryStore, PostStore,
    SqliteStore, UserStore,
};
use mini_social_feed_domain::User;
use mini_social_feed_infra::{ChronologicalFeed, EventBus, JwtService};

use crate::middleware::HasAuthPort;

/// Top-level application state held by axum.
#[derive(Clone)]
pub struct AppState {
    pub users: Arc<dyn UserStore>,
    pub posts: Arc<dyn PostStore>,
    pub graph: Arc<dyn GraphStore>,
    pub media_local: Option<Arc<dyn MediaStore>>,
    pub media_cloud: Option<Arc<dyn MediaStore>>,
    pub media_local_policy: Option<MediaPolicy>,
    pub auth: Arc<dyn AuthPort>,
    pub feed: Arc<dyn FeedPort>,
    pub event: Arc<dyn EventPort>,
}

impl HasAuthPort for AppState {
    fn auth_port(&self) -> &dyn AuthPort {
        self.auth.as_ref()
    }
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        users: Arc<dyn UserStore>,
        posts: Arc<dyn PostStore>,
        graph: Arc<dyn GraphStore>,
        media_local: Option<Arc<dyn MediaStore>>,
        media_cloud: Option<Arc<dyn MediaStore>>,
        media_local_policy: Option<MediaPolicy>,
        auth: Arc<dyn AuthPort>,
        feed: Arc<dyn FeedPort>,
        event: Arc<dyn EventPort>,
    ) -> Self {
        Self {
            users,
            posts,
            graph,
            media_local,
            media_cloud,
            media_local_policy,
            auth,
            feed,
            event,
        }
    }

    /// Convenience: wire up the default in-memory stack.
    pub fn with_defaults(config: &Config, jwt_secret: String) -> Self {
        let store = Arc::new(MemoryStore::new());
        let users: Arc<dyn UserStore> = store.clone();
        let posts: Arc<dyn PostStore> = store.clone();
        let graph: Arc<dyn GraphStore> = store.clone();
        let auth = Arc::new(JwtService::new(jwt_secret, users.clone()));
        let feed = Arc::new(ChronologicalFeed::new(posts.clone()));
        let event = Arc::new(EventBus::new(1024));
        let (media_local, media_local_policy) = if config.media.local.enabled {
            let policy = MediaPolicy::named(&config.media.local.policy);
            (
                Some(Arc::new(LocalMediaStore::new(
                    config.media.local.path.clone().into(),
                    policy.clone(),
                )) as Arc<dyn MediaStore>),
                Some(policy),
            )
        } else {
            (None, None)
        };
        Self {
            users,
            posts,
            graph,
            media_local,
            media_cloud: None,
            media_local_policy,
            auth,
            feed,
            event,
        }
    }

    pub async fn with_sqlite(config: &Config, jwt_secret: String) -> Result<Self, AppError> {
        let conn_str = if config.database.sqlite_path == ":memory:" {
            ":memory:".to_string()
        } else {
            format!("sqlite:{}?mode=rwc", config.database.sqlite_path)
        };
        let store = SqliteStore::connect(&conn_str).await?;
        let store_arc = Arc::new(store);
        let users: Arc<dyn UserStore> = store_arc.clone();
        let posts: Arc<dyn PostStore> = store_arc.clone();
        let graph: Arc<dyn GraphStore> = store_arc.clone();
        let auth = Arc::new(JwtService::new(jwt_secret, users.clone()));
        let feed = Arc::new(ChronologicalFeed::new(posts.clone()));
        let event = Arc::new(EventBus::new(1024));
        let (media_local, media_local_policy) = if config.media.local.enabled {
            let policy = MediaPolicy::named(&config.media.local.policy);
            (
                Some(Arc::new(LocalMediaStore::new(
                    config.media.local.path.clone().into(),
                    policy.clone(),
                )) as Arc<dyn MediaStore>),
                Some(policy),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            users,
            posts,
            graph,
            media_local,
            media_cloud: None,
            media_local_policy,
            auth,
            feed,
            event,
        })
    }

    /// Resolve a user's avatar URL from their avatar_media_id.
    pub async fn resolve_avatar_url(&self, user: &User) -> Option<String> {
        let media_id = user.avatar_media_id.as_ref()?;
        if let Some(ref store) = self.media_local {
            if let Ok(Some(url)) = store.get_url(media_id).await {
                return Some(url);
            }
        }
        if let Some(ref store) = self.media_cloud {
            if let Ok(Some(url)) = store.get_url(media_id).await {
                return Some(url);
            }
        }
        None
    }
}
