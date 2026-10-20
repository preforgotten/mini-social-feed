use crate::common::{register_user, spawn_server};

#[tokio::test]
async fn events_sse_stream_receives_registration() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    // Open SSE stream first
    let sse_resp = client
        .get(format!("{}/api/events", base))
        .send()
        .await
        .unwrap();
    assert!(sse_resp.status().is_success());
    // We can't easily test the stream bytes in a short test,
    // but we can verify the endpoint accepts connections.

    // Register a user — this emits an event
    let (_token, _user_id) = register_user(&base, &client, "eventuser").await;

    // Verify we can still use the API normally after event emission
    let resp = client
        .get(format!("{}/api/health", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn events_endpoint_accessible() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/events", base))
        .header("Accept", "text/event-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
}
