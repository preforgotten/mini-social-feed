use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn negative_feed_limit_returns_200_or_400() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "neglim").await;

    let resp = client
        .get(format!("{}/api/posts?limit=-1", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success() || resp.status() == 400);
}

#[tokio::test]
async fn content_at_max_size_is_accepted() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "maxcon").await;

    let content = "a".repeat(mini_social_feed_domain::MAX_CONTENT_SIZE);
    let resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"content": content}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "exact max content should be accepted");
}

#[tokio::test]
async fn username_at_max_length_is_accepted() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let name = "a".repeat(mini_social_feed_domain::MAX_USERNAME_LEN);
    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": name, "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "exact max username should be accepted");
}

#[tokio::test]
async fn empty_post_content_accepted_or_rejected() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "emptycon").await;

    let resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"content": ""}))
        .send()
        .await
        .unwrap();
    // Currently accepted (no empty-content validation) — document the behavior
    assert!(resp.status().is_success() || resp.status() == 400);
}

#[tokio::test]
async fn malformed_json_returns_400() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .header("Content-Type", "application/json")
        .body("{ bad json ")
        .send()
        .await
        .unwrap();
    assert!(!resp.status().is_success());
}

#[tokio::test]
async fn wrong_content_type_handled() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .header("Content-Type", "text/plain")
        .body("username=test")
        .send()
        .await
        .unwrap();
    // Should not crash — either 400 or 415
    assert!(resp.status() != 500);
}

#[tokio::test]
async fn page_size_capped_at_100() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "hugepage").await;
    let auth = auth_header(&token);

    for i in 0..3 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("p {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let resp = client
        .get(format!("{}/api/posts?limit=1000", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    // Should not return more than 100 items even with limit=1000
    let page: serde_json::Value = resp.json().await.unwrap();
    let items = page["items"].as_array().unwrap();
    assert!(
        items.len() <= 3,
        "only 3 posts exist, should return at most 3"
    );
    assert_eq!(page["page_size"].as_u64().unwrap(), 100);
}
