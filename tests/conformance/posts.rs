use mini_social_feed_domain::{Page, PostResponse, ThreadResponse, UserResponse};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn create_post_requires_auth() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/posts", base))
        .json(&json!({"content": "hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn full_post_and_thread_flow() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "poster").await;
    let auth = auth_header(&token);

    let root_resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "root post"}))
        .send()
        .await
        .unwrap();
    assert_eq!(root_resp.status(), 201);
    let root: PostResponse = root_resp.json().await.unwrap();

    let reply_resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "a reply", "parent_id": root.id}))
        .send()
        .await
        .unwrap();
    assert_eq!(reply_resp.status(), 201);
    let reply: PostResponse = reply_resp.json().await.unwrap();

    let nested_resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "nested reply", "parent_id": reply.id}))
        .send()
        .await
        .unwrap();
    assert_eq!(nested_resp.status(), 201);
    let nested: PostResponse = nested_resp.json().await.unwrap();

    let feed: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=10", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(feed.items.len(), 3);
    assert_eq!(feed.total, 3);

    let thread_resp = client
        .get(format!("{}/api/posts/{}/thread", base, root.id))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let thread_status = thread_resp.status();
    let thread_body = thread_resp.text().await.unwrap();
    let thread: ThreadResponse = serde_json::from_str(&thread_body).unwrap_or_else(|e| {
        panic!(
            "thread failed: status={}, body={}, err={}",
            thread_status, thread_body, e
        )
    });
    assert_eq!(thread.post.id, root.id);
    assert_eq!(thread.replies.len(), 1);
    assert_eq!(thread.replies[0].post.id, reply.id);
    assert_eq!(thread.replies[0].replies.len(), 1);
    assert_eq!(thread.replies[0].replies[0].post.id, nested.id);

    let user_info: UserResponse = client
        .get(format!("{}/api/users/{}", base, root.author_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let user_posts: Page<PostResponse> = client
        .get(format!(
            "{}/api/users/{}/posts?limit=10",
            base, user_info.id
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(user_posts.total, 3);
}

#[tokio::test]
async fn content_size_limit_is_enforced() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "sizetester").await;

    let oversize = "a".repeat(mini_social_feed_domain::MAX_CONTENT_SIZE + 1);
    let resp = client
        .post(format!("{}/api/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"content": oversize}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn nonexistent_post_returns_404() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "looker").await;

    let resp = client
        .get(format!("{}/api/posts/nonexistent", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn read_post_requires_auth() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/posts/some-id", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn thread_of_nonexistent_post_returns_404() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "threadlooker").await;

    let resp = client
        .get(format!("{}/api/posts/nonexistent/thread", base))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn feed_pagination_respects_limit() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "pager").await;
    let auth = auth_header(&token);

    for i in 0..5 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("post {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let page: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=2", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, 5);
    assert_eq!(page.page_size, 2);
    // With 5 posts and limit=2, there should be more pages
    assert!(
        page.last.is_some(),
        "last cursor should exist for next page"
    );
}

#[tokio::test]
async fn cursor_pagination_walks_all_pages() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "cursorwalker").await;
    let auth = auth_header(&token);

    // Create 10 posts
    for i in 0..10 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("post {}", i)}))
            .send()
            .await
            .unwrap();
    }

    // Page 1: first 3 items
    let p1: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=3", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 3);
    assert_eq!(p1.total, 10);
    assert!(p1.last.is_some(), "should have last cursor");
    let cursor = p1.last.clone().unwrap().0;

    // Page 2: next 3 items after cursor
    let p2: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=3&before={}", base, cursor))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 3);
    assert!(p2.last.is_some(), "should have last cursor");

    // Page 3: next 3 items
    let cursor2 = p2.last.as_ref().unwrap().0.clone();
    let p3: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=3&before={}", base, cursor2))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p3.items.len(), 3);

    // Page 4: remaining 1 item
    let cursor3 = p3.last.as_ref().unwrap().0.clone();
    let p4: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=3&before={}", base, cursor3))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p4.items.len(), 1);

    // Verify all 10 unique posts across pages
    let mut seen = std::collections::HashSet::new();
    for p in [p1, p2, p3, p4].iter().flat_map(|page| page.items.iter()) {
        seen.insert(p.id.clone());
    }
    assert_eq!(seen.len(), 10, "all 10 posts should be unique across pages");
}

#[tokio::test]
async fn cursor_pagination_empty_page_after_last() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "emptypager").await;
    let auth = auth_header(&token);

    // Create 1 post
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "only post"}))
        .send()
        .await
        .unwrap();

    // First page gets the post
    let p1: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=5", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 1);
    assert_eq!(p1.total, 1);

    // Second page past last should be empty
    if let Some(ref last) = p1.last {
        let p2: Page<PostResponse> = client
            .get(format!("{}/api/posts?limit=5&before={}", base, last.0))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(p2.items.len(), 0, "page past last item should be empty");
        assert_eq!(p2.total, 1);
    }
}

