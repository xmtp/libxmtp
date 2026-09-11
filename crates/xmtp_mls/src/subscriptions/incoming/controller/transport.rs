use super::*;

/// One receiver lifecycle. Requested topics are not proof of registration.
pub(super) struct Transport {
    pub(super) factory: Option<Arc<dyn SubscriptionFactory>>,
    pub(super) state: TransportState,
    pub(super) requested: HashSet<Topic>,
    pub(super) registered: HashSet<Topic>,
    pub(super) generation: u64,
    pub(super) error: Option<Arc<IncomingError>>,
}

pub(super) enum TransportState {
    Waiting(Instant),
    Opening(OpenFuture),
    Streaming(IncomingSubscription<NetworkError>),
    Unary,
    Failed,
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
        }
    }

    /// Cancel an obsolete receiver without resetting backoff or terminal failure.
    pub(super) fn request(&mut self, topics: HashSet<Topic>) {
        if self.requested == topics {
            return;
        }
        self.requested = topics;
        self.registered.clear();
        if !matches!(
            self.state,
            TransportState::Waiting(_) | TransportState::Failed
        ) {
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

    pub(super) fn is_failed(&self) -> bool {
        matches!(self.state, TransportState::Failed)
    }

    pub(super) fn backing_off(&self) -> bool {
        matches!(self.state, TransportState::Waiting(at) if Instant::now() < at)
    }

    /// A wake can advance a retry. It cannot revive a terminal failure.
    pub(super) fn wake(&mut self) {
        if let TransportState::Waiting(at) = &mut self.state {
            *at = Instant::now();
        }
    }

    /// Drop all uncommitted wire progress. The next open starts from durable receipt.
    pub(super) fn disconnect(&mut self, delay: Duration) {
        self.registered.clear();
        if !self.is_failed() {
            self.state = TransportState::Waiting(Instant::now() + delay);
        }
    }

    pub(super) fn fail(&mut self, error: NetworkError, delay: Duration) {
        self.disconnect(delay);
        if !error.is_retryable() {
            self.state = TransportState::Failed;
        }
        self.error = Some(Arc::new(error.into()));
    }

    pub(super) fn connection(&self) -> IncomingConnection {
        match self.state {
            TransportState::Failed => IncomingConnection::Failed,
            TransportState::Streaming(_) | TransportState::Unary => IncomingConnection::Connected,
            TransportState::Opening(_) if self.generation == 1 => IncomingConnection::Connecting,
            TransportState::Waiting(_) if self.generation == 0 => IncomingConnection::Connecting,
            _ => IncomingConnection::Reconnecting,
        }
    }
}
