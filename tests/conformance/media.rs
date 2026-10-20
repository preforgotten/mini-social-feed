use crate::common::{auth_header, register_user, spawn_server};

#[tokio::test]
async fn media_upload_and_serve() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "mediauser").await;
    let auth = auth_header(&token);

    // Upload a simple PNG-like blob (small enough for policy)
    let bytes = vec![0u8; 100];
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes)
            .file_name("test.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload: serde_json::Value = client
        .post(format!("{}/api/media", base))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let media_id = upload["id"].as_str().unwrap();
    assert!(!media_id.is_empty());

    // Serve the media
    let serve = client
        .get(format!("{}/api/media/{}", base, media_id))
        .send()
        .await
        .unwrap();
    assert_eq!(serve.status(), 200);
    assert_eq!(serve.headers().get("content-type").unwrap(), "image/png");
}

#[tokio::test]
async fn media_delete_and_tombstone() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();
    let (token, _) = register_user(&base, &client, "delmedia").await;
    let auth = auth_header(&token);

    let bytes = vec![0u8; 100];
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes)
            .file_name("del.png")
            .mime_str("image/png")
            .unwrap(),
    );

    let upload: serde_json::Value = client
        .post(format!("{}/api/media", base))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let media_id = upload["id"].as_str().unwrap();

    // Delete
    let del = client
        .delete(format!("{}/api/media/{}", base, media_id))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);

    // Serve returns 404 for deleted media
    let serve = client
        .get(format!("{}/api/media/{}", base, media_id))
        .send()
        .await
        .unwrap();
    assert_eq!(serve.status(), 404);
}

#[tokio::test]
async fn media_upload_requires_auth() {
    let base = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/media", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
