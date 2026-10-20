use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use mini_social_feed_domain::{MediaRef, UserId};

use crate::error::DataError;
use crate::media_policy::MediaPolicy;
use crate::ports::MediaStore;

#[derive(Debug, Serialize, Deserialize)]
struct MediaMeta {
    id: String,
    user_id: String,
    content_type: String,
    size_bytes: u64,
    deleted: bool,
}

/// Local disk media storage. Files go to `{root}/files/{user_id}/{media_id}.{ext}`.
/// Metadata stored as `{root}/meta/{media_id}.json`.
/// Deleted files get a `.deleted` marker — the binary stays on disk.
#[derive(Clone)]
pub struct LocalMediaStore {
    root: PathBuf,
    policy: MediaPolicy,
}

impl LocalMediaStore {
    pub fn new(root: PathBuf, policy: MediaPolicy) -> Self {
        std::fs::create_dir_all(root.join("meta")).ok();
        std::fs::create_dir_all(root.join("files")).ok();
        Self { root, policy }
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.root.join("meta").join(format!("{id}.json"))
    }

    fn file_path(&self, user_id: &str, id: &str, ext: &str) -> PathBuf {
        let dir = self.root.join("files").join(user_id);
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{id}.{ext}"))
    }

    fn read_meta(&self, id: &str) -> Option<MediaMeta> {
        let path = self.meta_path(id);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    fn write_meta(&self, meta: &MediaMeta) -> Result<(), DataError> {
        let path = self.meta_path(&meta.id);
        let json = serde_json::to_string(meta).map_err(|e| DataError::Internal(e.to_string()))?;
        std::fs::write(&path, json).map_err(|e| DataError::Internal(e.to_string()))
    }
}

#[async_trait]
impl MediaStore for LocalMediaStore {
    async fn upload(
        &self,
        user_id: &UserId,
        data: Vec<u8>,
        _filename: &str,
        content_type: &str,
    ) -> Result<MediaRef, DataError> {
        // Policy check: content type — must match either post_image or avatar policy
        let post_ok = self
            .policy
            .post_image
            .allowed_types
            .iter()
            .any(|t| t == content_type);
        let avatar_ok = self
            .policy
            .avatar
            .allowed_types
            .iter()
            .any(|t| t == content_type);
        if !post_ok && !avatar_ok {
            return Err(DataError::BadRequest(format!(
                "media type not allowed by policy: {content_type}"
            )));
        }

        // Policy check: size — must fit within either post_image or avatar limits
        let post_size_ok = (data.len() as u64) <= self.policy.post_image.max_file_size;
        let avatar_size_ok = (data.len() as u64) <= self.policy.avatar.max_file_size;
        if !post_size_ok && !avatar_size_ok {
            return Err(DataError::BadRequest(format!(
                "file too large: {} bytes (max post_image: {}, max avatar: {})",
                data.len(),
                self.policy.post_image.max_file_size,
                self.policy.avatar.max_file_size
            )));
        }

        let ext = content_type.split('/').next_back().unwrap_or("bin");
        let id = uuid::Uuid::new_v4().to_string();
        let file_path = self.file_path(&user_id.0, &id, ext);
        std::fs::write(&file_path, &data).map_err(|e| DataError::Internal(e.to_string()))?;

        let meta = MediaMeta {
            id: id.clone(),
            user_id: user_id.0.clone(),
            content_type: content_type.to_string(),
            size_bytes: data.len() as u64,
            deleted: false,
        };
        self.write_meta(&meta)?;

        Ok(MediaRef {
            id,
            url: format!("/api/media/{}", meta.id),
            backend: "local".into(),
            content_type: content_type.to_string(),
            size_bytes: data.len() as u64,
            deleted: false,
        })
    }

    async fn get_file(&self, id: &str) -> Result<Option<(Vec<u8>, String)>, DataError> {
        let meta = self.read_meta(id);
        match meta {
            Some(m) if !m.deleted => {
                let ext = m.content_type.split('/').next_back().unwrap_or("bin");
                let path = self.file_path(&m.user_id, id, ext);
                match std::fs::read(&path) {
                    Ok(data) => Ok(Some((data, m.content_type))),
                    Err(_) => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    async fn delete(&self, user_id: &UserId, id: &str) -> Result<(), DataError> {
        let mut meta = self
            .read_meta(id)
            .ok_or_else(|| DataError::NotFound("media not found".into()))?;

        if meta.user_id != user_id.0 {
            return Err(DataError::BadRequest("not the media owner".into()));
        }

        meta.deleted = true;
        self.write_meta(&meta)?;

        let ext = meta.content_type.split('/').next_back().unwrap_or("bin");
        let file_path = self.file_path(&meta.user_id, id, ext);
        let deleted_path = self.file_path(&meta.user_id, id, &format!("{ext}.deleted"));
        std::fs::rename(&file_path, &deleted_path).ok();

        Ok(())
    }

    async fn get_url(&self, id: &str) -> Result<Option<String>, DataError> {
        let meta = self.read_meta(id);
        match meta {
            Some(m) if !m.deleted => Ok(Some(format!("/api/media/{id}"))),
            _ => Ok(None),
        }
    }

    async fn resolve_batch(&self, ids: &[String]) -> Result<Vec<MediaRef>, DataError> {
        let mut refs = Vec::new();
        for id in ids {
            if let Some(meta) = self.read_meta(id) {
                refs.push(MediaRef {
                    id: id.clone(),
                    url: if meta.deleted {
                        String::new()
                    } else {
                        format!("/api/media/{id}")
                    },
                    backend: "local".into(),
                    content_type: meta.content_type,
                    size_bytes: meta.size_bytes,
                    deleted: meta.deleted,
                });
            }
        }
        Ok(refs)
    }
}
