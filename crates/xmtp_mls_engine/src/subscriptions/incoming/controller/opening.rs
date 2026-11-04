//! Controller opening: opening the stream, registering targets, and the open and
//! target errors.

use super::*;

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn start_open(&mut self) {
        // After a failed reconcile the scopes can be partly updated. Such a pass
        // only keeps or drops the interest of the last completed one; it never adds.
        let failed = self.storage_error.is_some();
        let topics: HashSet<_> = self
            .interested()
            .into_iter()
            .filter(|topic| !self.receipt(topic).paused && !self.receipt(topic).blocked())
            .filter(|topic| !failed || self.transport.requested.contains(topic))
            .collect();
        self.transport.request(topics);
        let can_open = self.transport.can_open();
        if self.transport.requested.is_empty() || (!can_open && !self.classify_new_caller) {
            return;
        }
        let topics = self.transport.requested.clone();
        let topics: Vec<_> = topics.into_iter().collect();
        #[cfg(test)]
        {
            self.open_cursor_reads += 1;
        }
        let cursors = match MlsStore::new(self.context.clone()).received_cursors(&topics) {
            Ok(cursors) => cursors,
            Err(error) => {
                self.transport.error = Some(Arc::new(error.into()));
                return;
            }
        };
        let request = if self.transport.factory.is_some() {
            RequestKey::Bidi(cursors.clone())
        } else {
            RequestKey::QueryNewest(topics.iter().cloned().collect())
        };
        self.classify_new_caller = false;
        // implements: API-284
        let rejected = self
            .rejected_requests
            .lock()
            .iter()
            .find(|(key, _)| key == &request)
            .map(|(_, cause)| cause.clone());
        if let Some(cause) = rejected {
            self.apply_rejected_open(cause);
            return;
        }
        if !can_open {
            return;
        }
        let future: OpenFuture = if let Some(factory) = &self.transport.factory {
            let limits = self.context.incoming_runtime().policy().fetched_limits();
            let future = factory.open(cursors, limits);
            Box::pin(async move { future.await.map(Opened::Stream) })
        } else {
            let context = self.context.clone();
            Box::pin(async move {
                context
                    .api()
                    .newest_topic_cursors(topics)
                    .await
                    .map(Opened::Unary)
                    .map_err(NetworkError::new)
            })
        };
        self.transport.start(future, request);
    }

    pub(super) fn opened(&mut self, result: Result<Opened, NetworkError>) {
        match result {
            Ok(Opened::Stream(subscription)) => {
                self.transport.state = TransportState::Streaming(subscription);
                self.transport.error = None;
                self.transport.opened();
            }
            Ok(Opened::Unary(targets)) => {
                self.registered(targets);
                self.transport.state = TransportState::Unary;
                self.transport.error = None;
                self.transport.opened();
            }
            Err(error) => {
                self.open_error(error);
            }
        }
    }

    pub(super) fn registered(&mut self, targets: TopicCursor) {
        self.transport.registered.extend(targets.keys().cloned());
        self.transport.registered();
        for scope in self.scopes.values_mut() {
            for (topic, target) in &targets {
                if scope.topics.contains(topic) {
                    scope.targets.entry(topic.clone()).or_insert(*target);
                }
            }
        }
    }

    // A new scope can share an active registration. Capture its own fixed target
    // without replacing the connection or reusing an older scope's target.
    pub(super) fn start_targets(&mut self) {
        if self.targets.is_some() {
            return;
        }
        let now = Instant::now();
        self.target_floors.retain(|_, until| now < *until);
        let mut scopes = Vec::new();
        let mut topics = HashSet::new();
        for (id, scope) in &self.scopes {
            let missing: Vec<_> = scope
                .topics
                .iter()
                .filter(|topic| {
                    self.transport.registered.contains(*topic)
                        && !scope.targets.contains_key(*topic)
                        && !self.target_floors.contains_key(*topic)
                })
                .cloned()
                .collect();
            if !missing.is_empty() {
                scopes.push((*id, scope.generation));
                topics.extend(missing);
            }
        }
        if topics.is_empty() {
            return;
        }
        // implements: API-284
        let rejected = self
            .rejected_requests
            .lock()
            .iter()
            .find(|(key, _)| key == &RequestKey::QueryNewest(topics.clone()))
            .map(|(_, cause)| cause.clone());
        if let Some(cause) = rejected {
            self.record_target_error(&scopes, &topics, cause, false);
            return;
        }
        if self.live_suspended()
            || self.transport.backing_off()
            || self.targets_retry_at.is_some_and(|at| Instant::now() < at)
        {
            return;
        }
        self.targets_request = Some(topics.clone());
        let context = self.context.clone();
        self.targets = Some(Box::pin(async move {
            let result = context
                .api()
                .newest_topic_cursors(topics.into_iter().collect())
                .await
                .map_err(NetworkError::new);
            (scopes, result)
        }));
    }

    pub(super) fn targets_finished(
        &mut self,
        (scopes, result): (Vec<(u64, u64)>, Result<TopicCursor, NetworkError>),
    ) {
        let request = self.targets_request.take();
        match result {
            Ok(targets) => {
                self.targets_retry_at = None;
                for (id, generation) in scopes {
                    if let Some(scope) = self
                        .scopes
                        .get_mut(&id)
                        .filter(|scope| scope.generation == generation)
                    {
                        scope.target_error = None;
                        for (topic, target) in &targets {
                            if scope.topics.contains(topic) {
                                scope.targets.entry(topic.clone()).or_insert(*target);
                                if let Some(state) =
                                    self.state.consumer_recovery.lock().get_mut(&id)
                                {
                                    state.clear_query_error(topic);
                                }
                            }
                        }
                    }
                }
            }
            Err(error) => {
                let rejected = crate::subscriptions::recovery::rejected_request(&error);
                if !rejected {
                    let fallback = self
                        .context
                        .incoming_runtime()
                        .policy()
                        .receiver_fallback_interval;
                    self.targets_retry_at = Some(xmtp_common::time::saturating_after(
                        Instant::now(),
                        fallback,
                    ));
                    // A rate-limited target read waits its delay; other
                    // topics' captures do not.
                    // implements: RATE-041
                    if let (Some(delay), Some(request)) = (error.rate_limit_delay(), &request) {
                        let until = xmtp_common::time::saturating_after(Instant::now(), delay);
                        for topic in request {
                            let floor = self.target_floors.entry(topic.clone()).or_insert(until);
                            *floor = (*floor).max(until);
                        }
                    }
                }
                let cause = Arc::new(IncomingError::Transport(error));
                if let Some(request) = request {
                    let mut permanent = self.rejected_requests.lock();
                    if rejected
                        && !permanent
                            .iter()
                            .any(|(key, _)| key == &RequestKey::QueryNewest(request.clone()))
                    {
                        permanent.push((RequestKey::QueryNewest(request.clone()), cause.clone()));
                    }
                    drop(permanent);
                    self.record_target_error(&scopes, &request, cause, true);
                }
            }
        }
    }

    fn record_target_error(
        &mut self,
        scopes: &[(u64, u64)],
        request: &HashSet<Topic>,
        cause: Arc<IncomingError>,
        attempted: bool,
    ) {
        let now = Instant::now();
        for &(id, generation) in scopes {
            let Some(scope) = self
                .scopes
                .get_mut(&id)
                .filter(|scope| scope.generation == generation)
            else {
                continue;
            };
            let affected: HashSet<_> = request.intersection(&scope.topics).cloned().collect();
            let Some(topic) = affected.iter().next() else {
                continue;
            };
            if scope
                .target_error
                .as_ref()
                .is_some_and(|(old, error)| old == &affected && Arc::ptr_eq(error, &cause))
            {
                continue;
            }
            if let Some(state) = self.state.consumer_recovery.lock().get_mut(&id) {
                if attempted && xmtp_api::preflight::failure(cause.as_ref()).is_none() {
                    state.record_query_failure(
                        topic,
                        cause.clone(),
                        self.transport.recovery.failures,
                        now,
                    );
                } else {
                    state.reject(cause.clone());
                }
            }
            scope.target_error = Some((affected, cause.clone()));
        }
    }

    pub(super) fn open_error(&mut self, error: NetworkError) {
        let preflight = xmtp_api::preflight::failure(&error).is_some();
        let rejected = crate::subscriptions::recovery::rejected_request(&error);
        self.source_error(error);
        if preflight && let Some(cause) = self.transport.error.clone() {
            self.apply_rejected_open(cause);
        }
        if rejected
            && let (Some(request), Some(cause)) = (
                self.transport.attempted_open.clone(),
                self.transport.error.clone(),
            )
        {
            let mut permanent = self.rejected_requests.lock();
            if !permanent.iter().any(|(prior, _)| prior == &request) {
                permanent.push((request, cause));
            }
        }
    }

    fn apply_rejected_open(&mut self, cause: Arc<IncomingError>) {
        self.transport.error = Some(cause.clone());
        self.transport.recovery.error = Some(cause.clone());
        *self.state.recovery.lock() = self.transport.recovery.clone();
        let mut consumers = self.state.consumer_recovery.lock();
        for (id, scope) in &self.scopes {
            if scope
                .topics
                .iter()
                .any(|topic| self.transport.requested.contains(topic))
                && let Some(state) = consumers.get_mut(id)
            {
                state.reject(cause.clone());
            }
        }
        self.state.notify();
    }
}
