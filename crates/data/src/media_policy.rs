/// Per-image-type limits: file size, count per entity, and format constraints.
#[derive(Debug, Clone)]
pub struct ImageLimits {
    pub max_file_size: u64,
    pub max_count: usize,
    pub require_square: bool,
    pub allowed_types: Vec<String>,
}

/// Deployment-level media policy. Controls size limits, per-post/per-avatar
/// counts, and accepted formats.
#[derive(Debug, Clone)]
pub struct MediaPolicy {
    pub post_image: ImageLimits,
    pub avatar: ImageLimits,
    pub max_space_per_user: u64,
    pub max_uploads_per_hour: u32,
}

impl MediaPolicy {
    /// Create from a policy name: "minimal", "standard", or "generous".
    pub fn named(policy: &str) -> Self {
        // Avatar policy is the same across all tiers.
        let avatar = ImageLimits {
            max_file_size: 2 * 1024 * 1024, // 2 MB
            max_count: 1,
            require_square: true,
            allowed_types: vec!["image/jpeg".into(), "image/png".into(), "image/webp".into()],
        };

        match policy {
            "minimal" => Self {
                post_image: ImageLimits {
                    max_file_size: 1024 * 1024, // 1 MB
                    max_count: 1,
                    require_square: false,
                    allowed_types: vec![
                        "image/jpeg".into(),
                        "image/png".into(),
                        "image/webp".into(),
                    ],
                },
                avatar,
                max_space_per_user: 10 * 1024 * 1024, // 10 MB
                max_uploads_per_hour: 5,
            },
            "generous" => Self {
                post_image: ImageLimits {
                    max_file_size: 50 * 1024 * 1024, // 50 MB
                    max_count: 20,
                    require_square: false,
                    allowed_types: vec![
                        "image/jpeg".into(),
                        "image/png".into(),
                        "image/gif".into(),
                        "image/webp".into(),
                        "image/svg+xml".into(),
                    ],
                },
                avatar,
                max_space_per_user: 1024 * 1024 * 1024, // 1 GB
                max_uploads_per_hour: 100,
            },
            _ => Self {
                post_image: ImageLimits {
                    max_file_size: 10 * 1024 * 1024, // 10 MB
                    max_count: 4,
                    require_square: false,
                    allowed_types: vec![
                        "image/jpeg".into(),
                        "image/png".into(),
                        "image/gif".into(),
                        "image/webp".into(),
                    ],
                },
                avatar,
                max_space_per_user: 100 * 1024 * 1024, // 100 MB
                max_uploads_per_hour: 20,
            },
        }
    }
}
