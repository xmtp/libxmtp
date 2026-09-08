use xmtp_common::time::{Duration, Instant};

#[cfg(test)]
mod tests;

pub(super) struct Bucket {
    rate: f64,
    capacity: f64,
    tokens: f64,
    updated: Instant,
}
impl Bucket {
    pub fn new(rate: u32, burst: u32) -> Self {
        Self {
            rate: rate as f64,
            capacity: burst as f64,
            tokens: burst as f64,
            updated: Instant::now(),
        }
    }
    pub fn take(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.updated).as_secs_f64() * self.rate)
            .min(self.capacity);
        self.updated = now;
        if self.tokens < 1.0 {
            false
        } else {
            self.tokens -= 1.0;
            true
        }
    }
}

pub(super) struct Challenge {
    pub nonce: u64,
    pub handed: tokio::sync::oneshot::Receiver<Instant>,
    pub deadline: Option<Instant>,
}

impl Challenge {
    /// Start the deadline at transport handoff. Reject an unrepresentable clock
    /// value instead of panicking the session task on an extreme configuration.
    pub fn start_deadline(&mut self, sent: Instant, wait: Duration) -> Result<(), tonic::Status> {
        self.deadline = Some(
            sent.checked_add(wait)
                .ok_or_else(|| tonic::Status::internal("pong deadline exceeds the clock range"))?,
        );
        Ok(())
    }
}
