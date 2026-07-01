//! A capacity-1 "wake the worker" channel: a unit `()` mpsc nudge. Capacity 1
//! because a worker drains and re-checks on every wake, so one pending nudge
//! already means "something changed". `rearm()` is best-effort (full slot or
//! absent consumer drops the send harmlessly). Each worker owns its OWN instance.

use std::sync::Arc;

#[derive(Clone)]
pub struct RearmChannel {
    sender: tokio::sync::mpsc::Sender<()>,
    pub receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>>,
}

impl Default for RearmChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl RearmChannel {
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        Self {
            sender,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
        }
    }

    /// Best-effort wake. A full slot or absent consumer drops the nudge.
    pub fn rearm(&self) {
        let _ = self.sender.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    async fn rearm_delivers_a_signal() {
        let ch = RearmChannel::new();
        ch.rearm();
        let mut rx = ch.receiver.lock().await;
        assert!(rx.try_recv().is_ok());
    }

    #[xmtp_common::test]
    async fn rearm_is_capacity_one_and_lossy() {
        let ch = RearmChannel::new();
        ch.rearm();
        ch.rearm();
        let mut rx = ch.receiver.lock().await;
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }
}
