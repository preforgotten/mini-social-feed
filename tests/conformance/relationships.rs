use mini_social_feed_domain::UserResponse;

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn subscribe_and_list_subscriptions() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, alice_id) = register_user(&base, &client, "sub_alice").await;
    let (token_b, bob_id) = register_user(&base, &client, "sub_bob").await;

    // Alice subscribes to Bob
    let resp = client
        .post(format!("{}/api/relationships/{}/subscribe", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // Alice's subscriptions include Bob
    let subs: Vec<UserResponse> = client
        .get(format!("{}/api/relationships/subscriptions", base))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].id, bob_id);

    // Bob's subscribers include Alice
    let subbers: Vec<UserResponse> = client
        .get(format!("{}/api/relationships/subscribers", base))
        .header("Authorization", auth_header(&token_b))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(subbers.len(), 1);
    assert_eq!(subbers[0].id, alice_id);
}

#[tokio::test]
async fn unsubscribe_removes_edge() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, bob_id) = register_user(&base, &client, "unsub_alice").await;
    register_user(&base, &client, "unsub_bob").await;

    client
        .post(format!("{}/api/relationships/{}/subscribe", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();

    let resp = client
        .delete(format!("{}/api/relationships/{}/subscribe", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let subs: Vec<UserResponse> = client
        .get(format!("{}/api/relationships/subscriptions", base))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(subs.is_empty());
}

#[tokio::test]
async fn connect_request_and_accept_flow() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, alice_id) = register_user(&base, &client, "conn_alice").await;
    let (token_b, bob_id) = register_user(&base, &client, "conn_bob").await;

    // Alice sends connect request to Bob
    let req = client
        .post(format!("{}/api/relationships/{}/connect", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(req.status(), 204);

    // Bob accepts Alice's request
    let accept = client
        .post(format!("{}/api/relationships/{}/accept", base, alice_id))
        .header("Authorization", auth_header(&token_b))
        .send()
        .await
        .unwrap();
    assert_eq!(accept.status(), 204);

    // Both see each other in connections
    let conns_a: Vec<UserResponse> = client
        .get(format!("{}/api/relationships/connections", base))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(conns_a.len(), 1);
    assert_eq!(conns_a[0].id, bob_id);

    let conns_b: Vec<UserResponse> = client
        .get(format!("{}/api/relationships/connections", base))
        .header("Authorization", auth_header(&token_b))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(conns_b.len(), 1);
    assert_eq!(conns_b[0].id, alice_id);
}

#[tokio::test]
async fn accept_without_request_fails() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, bob_id) = register_user(&base, &client, "no_req_alice").await;
    register_user(&base, &client, "no_req_bob").await;

    let resp = client
        .post(format!("{}/api/relationships/{}/accept", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn block_and_unblock() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let (token_a, bob_id) = register_user(&base, &client, "block_alice").await;
    register_user(&base, &client, "block_bob").await;

    let resp = client
        .post(format!("{}/api/relationships/{}/block", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let resp = client
        .delete(format!("{}/api/relationships/{}/block", base, bob_id))
        .header("Authorization", auth_header(&token_a))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn subscribe_nonexistent_user_returns_404() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "ghost_sub").await;

    let resp = client
        .post(format!("{}/api/relationships/nonexistent/subscribe", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn relationship_endpoints_require_auth() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    for path in &[
        "/api/relationships/subscribers",
        "/api/relationships/subscriptions",
        "/api/relationships/connections",
    ] {
        let resp = client
            .get(format!("{}{}", base, path))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "{} should require auth", path);
    }
}
