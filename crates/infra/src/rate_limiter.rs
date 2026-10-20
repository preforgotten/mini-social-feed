use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use mini_social_feed_data::ActionLimit;
use tokio::sync::RwLock;

/// Token-bucket rate limiter. Thread-safe, single-instance.
///
/// Keys are `scope:id:action` (e.g. `ip:10.0.0.1:login`).
/// A background task sweeps stale buckets periodically.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    buckets: HashMap<String, Bucket>,
    last_sweep: Instant,
}

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                buckets: HashMap::new(),
                last_sweep: Instant::now(),
            })),
        }
    }

    /// Check the bucket for `key` against `limit`. Returns true if allowed.
    /// Deducts 1 token if allowed.
    pub async fn check(&self, key: &str, limit: &ActionLimit) -> bool {
        let now = Instant::now();
        let mut inner = self.inner.write().await;

        // Periodic sweep: remove buckets unused for 60 seconds
        if now.duration_since(inner.last_sweep).as_secs() > 60 {
            inner
                .buckets
                .retain(|_, b| now.duration_since(b.last_refill).as_secs() < 60);
            inner.last_sweep = now;
        }

        let bucket = inner
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| Bucket {
                tokens: limit.max_burst,
                last_refill: now,
            });

        // Refill
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * limit.refill_per_sec).min(limit.max_burst);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// How many seconds until the bucket refills to at least 1 token.
    pub async fn retry_after(&self, key: &str, limit: &ActionLimit) -> f64 {
        let inner = self.inner.read().await;
        let tokens = inner
            .buckets
            .get(key)
            .map(|b| b.tokens)
            .unwrap_or(limit.max_burst);
        if tokens >= 1.0 {
            0.0
        } else {
            ((1.0 - tokens) / limit.refill_per_sec).ceil().max(1.0)
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
