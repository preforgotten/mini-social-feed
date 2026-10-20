use k256::schnorr::SigningKey;
use mini_social_feed_domain::{NostrVerifyResponse, UserResponse};
use serde_json::json;

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn challenge_verify_flow() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let sk = SigningKey::random(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    let pubkey = hex::encode(vk.to_bytes());

    let chall: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": pubkey}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let challenge_bytes = hex::decode(chall["challenge_hex"].as_str().unwrap()).unwrap();
    assert_eq!(challenge_bytes.len(), 32);

    let aux = [0u8; 32];
    let sig = sk.sign_raw(&challenge_bytes, &aux).unwrap();
    let sig_hex = hex::encode(sig.to_bytes());

    let verify: NostrVerifyResponse = client
        .post(format!("{}/api/auth/verify", base))
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": sig_hex, "public_key": pubkey}))
        .send().await.unwrap().json().await.unwrap();
    assert!(verify.verified);

    let reuse = client
        .post(format!("{}/api/auth/verify", base))
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": sig_hex, "public_key": pubkey}))
        .send().await.unwrap();
    assert!(
        !reuse.status().is_success(),
        "challenge should be one-time-use"
    );
}

#[tokio::test]
async fn link_nostr_pubkey_to_profile() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "nostruser").await;
    let auth = auth_header(&token);

    let sk = SigningKey::random(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    let pubkey = hex::encode(vk.to_bytes());

    let chall: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": pubkey}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let challenge_bytes = hex::decode(chall["challenge_hex"].as_str().unwrap()).unwrap();
    let aux = [0u8; 32];
    let sig = sk.sign_raw(&challenge_bytes, &aux).unwrap();

    let link_resp = client
        .post(format!("{}/api/users/me/nostr", base)).header("Authorization", &auth)
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": hex::encode(sig.to_bytes()), "public_key": pubkey}))
        .send().await.unwrap();
    assert_eq!(link_resp.status(), 200);
    let updated: UserResponse = link_resp.json().await.unwrap();
    assert_eq!(updated.nostr_pubkey.as_deref(), Some(pubkey.as_str()));
}

#[tokio::test]
async fn wrong_signature_fails() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let sk = SigningKey::random(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    let pubkey = hex::encode(vk.to_bytes());

    let chall: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": pubkey}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let verify = client
        .post(format!("{}/api/auth/verify", base))
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": "00", "public_key": pubkey}))
        .send().await.unwrap();
    assert!(!verify.status().is_success());
}

#[tokio::test]
async fn link_nostr_requires_auth() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/users/me/nostr", base))
        .json(&json!({"challenge_id": "fake", "signature": "00", "public_key": "03aa"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn nostr_pubkey_cannot_be_linked_twice() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, _) = register_user(&base, &client, "first").await;
    let (token_b, _) = register_user(&base, &client, "second").await;

    let sk = SigningKey::random(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    let pubkey = hex::encode(vk.to_bytes());
    let aux = [0u8; 32];

    let chall: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": pubkey}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sig = sk
        .sign_raw(
            &hex::decode(chall["challenge_hex"].as_str().unwrap()).unwrap(),
            &aux,
        )
        .unwrap();

    let link1 = client
        .post(format!("{}/api/users/me/nostr", base)).header("Authorization", auth_header(&token_a))
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": hex::encode(sig.to_bytes()), "public_key": pubkey}))
        .send().await.unwrap();
    assert_eq!(link1.status(), 200);

    let chall2: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": pubkey}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sig2 = sk
        .sign_raw(
            &hex::decode(chall2["challenge_hex"].as_str().unwrap()).unwrap(),
            &aux,
        )
        .unwrap();

    let link2 = client
        .post(format!("{}/api/users/me/nostr", base)).header("Authorization", auth_header(&token_b))
        .json(&json!({"challenge_id": chall2["challenge_id"], "signature": hex::encode(sig2.to_bytes()), "public_key": pubkey}))
        .send().await.unwrap();
    assert_eq!(
        link2.status(),
        409,
        "same nostr pubkey should not link to two users"
    );
}

#[tokio::test]
async fn refresh_token_flow() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "refresher", "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp["user"]["username"], "refresher");
    let access_token = resp["access_token"].as_str().unwrap().to_string();
    let refresh_token = resp["refresh_token"].as_str().unwrap().to_string();
    assert!(!access_token.is_empty());
    assert!(!refresh_token.is_empty());

    let post_resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&access_token))
        .json(&json!({"content": "authenticated"}))
        .send()
        .await
        .unwrap();
    assert_eq!(post_resp.status(), 201);

    let refresh_resp: serde_json::Value = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": refresh_token}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_access = refresh_resp["access_token"].as_str().unwrap();
    let new_refresh = refresh_resp["refresh_token"].as_str().unwrap();
    assert!(!new_access.is_empty());
    assert!(!new_refresh.is_empty());

    let reuse = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": refresh_token}))
        .send()
        .await
        .unwrap();
    assert!(
        !reuse.status().is_success(),
        "refresh tokens should be one-time-use"
    );

    let second_refresh: serde_json::Value = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": new_refresh}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(second_refresh["access_token"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn logout_revokes_refresh_tokens() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "quitter", "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let access = resp["access_token"].as_str().unwrap().to_string();
    let refresh = resp["refresh_token"].as_str().unwrap().to_string();

    let logout_resp = client
        .post(format!("{}/api/auth/logout", base))
        .header("Authorization", auth_header(&access))
        .send()
        .await
        .unwrap();
    assert_eq!(logout_resp.status(), 200);

    let reuse = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": refresh}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        reuse.status(),
        401,
        "refresh should be revoked after logout"
    );
}

#[tokio::test]
async fn invalid_refresh_token_rejected() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/auth/refresh", base))
        .json(&json!({"refresh_token": "not-a-real-token"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn login_returns_tokens() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    // Register first
    client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "logintest", "password": "test123"}))
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = client
        .post(format!("{}/api/auth/login", base))
        .json(&json!({"username": "logintest", "password": "test123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp["access_token"].as_str().unwrap().len() > 0);
    assert!(resp["refresh_token"].as_str().unwrap().len() > 0);
    assert_eq!(resp["user"]["username"], "logintest");
}

#[tokio::test]
async fn login_wrong_password_fails() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    client
        .post(format!("{}/api/users/register", base))
        .json(&json!({"username": "wrongpw", "password": "test123"}))
        .send()
        .await
        .unwrap();

    let resp = client
        .post(format!("{}/api/auth/login", base))
        .json(&json!({"username": "wrongpw", "password": "badpw"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn api_key_creation_and_usage() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "apikeyuser").await;

    let key_resp: serde_json::Value = client
        .post(format!("{}/api/auth/api-key", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let api_key = key_resp["api_key"].as_str().unwrap();

    // Use API key to access a protected endpoint
    let resp = client
        .get(format!("{}/api/posts", base))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "API key should authorize requests"
    );
}
