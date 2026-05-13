pub mod budget;
pub mod circuit_breaker;

pub use budget::{BudgetConfig, BudgetError, BudgetGovernor, BudgetLimit, BudgetStates};
pub use circuit_breaker::{BreakerError, BreakerState, CircuitBreaker};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Token-bucket rate limiter with async acquisition.
///
/// The bucket holds up to `capacity` tokens. Tokens refill at `refill_rate` tokens/second.
/// Callers block until enough tokens are available, then consume them atomically.
#[derive(Clone)]
pub struct TokenBucket {
    inner: Arc<Mutex<BucketState>>,
}

struct BucketState {
    tokens: f64,
    capacity: f64,
    /// Tokens added per second.
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    #[must_use]
    pub fn new(capacity: f64, refill_per_second: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BucketState {
                tokens: capacity,
                capacity,
                refill_rate: refill_per_second,
                last_refill: Instant::now(),
            })),
        }
    }

    /// Wait asynchronously until `tokens` tokens are available, then consume them.
    ///
    /// The wait is computed from the deficit and refill rate; we do not spin-poll.
    pub async fn acquire(&self, tokens: f64) {
        loop {
            let wait = {
                let mut state = self.inner.lock().await;
                state.refill();
                if state.tokens >= tokens {
                    state.tokens -= tokens;
                    return;
                }
                let deficit = tokens - state.tokens;
                Duration::from_secs_f64(deficit / state.refill_rate)
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// Try to acquire without waiting. Returns `false` if insufficient tokens.
    pub async fn try_acquire(&self, tokens: f64) -> bool {
        let mut state = self.inner.lock().await;
        state.refill();
        if state.tokens >= tokens {
            state.tokens -= tokens;
            true
        } else {
            false
        }
    }
}

impl BucketState {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }
}

/// Dual-limiter enforcing both TPM (tokens per minute) and RPM (requests per minute)
/// against the Anthropic API.
///
/// `acquire_request()` concurrently acquires from both buckets to minimise overhead.
#[derive(Clone)]
pub struct AnthropicLimiter {
    tpm: TokenBucket,
    rpm: TokenBucket,
}

impl AnthropicLimiter {
    /// Default Anthropic Pro limits: 120 000 TPM, 15 RPM (concurrent connections).
    #[must_use]
    pub fn default_pro() -> Self {
        Self {
            tpm: TokenBucket::new(120_000.0, 2_000.0), // 120k/min = 2k/sec
            rpm: TokenBucket::new(15.0, 0.25),         // 15/min = 0.25/sec
        }
    }

    #[must_use]
    pub fn custom(tpm_limit: f64, rpm_limit: f64) -> Self {
        Self {
            tpm: TokenBucket::new(tpm_limit, tpm_limit / 60.0),
            rpm: TokenBucket::new(rpm_limit, rpm_limit / 60.0),
        }
    }

    /// Acquire capacity for one request that will consume `estimated_tokens` tokens.
    /// Blocks until both TPM and RPM buckets can satisfy the request.
    pub async fn acquire_request(&self, estimated_tokens: f64) {
        tokio::join!(self.tpm.acquire(estimated_tokens), self.rpm.acquire(1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bucket_starts_full() {
        let bucket = TokenBucket::new(100.0, 10.0);
        assert!(bucket.try_acquire(100.0).await);
    }

    #[tokio::test]
    async fn bucket_depletes() {
        let bucket = TokenBucket::new(10.0, 0.1); // very slow refill
        assert!(bucket.try_acquire(10.0).await);
        assert!(!bucket.try_acquire(1.0).await);
    }

    #[tokio::test]
    async fn bucket_partial_acquire() {
        let bucket = TokenBucket::new(50.0, 10.0);
        assert!(bucket.try_acquire(25.0).await);
        assert!(bucket.try_acquire(25.0).await);
        assert!(!bucket.try_acquire(1.0).await);
    }

    #[tokio::test]
    async fn anthropic_limiter_construction() {
        let _limiter = AnthropicLimiter::default_pro();
        let _limiter2 = AnthropicLimiter::custom(50_000.0, 5.0);
    }

    #[tokio::test]
    async fn limiter_acquire_depletes_both_buckets() {
        let limiter = AnthropicLimiter::custom(100.0, 2.0);
        limiter.acquire_request(50.0).await;
        // RPM bucket had 2 tokens, should be at 1 now.
        let rpm_ok = limiter.rpm.try_acquire(1.0).await;
        assert!(rpm_ok, "RPM bucket should still have 1 token");
    }
}
