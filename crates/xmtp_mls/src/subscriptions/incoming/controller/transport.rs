use super::*;

/// Doubling delay for a source that keeps returning permanent errors.
#[derive(Debug, Clone, Copy)]
pub(super) struct RetryBackoff {
    pub(super) initial: Duration,
    pub(super) max: Duration,
}

impl RetryBackoff {
    /// `failures` counts consecutive permanent errors and starts at one.
    fn delay(&self, failures: u32) -> Duration {
        let shift = failures.saturating_sub(1).min(u32::BITS - 1);
        self.initial
            .checked_mul(1u32 << shift)
            .unwrap_or(self.max)
            .min(self.max)
    }
}

/// One receiver lifecycle. Requested topics are not proof of registration.
pub(super) struct Transport {
    pub(super) factory: Option<Arc<dyn SubscriptionFactory>>,
    pub(super) state: TransportState,
    pub(super) requested: HashSet<Topic>,
    pub(super) registered: HashSet<Topic>,
    pub(super) generation: u64,
    pub(super) error: Option<Arc<IncomingError>>,
    /// Consecutive permanent failures. Only the retry delay grows with it.
    pub(super) permanent_failures: u32,
}

pub(super) enum TransportState {
    Waiting(Instant),
    Opening(OpenFuture),
    Streaming(IncomingSubscription<NetworkError>),
    Unary,
}

pub(super) enum TransportEvent {
    Opened(Result<Opened, NetworkError>),
    Incoming(Option<Result<IncomingEvent, NetworkError>>),
}

impl Transport {
    pub(super) fn new(factory: Option<Arc<dyn SubscriptionFactory>>) -> Self {
        Self {
            factory,
            state: TransportState::Waiting(Instant::now()),
            requested: HashSet::new(),
            registered: HashSet::new(),
            generation: 0,
            error: None,
            permanent_failures: 0,
        }
    }

    /// Cancel an obsolete receiver without resetting a pending retry delay.
    pub(super) fn request(&mut self, topics: HashSet<Topic>) {
        if self.requested == topics {
            return;
        }
        self.requested = topics;
        self.registered.clear();
        if !matches!(self.state, TransportState::Waiting(_)) {
            self.state = TransportState::Waiting(Instant::now());
        }
    }

    pub(super) fn can_open(&self) -> bool {
        !self.requested.is_empty()
            && match self.state {
                TransportState::Waiting(at) => Instant::now() >= at,
                TransportState::Unary => self.registered != self.requested,
                _ => false,
            }
    }

    pub(super) fn start(&mut self, future: OpenFuture) {
        self.generation += 1;
        self.state = TransportState::Opening(future);
    }

    pub(super) async fn next(&mut self) -> TransportEvent {
        match &mut self.state {
            TransportState::Opening(future) => TransportEvent::Opened(future.await),
            TransportState::Streaming(subscription) => {
                TransportEvent::Incoming(subscription.events.next().await)
            }
            _ => futures::future::pending().await,
        }
    }

    #[cfg(test)]
    pub(super) fn is_opening(&self) -> bool {
        matches!(self.state, TransportState::Opening(_))
    }

    pub(super) fn subscription(&self) -> Option<&IncomingSubscription<NetworkError>> {
        match &self.state {
            TransportState::Streaming(subscription) => Some(subscription),
            _ => None,
        }
    }

    pub(super) fn backing_off(&self) -> bool {
        matches!(self.state, TransportState::Waiting(at) if Instant::now() < at)
    }

    #[cfg(test)]
    pub(super) fn retry_at(&self) -> Option<Instant> {
        match self.state {
            TransportState::Waiting(at) => Some(at),
            _ => None,
        }
    }

    /// A wake advances a pending retry, including one scheduled after a
    /// permanent error. No failure is terminal.
    pub(super) fn wake(&mut self) {
        if let TransportState::Waiting(at) = &mut self.state {
            *at = Instant::now();
        }
    }

    /// Drop all uncommitted wire progress. The next open starts from durable receipt.
    pub(super) fn disconnect(&mut self, delay: Duration) {
        self.registered.clear();
        self.state = TransportState::Waiting(Instant::now() + delay);
    }

    /// A permanent classification describes one response, not the source. The
    /// receiver keeps retrying on a growing delay so a server that repairs
    /// itself is picked up without recreating the client. Skipping an envelope
    /// is never the recovery: only the delay changes.
    pub(super) fn fail(&mut self, error: NetworkError, delay: Duration, backoff: RetryBackoff) {
        let retryable = error.is_retryable();
        let delay = if retryable {
            self.permanent_failures = 0;
            delay
        } else {
            self.permanent_failures = self.permanent_failures.saturating_add(1);
            backoff.delay(self.permanent_failures)
        };
        tracing::warn!(
            error = %error,
            retryable,
            permanent_failures = self.permanent_failures,
            retry_in_ms = delay.as_millis() as u64,
            "incoming receiver failed"
        );
        self.disconnect(delay);
        self.error = Some(Arc::new(error.into()));
    }

    /// Clear the permanent-failure streak after the source proves it works.
    pub(super) fn opened(&mut self) {
        self.permanent_failures = 0;
    }

    pub(super) fn connection(&self) -> IncomingConnection {
        match self.state {
            // Report a repeatedly failing source as failed so hosts can surface
            // it, while the receiver keeps retrying underneath.
            _ if self.permanent_failures > 0
                && !matches!(
                    self.state,
                    TransportState::Streaming(_) | TransportState::Unary
                ) =>
            {
                IncomingConnection::Failed
            }
            TransportState::Streaming(_) | TransportState::Unary => IncomingConnection::Connected,
            TransportState::Opening(_) if self.generation == 1 => IncomingConnection::Connecting,
            TransportState::Waiting(_) if self.generation == 0 => IncomingConnection::Connecting,
            _ => IncomingConnection::Reconnecting,
        }
    }
}
