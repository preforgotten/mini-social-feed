use async_trait::async_trait;
use std::sync::Arc;

use mini_social_feed_app::{AppError, FeedPort};
use mini_social_feed_data::PostStore;
use mini_social_feed_domain::{Page, PageParams, Post, UserId};

/// Chronological feed — newest-first, no ranking.
///
/// Delegates to the `PostStore` trait. Swappable for `HotFeed`,
/// `TrendingFeed`, etc. by providing a different implementation of `FeedPort`.
pub struct ChronologicalFeed {
    post_store: Arc<dyn PostStore>,
}

impl ChronologicalFeed {
    pub fn new(post_store: Arc<dyn PostStore>) -> Self {
        Self { post_store }
    }
}

#[async_trait]
impl FeedPort for ChronologicalFeed {
    async fn get_feed(
        &self,
        _viewer: Option<&UserId>,
        params: &PageParams,
    ) -> Result<Page<Post>, AppError> {
        self.post_store
            .get_posts(params, None)
            .await
            .map_err(AppError::Data)
    }
}
