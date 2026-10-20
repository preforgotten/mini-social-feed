use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use mini_social_feed_domain::MediaRef;
use serde::Deserialize;

use crate::error::HttpError;
use crate::middleware::AuthUser;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct UploadParams {
    #[serde(default = "default_backend")]
    backend: String,
}

fn default_backend() -> String {
    "local".into()
}

/// POST /api/media — upload a single file.
/// Query param `?backend=local` (default) or `?backend=cloud`.
/// Returns 400 if the requested backend is not available.
/// For local backend, checks per-user storage quota (413 if exceeded).
pub async fn upload_media(
    State(state): State<AppState>,
    Query(params): Query<UploadParams>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MediaRef>), HttpError> {
    let mut data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = Some(field.file_name().unwrap_or("unnamed").to_string());
            content_type = field
                .content_type()
                .map(|s| s.to_string())
                .or_else(|| Some("application/octet-stream".into()));
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| HttpError::BadRequest(e.to_string()))?
                    .to_vec(),
            );
            break;
        }
    }

    let data = data.ok_or_else(|| HttpError::BadRequest("missing 'file' field".into()))?;
    let filename = filename.unwrap_or_else(|| "unnamed".into());
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".into());

    // Select backend based on query param
    let backend = params.backend.as_str();
    match backend {
        "local" => {
            let store = state
                .media_local
                .as_ref()
                .ok_or_else(|| HttpError::BadRequest("local media backend not enabled".into()))?;

            // Per-user quota check for local storage
            if let Some(ref policy) = state.media_local_policy {
                let used = state.users.get_space_used(&auth.0).await?;
                let new_size = data.len() as u64;
                if used + new_size > policy.max_space_per_user {
                    return Err(HttpError::PayloadTooLarge(format!(
                        "storage quota exceeded: {used} used + {new_size} new > {} max",
                        policy.max_space_per_user
                    )));
                }
            }

            let media_ref = store
                .upload(&auth.0, data, &filename, &content_type)
                .await?;
            Ok((StatusCode::CREATED, Json(media_ref)))
        }
        "cloud" => {
            let store = state
                .media_cloud
                .as_ref()
                .ok_or_else(|| HttpError::BadRequest("cloud media backend not enabled".into()))?;
            let media_ref = store
                .upload(&auth.0, data, &filename, &content_type)
                .await?;
            Ok((StatusCode::CREATED, Json(media_ref)))
        }
        other => Err(HttpError::BadRequest(format!("unknown backend: {other}"))),
    }
}

/// GET /api/media/:id — serve the actual file bytes.
/// Tries local first, then cloud.
pub async fn serve_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, axum::response::Response), HttpError> {
    use axum::body::Body;
    use axum::http::header;
    use axum::response::Response;

    // Try local first — read file from disk
    if let Some(store) = state.media_local.as_ref() {
        if let Some((data, content_type)) = store.get_file(&id).await? {
            return Ok((
                StatusCode::OK,
                Response::builder()
                    .header(header::CONTENT_TYPE, &content_type)
                    .header(header::CACHE_CONTROL, "public, max-age=86400")
                    .body(Body::from(data))
                    .unwrap(),
            ));
        }
    }

    Err(HttpError::NotFound("media not found".into()))
}

/// DELETE /api/media/:id — soft-delete a media file (owner only).
/// Tries local first, then cloud.
pub async fn delete_media(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    // Try local first
    if let Some(store) = state.media_local.as_ref() {
        match store.delete(&auth.0, &id).await {
            Ok(()) => return Ok(StatusCode::NO_CONTENT),
            Err(e) => {
                // If local didn't find it, fall through to cloud
                if !matches!(e, mini_social_feed_data::DataError::NotFound(_)) {
                    return Err(e.into());
                }
            }
        }
    }

    // Fall back to cloud
    if let Some(store) = state.media_cloud.as_ref() {
        store.delete(&auth.0, &id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    Err(HttpError::NotFound("media not found".into()))
}
