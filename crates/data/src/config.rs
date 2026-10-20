use crate::RateLimitConfig;
use serde::Deserialize;

/// Application configuration. Reads from a TOML file (default: config.toml).
/// Secrets (JWT signing key, S3 credentials, etc.) are NEVER in this file —
/// they must be set via environment variables at startup.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub auth: AuthConfig,

    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub media: MediaConfig,

    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

#[allow(clippy::derivable_impls)]
impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            auth: AuthConfig::default(),
            database: DatabaseConfig::default(),
            media: MediaConfig::default(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// JWT secret is NOT read from config.toml — must be set via JWT_SECRET env var.
    /// This field is ignored; exists only for documentation.
    #[serde(default, skip_deserializing)]
    pub _jwt_secret_use_env_var: (),
}

#[allow(clippy::derivable_impls)]
impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            _jwt_secret_use_env_var: (),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_backend")]
    pub backend: String,

    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: default_db_backend(),
            sqlite_path: default_sqlite_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaConfig {
    #[serde(default)]
    pub local: LocalMediaConfig,
    #[serde(default)]
    pub s3: S3MediaConfig,
}

#[allow(clippy::derivable_impls)]
impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            local: LocalMediaConfig::default(),
            s3: S3MediaConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalMediaConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_media_policy")]
    pub policy: String,
    #[serde(default = "default_media_local_path")]
    pub path: String,
}

impl Default for LocalMediaConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            policy: default_media_policy(),
            path: default_media_local_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3MediaConfig {
    #[serde(default)]
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub region: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for S3MediaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            bucket: None,
            region: None,
        }
    }
}

// ── defaults ──

fn default_true() -> bool {
    true
}
fn default_host() -> String {
    "0.0.0.0".into()
}
fn default_port() -> u16 {
    3000
}
fn default_db_backend() -> String {
    "sqlite".into()
}
fn default_sqlite_path() -> String {
    "data/app.db".into()
}
fn default_media_policy() -> String {
    "standard".into()
}
fn default_media_local_path() -> String {
    "uploads".into()
}

impl Config {
    /// Load from a TOML file path. Fails if the file doesn't exist.
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
