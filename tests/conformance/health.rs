use crate::common::spawn_server;

#[tokio::test]
async fn health_check_works() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .get(format!("{}/api/health", base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp["status"], "ok");
}
