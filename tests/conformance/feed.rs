use mini_social_feed_domain::Page;
use mini_social_feed_domain::PostResponse;
use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

/// Note: the default server uses ChronologicalFeed, so subscribed filtering
/// is not active. This test documents that the feed returns posts from all
/// users (chronological behavior). When SubscribedFeed is wired, we'll
/// add a separate test for post filtering.
#[tokio::test]
async fn feed_includes_posts_from_non_subscribed_users() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, _) = register_user(&base, &client, "feed_alice").await;
    let (token_b, _) = register_user(&base, &client, "feed_bob").await;

    // Bob posts
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token_b))
        .json(&json!({"content": "bob's post"}))
        .send()
        .await
        .unwrap();

    // Alice sees Bob's post in global feed (chronological, no filter)
    let feed: Page<PostResponse> = client
        .get(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(feed.items.iter().any(|p| p.content == "bob's post"));
}
