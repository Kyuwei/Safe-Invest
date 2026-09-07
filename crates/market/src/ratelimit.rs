//! A token bucket per provider.
//!
//! Free API tiers ban callers that hammer them. Rather than discovering that
//! through a 429 storm, each provider declares its budget and the bucket makes
//! the app stay inside it — hand-rolled because the shape is twenty lines and
//! a rate limiter is not worth a dependency tree.

use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    /// Tokens added per second.
    refill_rate: f64,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// A bucket holding `per_minute` requests, refilling smoothly over a minute.
    pub fn per_minute(per_minute: u32) -> Self {
        let capacity = f64::from(per_minute.max(1));
        Self {
            capacity,
            refill_rate: capacity / 60.0,
            state: Mutex::new(State {
                tokens: capacity,
                last_refill: Instant::now(),
            }),
        }
    }

    /// Takes one token, or reports how long the caller would have to wait.
    ///
    /// Deliberately non-blocking: a source that has run out should be skipped
    /// in favour of the next one in the chain, not waited on while the user
    /// stares at a spinner.
    pub async fn try_take(&self) -> Result<(), Duration> {
        let mut state = self.state.lock().await;
        let now = Instant::now();

        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        state.tokens = (state.tokens + elapsed * self.refill_rate).min(self.capacity);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            return Ok(());
        }

        let missing = 1.0 - state.tokens;
        Err(Duration::from_secs_f64(missing / self.refill_rate))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unchecked_time_subtraction,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_bucket_allows_its_whole_budget_then_stops() {
        let bucket = TokenBucket::per_minute(3);
        for _ in 0..3 {
            bucket.try_take().await.unwrap();
        }
        let wait = bucket.try_take().await.unwrap_err();
        assert!(wait > Duration::ZERO, "le quota doit finir par bloquer");
    }

    #[tokio::test]
    async fn tokens_come_back_over_time() {
        let bucket = TokenBucket::per_minute(60); // one per second
        bucket.try_take().await.unwrap();

        // Rewind the clock rather than sleeping: the test stays instant.
        {
            let mut state = bucket.state.lock().await;
            state.tokens = 0.0;
            state.last_refill = Instant::now() - Duration::from_secs(5);
        }

        assert!(bucket.try_take().await.is_ok());
    }
}
