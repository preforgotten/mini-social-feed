use mini_social_feed_data::Config;
use mini_social_feed_infra::RateLimiter;
use mini_social_feed_server::state::AppState;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mini_social_feed=info,tower_http=debug".into()),
        )
        .init();

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production".to_string());

    let config = Config::default();

    let state = AppState::with_defaults(&config, jwt_secret);

    let rate_limiter = Arc::new(RateLimiter::new());
    let rate_limit_config = Arc::new(config.rate_limit.clone());
    let router = mini_social_feed_server::create_router(state, rate_limiter, rate_limit_config);

    let addr = "0.0.0.0:3000";
    tracing::info!("serving on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
