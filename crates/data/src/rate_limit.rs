use serde::Deserialize;
use std::collections::HashMap;

/// Rate limiting configuration — per-profile tiered action limits.
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub public: ActionLimits,

    #[serde(default)]
    pub human: TieredLimits,

    #[serde(default)]
    pub agent: TieredLimits,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            public: ActionLimits::default(),
            human: TieredLimits::default(),
            agent: TieredLimits::default(),
        }
    }
}

/// Flat limits for unauthenticated endpoints (no tier).
#[derive(Debug, Clone, Deserialize)]
pub struct ActionLimits {
    #[serde(default)]
    pub actions: HashMap<String, ActionLimit>,
}

impl Default for ActionLimits {
    fn default() -> Self {
        let mut actions = HashMap::new();
        actions.insert("register".into(), ActionLimit::new(3.0, 0.1));
        actions.insert("login".into(), ActionLimit::new(5.0, 0.2));
        actions.insert("read".into(), ActionLimit::new(20.0, 2.0));
        Self { actions }
    }
}

/// Tiered limits — per-tier action buckets for authenticated profiles.
#[derive(Debug, Clone, Deserialize)]
pub struct TieredLimits {
    #[serde(default = "default_tier_name")]
    pub default_tier: String,

    #[serde(default)]
    pub tiers: HashMap<String, TierLimits>,
}

impl Default for TieredLimits {
    fn default() -> Self {
        let mut tiers = HashMap::new();
        tiers.insert("minimal".into(), TierLimits::minimal());
        tiers.insert("basic".into(), TierLimits::basic());
        tiers.insert("power".into(), TierLimits::power());
        Self {
            default_tier: "minimal".into(),
            tiers,
        }
    }
}

/// All action limits for one tier.
#[derive(Debug, Clone, Deserialize)]
pub struct TierLimits {
    #[serde(default)]
    pub actions: HashMap<String, ActionLimit>,
}

impl TierLimits {
    pub fn minimal() -> Self {
        let mut actions = HashMap::new();
        actions.insert("login".into(), ActionLimit::new(3.0, 0.1));
        actions.insert("post_create".into(), ActionLimit::new(5.0, 0.25));
        actions.insert("media_upload".into(), ActionLimit::new(3.0, 0.25));
        actions.insert("read".into(), ActionLimit::new(30.0, 5.0));
        actions.insert("write".into(), ActionLimit::new(10.0, 1.0));
        Self { actions }
    }

    pub fn basic() -> Self {
        let mut actions = HashMap::new();
        actions.insert("login".into(), ActionLimit::new(5.0, 0.2));
        actions.insert("post_create".into(), ActionLimit::new(20.0, 1.0));
        actions.insert("media_upload".into(), ActionLimit::new(10.0, 0.5));
        actions.insert("read".into(), ActionLimit::new(100.0, 20.0));
        actions.insert("write".into(), ActionLimit::new(30.0, 5.0));
        Self { actions }
    }

    pub fn power() -> Self {
        let mut actions = HashMap::new();
        actions.insert("login".into(), ActionLimit::new(10.0, 0.5));
        actions.insert("post_create".into(), ActionLimit::new(50.0, 5.0));
        actions.insert("media_upload".into(), ActionLimit::new(30.0, 3.0));
        actions.insert("read".into(), ActionLimit::new(200.0, 50.0));
        actions.insert("write".into(), ActionLimit::new(100.0, 20.0));
        Self { actions }
    }
}

/// Single token bucket parameters.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ActionLimit {
    pub max_burst: f64,
    pub refill_per_sec: f64,
}

impl ActionLimit {
    pub const fn new(max_burst: f64, refill_per_sec: f64) -> Self {
        Self {
            max_burst,
            refill_per_sec,
        }
    }
}

// ── helpers ──

fn default_true() -> bool {
    true
}

fn default_tier_name() -> String {
    "minimal".into()
}
