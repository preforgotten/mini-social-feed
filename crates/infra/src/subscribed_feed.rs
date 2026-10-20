use async_trait::async_trait;
use std::sync::Arc;

use mini_social_feed_app::{AppError, FeedPort};
use mini_social_feed_data::{GraphStore, PostStore};
use mini_social_feed_domain::{Direction, EntityId, Page, PageParams, Post, Relation, UserId};

/// Feed of posts from users the viewer subscribes to, newest-first.
///
/// If the viewer has no subscriptions, returns an empty feed.
#[derive(Clone)]
pub struct SubscribedFeed {
    graph_store: Arc<dyn GraphStore>,
    post_store: Arc<dyn PostStore>,
}

impl SubscribedFeed {
    pub fn new(graph_store: Arc<dyn GraphStore>, post_store: Arc<dyn PostStore>) -> Self {
        Self {
            graph_store,
            post_store,
        }
    }
}

#[async_trait]
impl FeedPort for SubscribedFeed {
    async fn get_feed(
        &self,
        viewer: Option<&UserId>,
        params: &PageParams,
    ) -> Result<Page<Post>, AppError> {
        let viewer = match viewer {
            Some(v) => v,
            None => return Ok(Page::empty(params.page_size())),
        };

        // Get subscribed user IDs from the graph store
        let entity = EntityId::User(viewer.clone());
        let subscribed_entities = self
            .graph_store
            .related(&entity, Relation::Subscribe, Direction::Outbound)
            .await
            .map_err(AppError::Data)?;

        let subscribed_ids: Vec<UserId> = subscribed_entities
            .iter()
            .filter_map(|e| match e {
                EntityId::User(uid) => Some(uid.clone()),
                _ => None,
            })
            .collect();

        if subscribed_ids.is_empty() {
            return Ok(Page::empty(params.page_size()));
        }

        // Fetch a page of all posts, then filter to subscribed authors.
        // This trades a larger fetch for correctness — future optimization
        // can push the author filter into the data layer.
        let page = self
            .post_store
            .get_posts(params, None)
            .await
            .map_err(AppError::Data)?;

        let filtered: Vec<Post> = page
            .items
            .into_iter()
            .filter(|p| subscribed_ids.contains(&p.author_id))
            .collect();

        // Build cursors from the filtered results
        let first = filtered
            .first()
            .map(|p| mini_social_feed_domain::Cursor::from_time_id(p.created_at, &p.id.0));
        let last = filtered
            .last()
            .map(|p| mini_social_feed_domain::Cursor::from_time_id(p.created_at, &p.id.0));

        Ok(Page {
            items: filtered,
            total: page.total,
            page_size: params.page_size(),
            first,
            last,
        })
    }
}
