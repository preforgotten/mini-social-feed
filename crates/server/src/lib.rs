pub mod error;
pub mod middleware;
pub mod routes;
pub mod state;

use std::sync::Arc;

use axum::Router;
use mini_social_feed_data::RateLimitConfig;
use mini_social_feed_infra::RateLimiter;
use tower_http::trace::TraceLayer;

use crate::middleware::RateLimitLayer;

pub fn create_router(
    state: state::AppState,
    rate_limiter: Arc<RateLimiter>,
    rate_limit_config: Arc<RateLimitConfig>,
) -> Router {
    let rl = RateLimitLayer::new(rate_limiter, rate_limit_config);

    Router::new()
        .route(
            "/api/health",
            axum::routing::get(routes::health::health_check),
        )
        // auth
        .route(
            "/api/auth/challenge",
            axum::routing::post(routes::auth::request_challenge),
        )
        .route(
            "/api/auth/verify",
            axum::routing::post(routes::auth::verify_challenge),
        )
        .route(
            "/api/auth/refresh",
            axum::routing::post(routes::auth::refresh),
        )
        .route("/api/auth/login", axum::routing::post(routes::auth::login))
        .route(
            "/api/auth/api-key",
            axum::routing::post(routes::auth::create_api_key),
        )
        .route(
            "/api/auth/logout",
            axum::routing::post(routes::auth::logout),
        )
        // users
        .route(
            "/api/users/register",
            axum::routing::post(routes::users::register_user),
        )
        .route(
            "/api/users/:user_id",
            axum::routing::get(routes::users::get_user),
        )
        .route(
            "/api/users/:user_id/posts",
            axum::routing::get(routes::users::get_user_posts),
        )
        .route(
            "/api/users/me/nostr",
            axum::routing::post(routes::users::link_nostr),
        )
        .route(
            "/api/users/me",
            axum::routing::put(routes::users::update_profile),
        )
        .route(
            "/api/users/me/avatar",
            axum::routing::post(routes::users::upload_avatar),
        )
        .route(
            "/api/users/:id/followers",
            axum::routing::get(routes::users::get_user_followers),
        )
        .route(
            "/api/users/:id/following",
            axum::routing::get(routes::users::get_user_following),
        )
        // relationships
        .route(
            "/api/relationships/subscribers",
            axum::routing::get(routes::relationships::list_subscribers),
        )
        .route(
            "/api/relationships/subscriptions",
            axum::routing::get(routes::relationships::list_subscriptions),
        )
        .route(
            "/api/relationships/connections",
            axum::routing::get(routes::relationships::list_connections),
        )
        .route(
            "/api/relationships/:id/subscribe",
            axum::routing::post(routes::relationships::subscribe),
        )
        .route(
            "/api/relationships/:id/subscribe",
            axum::routing::delete(routes::relationships::unsubscribe),
        )
        .route(
            "/api/relationships/:id/connect",
            axum::routing::post(routes::relationships::connect_request),
        )
        .route(
            "/api/relationships/:id/connect",
            axum::routing::delete(routes::relationships::connect_remove),
        )
        .route(
            "/api/relationships/:id/accept",
            axum::routing::post(routes::relationships::connect_accept),
        )
        .route(
            "/api/relationships/:id/block",
            axum::routing::post(routes::relationships::block),
        )
        .route(
            "/api/relationships/:id/block",
            axum::routing::delete(routes::relationships::unblock),
        )
        // posts
        .route(
            "/api/posts",
            axum::routing::post(routes::posts::create_post),
        )
        .route("/api/posts", axum::routing::get(routes::posts::get_feed))
        .route(
            "/api/posts/:id",
            axum::routing::get(routes::posts::get_post),
        )
        .route(
            "/api/posts/:id/thread",
            axum::routing::get(routes::posts::get_thread),
        )
        // search
        .route(
            "/api/search/users",
            axum::routing::post(routes::search::search_users),
        )
        .route(
            "/api/search/posts",
            axum::routing::post(routes::search::search_posts),
        )
        // media
        .route(
            "/api/media",
            axum::routing::post(routes::media::upload_media),
        )
        .route(
            "/api/media/:id",
            axum::routing::get(routes::media::serve_media),
        )
        .route(
            "/api/media/:id",
            axum::routing::delete(routes::media::delete_media),
        )
        // events
        .route(
            "/api/events",
            axum::routing::get(routes::events::event_stream),
        )
        .layer(TraceLayer::new_for_http())
        .layer(rl)
        .with_state(state)
}
