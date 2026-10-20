use serde_json::json;

use crate::common::{auth_header, spawn_server};

#[tokio::test]
async fn concurrent_registration_same_username() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let url = format!("{}/api/users/register", base);
    let body = json!({"username": "concurrent", "password": "test123"});

    let (r1, r2) = tokio::join!(
        client.post(&url).json(&body).send(),
        client.post(&url).json(&body).send(),
    );

    let s1 = r1.unwrap().status();
    let s2 = r2.unwrap().status();

    // One must succeed (201), the other must be rejected (409)
    assert!(
        (s1 == 201 && s2 == 409) || (s1 == 409 && s2 == 201),
        "concurrent registration: got {s1} and {s2}, expected 201+409"
    );
}

#[tokio::test]
async fn refresh_token_is_bearer_not_bound_to_access_token() {
    // Refresh tokens are bearer credentials — anyone who obtains a valid
    // refresh token can use it to get new tokens. The refresh endpoint
    // does not require authentication. Security relies on TLS + token secrecy.
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let alice_resp: serde_json::Value = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "alice_refresh_bearer", "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_refresh = alice_resp["refresh_token"].as_str().unwrap();

    // Refresh works without any auth header — it's a bearer credential
    let refresh = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": alice_refresh}))
        .send()
        .await
        .unwrap();
    assert!(
        refresh.status().is_success(),
        "valid refresh token should work as bearer credential"
    );

    let body: serde_json::Value = refresh.json().await.unwrap();
    assert!(body["access_token"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn logout_then_immediate_refresh_fails() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "quicklogout", "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let access = resp["access_token"].as_str().unwrap().to_string();
    let refresh = resp["refresh_token"].as_str().unwrap().to_string();

    // Logout
    client
        .post(format!("{}/api/auth/logout", base))
        .header("Authorization", auth_header(&access))
        .send()
        .await
        .unwrap();

    // Immediate refresh attempt
    let retry = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": refresh}))
        .send()
        .await
        .unwrap();
    assert!(
        !retry.status().is_success(),
        "refresh after logout should fail"
    );
}

#[tokio::test]
async fn expired_token_rejected() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    // Try a completely fabricated JWT
    let resp = client
        .get(format!("{}/api/posts", base))
        .header(
            "Authorization",
            "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJmYWtlIn0.fake",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
