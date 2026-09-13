use xmtp_common::time::{Duration, Instant};

#[cfg(test)]
mod tests;

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
