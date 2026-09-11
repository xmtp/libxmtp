//! A token bucket shared by stream admission and client update scheduling.

use crate::time::{Duration, Instant};

/// A burst allowance that refills at a fixed number of tokens per second.
pub struct Bucket {
    rate: f64,
    capacity: f64,
    tokens: f64,
    updated: Instant,
}

impl Bucket {
    /// Start with the full burst allowance. A zero rate disables refill.
    pub fn new(rate: u32, burst: u32) -> Self {
        Self {
            rate: rate as f64,
            capacity: burst as f64,
            tokens: burst as f64,
            updated: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.updated).as_secs_f64() * self.rate)
            .min(self.capacity);
        self.updated = now;
    }

    /// Consume one available token without waiting.
    pub fn take(&mut self) -> bool {
        self.refill();
        if self.tokens < 1.0 {
            false
        } else {
            self.tokens -= 1.0;
            true
        }
    }

    /// Return a token when the transport did not accept the frame.
    pub fn refund(&mut self) {
        self.tokens = (self.tokens + 1.0).min(self.capacity);
    }

    /// Time until one token is available. A disabled bucket never refills.
    pub fn wait(&mut self) -> Duration {
        self.refill();
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else if self.rate == 0.0 || self.capacity < 1.0 {
            Duration::MAX
        } else {
            Duration::from_secs_f64((1.0 - self.tokens) / self.rate)
        }
    }
}
