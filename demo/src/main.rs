use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mini_social_feed_app::{AuthPort, EventPort, FeedPort};
use mini_social_feed_data::{
    GraphStore, LocalMediaStore, MediaPolicy, MediaStore, PostStore, RateLimitConfig, SqliteStore,
    UserStore,
};
use mini_social_feed_infra::{ChronologicalFeed, EventBus, JwtService, RateLimiter};
use mini_social_feed_server::state::AppState;
use tower_http::services::ServeFile;

mod seed;

const FRONTEND_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/dist");

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("mini_social_feed=info,tower_http=debug")
        .init();

    // Ensure frontend is built
    if !std::path::Path::new(FRONTEND_DIR)
        .join("index.html")
        .exists()
    {
        eprintln!("Frontend not built. Run: cd frontend && npm install && npm run build");
        std::process::exit(1);
    }

    // Create SQLite store
    let db_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/data");
    std::fs::create_dir_all(db_dir).expect("Failed to create data directory");
    let db_path = format!("{}/demo.db", db_dir);
    let store = SqliteStore::connect(&format!("sqlite:{db_path}?mode=rwc"))
        .await
        .expect("Failed to connect to SQLite");
    let store = Arc::new(store);
    let users: Arc<dyn UserStore> = store.clone();
    let posts: Arc<dyn PostStore> = store.clone();
    let graph: Arc<dyn GraphStore> = store.clone();
    let feed: Arc<dyn FeedPort> = Arc::new(ChronologicalFeed::new(posts.clone()));
    let auth: Arc<dyn AuthPort> = Arc::new(JwtService::new(
        "demo-secret-do-not-use-in-production".into(),
        users.clone(),
    ));
    let event: Arc<dyn EventPort> = Arc::new(EventBus::new(1024));

    // Seed demo data by making HTTP calls to the running server
    let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    let media_local: Option<Arc<dyn MediaStore>> = Some(Arc::new(LocalMediaStore::new(
        "uploads".into(),
        MediaPolicy::named("standard"),
    )));

    let state = AppState::new(
        users,
        posts,
        graph,
        media_local,
        None,
        Some(MediaPolicy::named("standard")),
        auth,
        feed,
        event,
    );

    // Build API router
    let rate_limiter = Arc::new(RateLimiter::new());
    let rate_limit_config = Arc::new(RateLimitConfig::default());
    let api =
        mini_social_feed_server::create_router(state.clone(), rate_limiter, rate_limit_config);

    let assets_dir = format!("{}/assets", FRONTEND_DIR);
    let index_path = format!("{}/index.html", FRONTEND_DIR);

    // Serve static assets + SPA fallback for client-side routing
    let app = Router::new()
        .merge(api)
        .nest_service("/assets", tower_http::services::ServeDir::new(assets_dir))
        .fallback_service(ServeFile::new(index_path));

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("API + frontend serving on http://localhost:{}", addr.port());

    // Seed in background after server starts
    let seed_url = base_url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if let Err(e) = seed::run(&seed_url).await {
            eprintln!("Seed failed: {e}");
        } else {
            tracing::info!("Demo data seeded successfully");
        }
    });

    axum::serve(listener, app).await.unwrap();
}
