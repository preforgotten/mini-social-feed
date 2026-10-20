use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn sql_injection_in_search_content() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "sqlinject").await;

    let resp = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"content_contains": "'; DROP TABLE posts; --"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    // Verify posts are still queryable (table not dropped)
    let feed = client
        .get(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(feed.status().is_success());
}

#[tokio::test]
async fn sql_injection_in_author_username() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "sqlinject2").await;

    let resp = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"author_username": "x' UNION SELECT * FROM users --"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn unicode_username_with_emoji() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "🚀alice🚀", "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn whitespace_only_username_stripped() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "   ", "password": "test123"}))
        .send()
        .await
        .unwrap();
    // Should reject — trimmed to empty
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn path_traversal_in_user_id() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/users/../../../etc/passwd", base))
        .send()
        .await
        .unwrap();
    // axum rejects malformed paths — 404 or 400, not 500 and not file access
    assert!(resp.status() != 500);
}

#[tokio::test]
async fn malformed_cursor_does_not_crash() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "malcursor").await;

    let resp = client
        .get(format!("{}/api/posts", base))
        .query(&[("before", "!!!not-valid-base64!!!")])
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "malformed cursor should not 500"
    );
}

#[tokio::test]
async fn cursor_with_sql_injection_does_not_crash() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "curinj").await;

    let bad = "'; DROP TABLE posts; --";
    let resp = client
        .get(format!("{}/api/posts", base))
        .query(&[("before", bad)])
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    // Table should still exist
    let feed = client
        .get(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert!(feed.status().is_success());
}
