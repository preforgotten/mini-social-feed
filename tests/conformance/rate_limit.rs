use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

/// This test requires rate limiting to be enabled, so we spawn a server
/// with rate limiting active by building the state manually.
#[tokio::test]
async fn rate_limit_429_on_flood() {
    let base = spawn_rate_limited_server().await;
    let client = reqwest::Client::new();

    // Make rapid unauthenticated requests to a public endpoint
    let mut success = 0;
    let mut limited = 0;
    for _ in 0..30 {
        let resp = client
            .post(format!("{}/api/auth/login", base))
            .json(&json!({"username": "noexist", "password": "x"}))
            .send()
            .await
            .unwrap();
        if resp.status() == 200 || resp.status() == 401 {
            success += 1;
        } else if resp.status() == 429 {
            limited += 1;
        }
    }
    assert!(
        limited > 0,
        "expected at least one 429 after flooding; got {success} ok/401"
    );
}

#[tokio::test]
async fn rate_limit_returns_retry_after_header() {
    let base = spawn_rate_limited_server().await;
    let client = reqwest::Client::new();

    // Flood to trigger rate limit
    let mut limited_resp = None;
    for _ in 0..30 {
        let resp = client
            .post(format!("{}/api/auth/login", base))
            .json(&json!({"username": "noexist", "password": "x"}))
            .send()
            .await
            .unwrap();
        if resp.status() == 429 {
            limited_resp = Some(resp);
            break;
        }
    }

    let resp = limited_resp.expect("should have hit rate limit");
    assert!(
        resp.headers().get("retry-after").is_some(),
        "429 should include retry-after header"
    );
}

#[tokio::test]
async fn rate_limit_reads_not_blocked_by_writes() {
    let base = spawn_rate_limited_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "rl_isolation").await;
    let auth = auth_header(&token);

    // Flood post creation until we hit limit
    for _ in 0..20 {
        let _ = client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": "spam"}))
            .send()
            .await;
    }

    // Read should still work (separate action bucket)
    let feed = client
        .get(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert!(
        feed.status().is_success(),
        "GET feed should not be blocked by post_create rate limit"
    );
}

// ---------------------------------------------------------------------------
// Helper — spawn a server with rate limiting enabled
// ---------------------------------------------------------------------------

async fn spawn_rate_limited_server() -> String {
    use std::sync::Arc;

    use mini_social_feed_app::{AuthPort, EventPort, FeedPort};
    use mini_social_feed_data::{
        GraphStore, LocalMediaStore, MediaPolicy, MediaStore, MemoryStore, PostStore,
        RateLimitConfig, UserStore,
    };
    use mini_social_feed_infra::{ChronologicalFeed, EventBus, JwtService, RateLimiter};
    use mini_social_feed_server::state::AppState;

    let store = Arc::new(MemoryStore::new());
    let users: Arc<dyn UserStore> = store.clone();
    let posts: Arc<dyn PostStore> = store.clone();
    let graph: Arc<dyn GraphStore> = store.clone();
    let auth: Arc<dyn AuthPort> = Arc::new(JwtService::new("rl-test-secret".into(), users.clone()));
    let feed: Arc<dyn FeedPort> = Arc::new(ChronologicalFeed::new(posts.clone()));
    let event: Arc<dyn EventPort> = Arc::new(EventBus::new(64));
    let media_local = Arc::new(LocalMediaStore::new(
        std::env::temp_dir().join("mini-social-feed-rl-test-media"),
        MediaPolicy::named("standard"),
    ));

    let state = AppState::new(
        users,
        posts,
        graph,
        Some(media_local),
        None,
        Some(MediaPolicy::named("standard")),
        auth,
        feed,
        event,
    );

    let rate_limiter = Arc::new(RateLimiter::new());
    let rate_limit_config = Arc::new(RateLimitConfig::default()); // enabled by default
    let router = mini_social_feed_server::create_router(state, rate_limiter, rate_limit_config);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://{}", addr)
}