#[tokio::test]
async fn cursor_pagination_user_posts() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, user_id) = register_user(&base, &client, "userpager").await;
    let auth = auth_header(&token);

    for i in 0..8 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("user post {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let p1: Page<PostResponse> = client
        .get(format!("{}/api/users/{}/posts?limit=3", base, user_id))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 3);
    assert_eq!(p1.total, 8);
    assert!(p1.last.is_some());

    let c = p1.last.unwrap().0;
    let p2: Page<PostResponse> = client
        .get(format!(
            "{}/api/users/{}/posts?limit=3&before={}",
            base, user_id, c
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 3);
    let ids1: Vec<_> = p1.items.iter().map(|p| &p.id).collect();
    let ids2: Vec<_> = p2.items.iter().map(|p| &p.id).collect();
    for id in &ids1 {
        assert!(!ids2.contains(id), "pages should not overlap");
    }
}

#[tokio::test]
async fn cursor_pagination_page_size_alias() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "pagesizer").await;
    let auth = auth_header(&token);

    for i in 0..5 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("p {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let p1: Page<PostResponse> = client
        .get(format!("{}/api/posts?limit=2", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    assert_eq!(p1.page_size, 2);

    let p2: Page<PostResponse> = client
        .get(format!("{}/api/posts?page_size=2", base))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 2);
    assert_eq!(p2.page_size, 2);
}

#[tokio::test]
async fn cursor_pagination_since_window() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "sinceuser").await;
    let auth = auth_header(&token);

    let mid = chrono::Utc::now().timestamp();
    for i in 0..3 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("early {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let page: Page<PostResponse> = client
        .get(format!("{}/api/posts?since={}", base, mid))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.total, 3);
}

// search
#[tokio::test]
async fn search_posts_by_content() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "searcher").await;
    let auth = auth_header(&token);

    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "Rust is great"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "Go is also fine"}))
        .send()
        .await
        .unwrap();

    let results: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content_contains": "Rust"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "Rust is great");
}

#[tokio::test]
async fn search_posts_by_author_username() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "author1").await;
    let auth = auth_header(&token);

    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "hello from author1"}))
        .send()
        .await
        .unwrap();

    let results: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"author_username": "author1"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn search_posts_combined_filters() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "writer").await;
    let auth = auth_header(&token);

    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "async rust is fast"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{}/api/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content": "sync rust is also fine"}))
        .send()
        .await
        .unwrap();

    let results: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content_contains": "async", "author_username": "writer"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn search_posts_empty_results() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "empty_search").await;

    let results: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", auth_header(&token))
        .json(&json!({"content_contains": "noone writes this"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn search_posts_pagination() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "paged").await;
    let auth = auth_header(&token);

    for i in 0..5 {
        client
            .post(format!("{}/api/posts", base))
            .header("Authorization", &auth)
            .json(&json!({"content": format!("searchable {}", i)}))
            .send()
            .await
            .unwrap();
    }

    let page1: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content_contains": "searchable", "limit": 2, "offset": 0}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page1.len(), 2);

    let page2: Vec<PostResponse> = client
        .post(format!("{}/api/search/posts", base))
        .header("Authorization", &auth)
        .json(&json!({"content_contains": "searchable", "limit": 2, "offset": 2}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);

    let ids_p1: Vec<_> = page1.iter().map(|p| &p.id).collect();
    let ids_p2: Vec<_> = page2.iter().map(|p| &p.id).collect();
    for id in &ids_p1 {
        assert!(!ids_p2.contains(id), "pages should not overlap");
    }
}

#[tokio::test]
async fn schnorr_nostr_post_lifecycle() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "schnorruser").await;
    let auth = auth_header(&token);

    let schnorr_sk = k256::schnorr::SigningKey::random(&mut rand_core::OsRng);
    let schnorr_vk = schnorr_sk.verifying_key();
    let xonly_hex = hex::encode(schnorr_vk.to_bytes());

    let chall: serde_json::Value = client
        .post(format!("{}/api/auth/challenge", base))
        .json(&json!({"public_key": xonly_hex}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let chall_bytes = hex::decode(chall["challenge_hex"].as_str().unwrap()).unwrap();
    let aux = [0u8; 32];
    let chall_sig = schnorr_sk.sign_raw(&chall_bytes, &aux).unwrap();

    let link_resp = client
        .post(format!("{}/api/users/me/nostr", base)).header("Authorization", &auth)
        .json(&json!({"challenge_id": chall["challenge_id"], "signature": hex::encode(chall_sig.to_bytes()), "public_key": xonly_hex}))
        .send().await.unwrap();
    let link_status = link_resp.status();
    let link_body = link_resp.text().await.unwrap();
    assert!(link_status.is_success(), "link failed: {link_body}");

    let content = "schnorr-signed post";
    let ts = chrono::Utc::now().timestamp() as u64;
    let tags: Vec<Vec<String>> = vec![];
    let ev = json!([0, xonly_hex, ts, 1, tags, content]);
    let serialized = serde_json::to_string(&ev).unwrap();
    let event_id = hex::encode(Sha256::digest(serialized.as_bytes()));
    let event_hash = Sha256::digest(serialized.as_bytes());
    let event_sig = schnorr_sk.sign_raw(&event_hash, &aux).unwrap();

    let post: PostResponse = client
        .post(format!("{}/api/posts", base)).header("Authorization", &auth)
        .json(&json!({"content": content, "nostr_event_id": event_id, "nostr_sig": hex::encode(event_sig.to_bytes()), "nostr_ts": ts}))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(post.nostr_event_id.as_deref(), Some(event_id.as_str()));
    assert_eq!(
        post.nostr_sig.as_deref(),
        Some(hex::encode(event_sig.to_bytes()).as_str())
    );
}
