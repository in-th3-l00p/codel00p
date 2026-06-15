//! Bounded retry-with-backoff for a single inference route.
//!
//! Transient provider failures (rate limits, overload) are retried on the same
//! route before falling back, using exponential backoff with full jitter and
//! honoring a provider `Retry-After` hint when present. Delays are capped and the
//! attempt count is bounded so a stuck provider degrades quickly to a fallback
//! route rather than hanging.

use std::time::Duration;

/// Retry policy applied per route by the [`crate::InferenceClient`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum retries after the first attempt (so total attempts ≤ `max_retries
    /// + 1`). Zero disables retrying.
    max_retries: u32,
    /// Base delay for the first backoff step (doubled each subsequent attempt).
    base_delay: Duration,
    /// Upper bound on any single delay, including a `Retry-After` hint.
    max_delay: Duration,
}

impl Default for RetryPolicy {
    /// A conservative default: two retries, 500 ms base, 20 s cap — enough to ride
    /// out a brief rate limit without making an interactive turn feel stuck.
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(20),
        }
    }
}

impl RetryPolicy {
    pub fn new(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay,
        }
    }

    /// Disables retrying (one attempt, no backoff).
    pub fn disabled() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Whether a retryable failure should be retried at this 0-based `attempt`
    /// index (0 = the failure of the first try).
    pub fn should_retry(&self, attempt: u32, retryable: bool) -> bool {
        retryable && attempt < self.max_retries
    }

    /// The delay before the retry following 0-based `attempt`.
    ///
    /// A `retry_after` hint wins (capped at `max_delay`). Otherwise the delay is
    /// exponential — `base_delay * 2^attempt`, capped at `max_delay` — scaled by
    /// `jitter`, a fraction in `[0, 1]` (full jitter). Callers pass a random
    /// fraction in production and a fixed one in tests.
    pub fn delay(&self, attempt: u32, retry_after: Option<Duration>, jitter: f64) -> Duration {
        if let Some(hint) = retry_after {
            return hint.min(self.max_delay);
        }
        let factor = 2u32.saturating_pow(attempt.min(16));
        let capped = self.base_delay.saturating_mul(factor).min(self.max_delay);
        capped.mul_f64(jitter.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_retry_respects_bound_and_retryability() {
        let policy = RetryPolicy::new(2, Duration::from_millis(10), Duration::from_secs(1));
        assert!(policy.should_retry(0, true));
        assert!(policy.should_retry(1, true));
        assert!(!policy.should_retry(2, true)); // bound reached
        assert!(!policy.should_retry(0, false)); // not retryable
        assert!(!RetryPolicy::disabled().should_retry(0, true));
    }

    #[test]
    fn exponential_backoff_doubles_and_caps() {
        let policy = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_millis(800));
        // jitter = 1.0 yields the full (un-jittered) delay.
        assert_eq!(policy.delay(0, None, 1.0), Duration::from_millis(100));
        assert_eq!(policy.delay(1, None, 1.0), Duration::from_millis(200));
        assert_eq!(policy.delay(2, None, 1.0), Duration::from_millis(400));
        // 100 * 2^3 = 800 == cap; 2^4 would exceed and stays capped.
        assert_eq!(policy.delay(3, None, 1.0), Duration::from_millis(800));
        assert_eq!(policy.delay(4, None, 1.0), Duration::from_millis(800));
    }

    #[test]
    fn jitter_scales_within_bounds() {
        let policy = RetryPolicy::new(3, Duration::from_millis(100), Duration::from_secs(10));
        assert_eq!(policy.delay(0, None, 0.0), Duration::ZERO);
        assert_eq!(policy.delay(0, None, 0.5), Duration::from_millis(50));
        // Out-of-range jitter is clamped, never exceeding the capped delay.
        assert_eq!(policy.delay(0, None, 2.0), Duration::from_millis(100));
    }

    #[test]
    fn retry_after_hint_wins_and_is_capped() {
        let policy = RetryPolicy::new(3, Duration::from_millis(100), Duration::from_secs(5));
        // Hint is honored exactly when under the cap (jitter is ignored).
        assert_eq!(
            policy.delay(0, Some(Duration::from_secs(2)), 0.0),
            Duration::from_secs(2)
        );
        // A hint beyond the cap is clamped to the cap.
        assert_eq!(
            policy.delay(0, Some(Duration::from_secs(60)), 1.0),
            Duration::from_secs(5)
        );
    }
}
