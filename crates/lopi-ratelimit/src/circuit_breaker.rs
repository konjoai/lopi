use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Observable state of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal — requests pass through.
    Closed,
    /// Tripped — requests are rejected without forwarding.
    Open,
    /// Recovery probe — one request is allowed through to test the downstream.
    HalfOpen,
}

/// Errors returned by `CircuitBreaker::check()`.
#[derive(Debug, thiserror::Error)]
pub enum BreakerError {
    /// Breaker is in the Open state — the request was rejected without forwarding.
    #[error("circuit breaker is open — service unavailable")]
    Open,
    /// Hourly cost cap exceeded — no further calls allowed until the window resets.
    #[error("hourly cost cap exceeded: ${cap:.2}/hr")]
    CostCapExceeded {
        /// The configured hourly USD cap that was exceeded.
        cap: f64,
    },
}

/// Adaptive circuit breaker combining failure counting with a per-hour cost cap.
///
/// Two independent trip conditions:
/// 1. Consecutive failures ≥ `failure_threshold` → Open for `open_duration`.
/// 2. Accumulated cost this hour ≥ `cost_per_hour_limit` → Open until hourly reset.
///
/// After `open_duration` elapses, the breaker transitions to `HalfOpen` and allows
/// one probe request through. `record_success()` closes it; `record_failure()` reopens it.
pub struct CircuitBreaker {
    inner: Arc<Mutex<BreakerInner>>,
}

struct BreakerInner {
    state: BreakerState,
    failure_count: u32,
    failure_threshold: u32,
    last_failure: Option<Instant>,
    open_duration: Duration,
    cost_per_hour_limit: f64,
    cost_this_hour: f64,
    hour_start: Instant,
    /// True while a `HalfOpen` probe request is outstanding. Gates
    /// `HalfOpen` to exactly one in-flight caller — without it, every
    /// caller that observes `HalfOpen` is let through, defeating the
    /// point of a single recovery probe.
    probe_in_flight: bool,
}

