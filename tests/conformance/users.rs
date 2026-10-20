use mini_social_feed_domain::UserResponse;
use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn user_registration_and_profile() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "alice", "password": "test123", "email": "alice@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["access_token"].as_str().unwrap();
    assert!(!token.is_empty());
    let user: UserResponse = serde_json::from_value(body["user"].clone()).unwrap();
    assert_eq!(user.username, "alice");
    assert_eq!(user.email.as_deref(), Some("alice@example.com"));
    let user_id = user.id.clone();

    let dup = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "alice", "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409);

    let get = client
        .get(format!("{}/api/users/{}", base, user_id))
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    let fetched: UserResponse = get.json().await.unwrap();
    assert_eq!(fetched.username, "alice");
}

#[tokio::test]
async fn lookup_user_by_username() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "alice").await;

    let resp = client
        .post(format!("{}/api/search/users", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"username": "alice"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let results: Vec<UserResponse> = resp.json().await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].username, "alice");

    let missing = client
        .post(format!("{}/api/search/users", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"username": "nobody"}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 200);
    let empty: Vec<UserResponse> = missing.json().await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn update_profile() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "updater").await;
    let auth = auth_header(&token);

    let resp = client
        .put(format!("{}/api/users/me", base))
        .header("Authorization", &auth)
        .json(&json!({"bio": "rustacean", "email": "new@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: UserResponse = resp.json().await.unwrap();
    assert_eq!(updated.bio.as_deref(), Some("rustacean"));
    assert_eq!(updated.email.as_deref(), Some("new@example.com"));

    let unauth = client
        .put(format!("{}/api/users/me", base))
        .json(&json!({"bio": "hacker"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), 401);
}

#[tokio::test]
async fn nonexistent_user_returns_404() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/users/nonexistent", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn register_with_empty_username_returns_400() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "", "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn register_with_oversize_bio_returns_400() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let oversize = "a".repeat(mini_social_feed_domain::MAX_BIO_LEN + 1);
    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "ok", "password": "test123", "bio": oversize}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn register_with_oversize_username_returns_400() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let oversize = "a".repeat(mini_social_feed_domain::MAX_USERNAME_LEN + 1);
    let resp = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": oversize, "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn user_followers_and_following() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token_a, alice_id) = register_user(&base, &client, "follower_alice").await;
    let (token_b, bob_id) = register_user(&base, &client, "follower_bob").await;

    // Alice subscribes to Bob
    client
        .post(format!("{}/api/relationships/{}/subscribe", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();

    // Bob's followers include Alice
    let followers: Vec<UserResponse> = client
        .get(format!("{}/api/users/{}/followers", base, bob_id))
        .header("Authorization", auth_header(&token_b))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(followers.len(), 1);
    assert_eq!(followers[0].id, alice_id);

    // Alice's following includes Bob
    let following: Vec<UserResponse> = client
        .get(format!("{}/api/users/{}/following", base, alice_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(following.len(), 1);
    assert_eq!(following[0].id, bob_id);
}
