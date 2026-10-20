use mini_social_feed_data::{
    Config, GraphStore, LocalMediaConfig, LocalMediaStore, MediaPolicy, MediaStore, PostStore,
    RateLimitConfig, S3MediaConfig, SqliteStore, UserStore,
};
use mini_social_feed_infra::RateLimiter;
use mini_social_feed_server::state::AppState;
use serde_json::json;

/// Build a test Config with sensible defaults for local/CI tests.
fn test_config() -> Config {
    Config {
        server: mini_social_feed_data::ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
        auth: mini_social_feed_data::AuthConfig {
            _jwt_secret_use_env_var: (),
        },
        database: mini_social_feed_data::DatabaseConfig {
            backend: "memory".into(),
            sqlite_path: ":memory:".into(),
        },
        media: mini_social_feed_data::MediaConfig {
            local: LocalMediaConfig {
                enabled: true,
                policy: "standard".into(),
                path: std::env::temp_dir()
                    .join("mini-social-feed-test-media")
                    .to_string_lossy()
                    .into(),
            },
            s3: S3MediaConfig::default(),
        },
        rate_limit: RateLimitConfig {
            enabled: false,
            ..RateLimitConfig::default()
        },
    }
}

/// Start the axum server on a random OS port, return the base URL.
///
/// Defaults to MemoryStore for fast local dev. Set `STORE=sqlite` to use
/// SqliteStore (used in CI).
pub async fn spawn_server() -> String {
    if std::env::var("STORE").unwrap_or_default() == "sqlite" {
        spawn_sqlite_server().await
    } else {
        spawn_memory_server().await
    }
}

pub async fn spawn_memory_server() -> String {
    use std::sync::Arc;

    let config = test_config();
    let state = AppState::with_defaults(&config, "conformance-test-secret".into());
    let rl = Arc::new(RateLimiter::new());
    let rl_config = Arc::new(config.rate_limit.clone());
    let router = mini_social_feed_server::create_router(state, rl, rl_config);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    format!("http://{}", addr)
}

/// Start server backed by SqliteStore (:memory:).
pub async fn spawn_sqlite_server() -> String {
    use mini_social_feed_app::{AuthPort, EventPort, FeedPort};
    use mini_social_feed_infra::{ChronologicalFeed, EventBus, JwtService};
    use std::sync::Arc;

    let store = Arc::new(SqliteStore::connect(":memory:").await.unwrap());
    let users: Arc<dyn UserStore> = store.clone();
    let posts: Arc<dyn PostStore> = store.clone();
    let graph: Arc<dyn GraphStore> = store.clone();
    let auth: Arc<dyn AuthPort> = Arc::new(JwtService::new(
        "conformance-test-secret".into(),
        users.clone(),
    ));
    let feed: Arc<dyn FeedPort> = Arc::new(ChronologicalFeed::new(posts.clone()));
    let event: Arc<dyn EventPort> = Arc::new(EventBus::new(64));
    let policy = MediaPolicy::named("standard");
    let media_local: Option<Arc<dyn MediaStore>> = Some(Arc::new(LocalMediaStore::new(
        std::env::temp_dir().join("mini-social-feed-test-media"),
        policy.clone(),
    )));

    let state = AppState::new(
        users,
        posts,
        graph,
        media_local,
        None,
        Some(policy),
        auth,
        feed,
        event,
    );
    let rl = Arc::new(RateLimiter::new());
    let rl_config = Arc::new(RateLimitConfig {
        enabled: false,
        ..RateLimitConfig::default()
    });
    let router = mini_social_feed_server::create_router(state, rl, rl_config);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://{}", addr)
}

/// Register a user and return the JWT token + user id.
pub async fn register_user(base: &str, client: &reqwest::Client, name: &str) -> (String, String) {
    let resp: serde_json::Value = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": name, "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let token = resp["access_token"].as_str().unwrap().to_string();
    let user_id = resp["user"]["id"].as_str().unwrap().to_string();
    (token, user_id)
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}