impl CircuitBreaker {
    /// Create a new breaker.
    ///
    /// - `failure_threshold`: consecutive failures before tripping.
    /// - `open_duration`: how long to stay Open before moving to `HalfOpen`.
    /// - `cost_per_hour_limit`: USD/hr cap — breaker trips when exceeded.
    #[must_use]
    pub fn new(failure_threshold: u32, open_duration: Duration, cost_per_hour_limit: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                failure_count: 0,
                failure_threshold,
                last_failure: None,
                open_duration,
                cost_per_hour_limit,
                cost_this_hour: 0.0,
                hour_start: Instant::now(),
                probe_in_flight: false,
            })),
        }
    }

    /// Returns `Ok(())` if the request should proceed, or an error explaining why not.
    ///
    /// # Errors
    ///
    /// Returns `Err(BreakerError::Open)` when the breaker is open.
    /// Returns `Err(BreakerError::CostCapExceeded)` when the hourly cost cap is exceeded.
    pub async fn check(&self) -> Result<(), BreakerError> {
        let mut inner = self.inner.lock().await;
        // Reset hourly cost counter on the hour boundary.
        if inner.hour_start.elapsed() >= Duration::from_secs(3600) {
            inner.cost_this_hour = 0.0;
            inner.hour_start = Instant::now();
        }
        if inner.cost_this_hour >= inner.cost_per_hour_limit {
            return Err(BreakerError::CostCapExceeded {
                cap: inner.cost_per_hour_limit,
            });
        }
        match inner.state {
            BreakerState::Open => {
                if let Some(t) = inner.last_failure {
                    if t.elapsed() >= inner.open_duration {
                        inner.state = BreakerState::HalfOpen;
                        inner.probe_in_flight = true;
                        return Ok(());
                    }
                }
                Err(BreakerError::Open)
            }
            // Only the single caller that flipped Open -> HalfOpen (above)
            // gets through; every other caller sees probe_in_flight and is
            // rejected until record_success/record_failure resolves it.
            BreakerState::HalfOpen => {
                if inner.probe_in_flight {
                    Err(BreakerError::Open)
                } else {
                    inner.probe_in_flight = true;
                    Ok(())
                }
            }
            BreakerState::Closed => Ok(()),
        }
    }

    /// Call after a successful downstream response. Resets failure count and closes the breaker.
    pub async fn record_success(&self) {
        let mut inner = self.inner.lock().await;
        inner.failure_count = 0;
        inner.state = BreakerState::Closed;
        inner.probe_in_flight = false;
    }

    /// Call after a failed downstream response. May trip the breaker to Open.
    pub async fn record_failure(&self) {
        let mut inner = self.inner.lock().await;
        inner.failure_count += 1;
        inner.last_failure = Some(Instant::now());
        inner.probe_in_flight = false;
        if inner.failure_count >= inner.failure_threshold {
            inner.state = BreakerState::Open;
            tracing::warn!(
                failures = inner.failure_count,
                threshold = inner.failure_threshold,
                "circuit breaker opened due to consecutive failures"
            );
        }
    }

    /// Accumulate cost against the hourly cap. May trip the breaker if the cap is exceeded.
    pub async fn record_cost(&self, usd: f64) {
        let mut inner = self.inner.lock().await;
        inner.cost_this_hour += usd;
        if inner.cost_this_hour >= inner.cost_per_hour_limit {
            inner.state = BreakerState::Open;
            inner.last_failure = Some(Instant::now());
            inner.probe_in_flight = false;
            tracing::warn!(
                cost = inner.cost_this_hour,
                cap = inner.cost_per_hour_limit,
                "circuit breaker opened: hourly cost cap exceeded"
            );
        }
    }

    /// Read the current breaker state without modifying it.
    pub async fn state(&self) -> BreakerState {
        self.inner.lock().await.state
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30), 10.0);
        assert_eq!(cb.state().await, BreakerState::Closed);
        assert!(cb.check().await.is_ok());
    }

    #[tokio::test]
    async fn trips_on_failure_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30), 10.0);
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, BreakerState::Closed);
        cb.record_failure().await;
        assert_eq!(cb.state().await, BreakerState::Open);
        assert!(matches!(cb.check().await, Err(BreakerError::Open)));
    }

    #[tokio::test]
    async fn success_resets_failure_count() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30), 10.0);
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_success().await;
        assert_eq!(cb.state().await, BreakerState::Closed);
        // Next two failures should not trip (counter reset).
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, BreakerState::Closed);
    }

    #[tokio::test]
    async fn trips_on_cost_cap() {
        let cb = CircuitBreaker::new(10, Duration::from_secs(30), 5.0);
        cb.record_cost(3.0).await;
        assert_eq!(cb.state().await, BreakerState::Closed);
        cb.record_cost(2.5).await;
        assert_eq!(cb.state().await, BreakerState::Open);
        assert!(matches!(
            cb.check().await,
            Err(BreakerError::CostCapExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn cost_cap_error_message_contains_cap() {
        let cb = CircuitBreaker::new(10, Duration::from_secs(30), 7.5);
        cb.record_cost(8.0).await;
        match cb.check().await {
            Err(BreakerError::CostCapExceeded { cap }) => {
                assert!((cap - 7.5).abs() < f64::EPSILON);
            }
            other => panic!("expected CostCapExceeded, got: {other:?}"),
        }
    }

    /// Regression test for the original bug: once the breaker flips
    /// Open -> HalfOpen, every concurrent caller used to see `HalfOpen =>
    /// Ok(())` and pass through — not just the single recovery probe the
    /// state is meant to allow. Fires many concurrent callers the instant
    /// `open_duration` elapses and asserts exactly one gets through.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn half_open_allows_exactly_one_probe() {
        let cb = Arc::new(CircuitBreaker::new(1, Duration::from_millis(20), 100.0));
        cb.record_failure().await; // threshold=1, trips immediately.
        assert_eq!(cb.state().await, BreakerState::Open);

        tokio::time::sleep(Duration::from_millis(40)).await;

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let cb = Arc::clone(&cb);
                tokio::spawn(async move { cb.check().await.is_ok() })
            })
            .collect();

        let mut ok_count = 0;
        for h in handles {
            if h.await.expect("task panicked") {
                ok_count += 1;
            }
        }
        assert_eq!(
            ok_count, 1,
            "exactly one HalfOpen probe should be let through concurrently"
        );
    }
}
