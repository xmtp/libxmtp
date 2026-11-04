//! Controller reading: admitting received batches, scheduled reads and their
//! results, and receive errors.

use super::*;

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn incoming(&mut self, event: Option<Result<IncomingEvent, NetworkError>>) {
        match event {
            Some(Ok(IncomingEvent::Registered { targets, .. })) => self.registered(targets),
            Some(Ok(IncomingEvent::OrderedBatch(batch))) => {
                let topic = batch.topic.clone();
                match self.admit_received_batch(batch) {
                    Ok(()) => {
                        self.topics.entry(topic.clone()).or_default().error = None;
                        self.clear_receipt_failure(&topic);
                    }
                    Err(error) => self.receive_error(topic, error),
                }
            }
            Some(Err(error)) => {
                // A shared wire error has no mutate ID. Hold each affected
                // lease's unchanged request until its caller changes it.
                self.open_error(error);
            }
            None | Some(Ok(IncomingEvent::Disconnected)) => {
                self.transport.ended();
                self.transport.disconnect(
                    self.context
                        .incoming_runtime()
                        .policy()
                        .receiver_fallback_interval,
                );
                self.clear_read_times();
            }
        }
    }

    /// Commit bounded prefixes. Uncommitted suffixes replay from durable receipt.
    pub(super) fn admit_received_batch(
        &mut self,
        batch: OrderedEnvelopeBatch,
    ) -> Result<(), IncomingError> {
        let key = topic_key(&batch.topic)?;
        let limits = self
            .context
            .incoming_runtime()
            .policy()
            .incoming_limits(key.kind);
        let max_rows = limits
            .batch
            .rows
            .min(limits.topic.rows)
            .min(limits.kind.rows);
        let max_bytes = limits
            .batch
            .bytes
            .min(limits.topic.bytes)
            .min(limits.kind.bytes);
        let capacity_error = || {
            xmtp_mls_core::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Envelope(
                xmtp_api_backend::envelope::EnvelopeError::Capacity,
            ))
        };
        if max_rows == 0 || max_bytes == 0 {
            return Err(capacity_error().into());
        }
        // Validate the full input before the first prefix can change receipt.
        let mut cursors = [(batch.topic.clone(), batch.after)].into();
        let batches = xmtp_api_backend::envelope::ordered_batches(
            &mut cursors,
            batch.envelopes,
            self.context.incoming_runtime().policy().fetched_limits(),
        )
        .map_err(xmtp_api::ApiError::from)
        .map_err(xmtp_mls_core::mls_store::MlsStoreError::from)?;
        let mut pending = batches
            .into_iter()
            .flat_map(|batch| batch.envelopes)
            .peekable();
        if let Some(pending) = &mut self.pending_topics {
            pending.insert(key.clone());
        }
        let mut after = batch.after;
        let store = MlsStore::new(self.context.clone());
        while pending.peek().is_some() {
            let mut bytes = 0u64;
            let mut chunk = OrderedEnvelopeBatch {
                topic: batch.topic.clone(),
                after,
                envelopes: Vec::new(),
            };
            while let Some(envelope) = pending.peek() {
                let next_bytes = bytes.saturating_add(envelope.raw().len() as u64);
                if chunk.envelopes.len() as u64 >= max_rows || next_bytes > max_bytes {
                    break;
                }
                bytes = next_bytes;
                if let Some(envelope) = pending.next() {
                    chunk.envelopes.push(envelope);
                }
            }
            if chunk.envelopes.is_empty() {
                return Err(capacity_error().into());
            }
            after = chunk
                .envelopes
                .last()
                .and_then(|envelope| envelope.meta())
                .and_then(|meta| meta.cursor.as_ref())
                .map(|cursor| Cursor(cursor.sequence_id))
                .ok_or_else(|| {
                    xmtp_mls_core::mls_store::MlsStoreError::Api(
                        xmtp_api::ApiError::InvalidResponse("incoming cursor"),
                    )
                })?;
            let admitted = store.admit_incoming_batch(&chunk, limits)?;
            if let Some(subscription) = self.transport.subscription() {
                subscription
                    .acknowledge_received([(batch.topic.clone(), admitted.received)].into());
            }
        }
        Ok(())
    }

    pub(super) fn source_error(&mut self, error: NetworkError) {
        let policy = self.context.incoming_runtime().policy();
        self.transport.fail(
            error,
            policy.receiver_fallback_interval,
            RetryBackoff {
                initial: policy.permanent_retry_initial,
                max: policy.permanent_retry_max,
            },
        );
        self.clear_read_times();
    }

    /// Unary reads are independent of the stream receiver. A failing or
    /// backing-off stream must not stop bounded Query recovery.
    // implements: API-284
    pub(super) fn start_read(&mut self) {
        if self.read.is_some() {
            return;
        }
        let now = Instant::now();
        for _ in 0..self.read_queue.len() {
            let Some(topic) = self.read_queue.pop_front() else {
                break;
            };
            self.read_queue.push_back(topic.clone());
            if self.receipt(&topic).paused
                || self.is_retired(&topic)
                || (self.storage_error.is_some() && !self.transport.requested.contains(&topic))
                || (self.receipt(&topic).blocked() && self.receipt(&topic).rejected_at.is_none())
                || self
                    .receipt(&topic)
                    .rate_limited_until
                    .is_some_and(|until| now < until)
            {
                continue;
            }
            let key = match topic_key(&topic) {
                Ok(key) => key,
                Err(error) => {
                    self.topic_error(topic, error);
                    continue;
                }
            };
            match self.read_due(&topic, &key, now) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    self.topic_error(topic, error);
                    continue;
                }
            }
            self.topics
                .entry(topic.clone())
                .or_default()
                .receipt
                .last_read = Some(now);
            let context = self.context.clone();
            let rejected_requests = self.rejected_requests.clone();
            let limits = context
                .incoming_runtime()
                .policy()
                .incoming_limits(key.kind);
            self.read = Some(Box::pin(async move {
                let store = MlsStore::new(context);
                let cursors = store.received_cursors(std::slice::from_ref(&topic));
                match cursors {
                    Ok(cursors) => {
                        let cursor = cursors.get(&topic).copied();
                        if let Some(cursor) = cursor {
                            let cause = rejected_requests
                                .lock()
                                .iter()
                                .find(|(request, _)| {
                                    request == &RequestKey::Query(topic.clone(), cursor)
                                })
                                .map(|(_, cause)| cause.clone());
                            if let Some(cause) = cause {
                                return (topic, Some(cursor), ReadOutcome::Rejected(cause));
                            }
                        }
                        let result = store.receive_topics_once_from(cursors, limits).await;
                        (topic, cursor, ReadOutcome::Fetched(result))
                    }
                    Err(error) => (topic, None, ReadOutcome::Fetched(Err(error))),
                }
            }));
            break;
        }
    }

    /// Explicit sync queries immediately. Stream-first targets keep one fixed receiver wait.
    pub(super) fn read_due(
        &mut self,
        topic: &Topic,
        key: &StreamTopic,
        now: Instant,
    ) -> Result<bool, IncomingError> {
        let settings = self.context.incoming_runtime().policy();
        let last = self.receipt(topic).last_read;
        let covered =
            self.transport.subscription().is_some() && self.transport.registered.contains(topic);
        let mut barrier = false;
        let mut unfinished = false;
        let mut ready = false;
        let mut first_read = false;
        let mut received = None;
        let suspended = self.live_suspended();
        for scope in self.scopes.values() {
            let (target, deadline, receive_policy) = match &scope.scope {
                ScopeKind::Barrier {
                    deadline,
                    receive_policy,
                } => (
                    self.barrier_receipt_target(topic, &scope.targets),
                    Some(*deadline),
                    *receive_policy,
                ),
                _ if !suspended => (
                    scope.targets.get(topic).copied(),
                    None,
                    IncomingReceivePolicy::StreamFirst,
                ),
                _ => continue,
            };
            let Some(target) = target else {
                continue;
            };
            barrier |= deadline.is_some();
            if self
                .receipt(topic)
                .received_floor
                .is_some_and(|floor| floor >= target)
            {
                continue;
            }
            let received = match received {
                Some(received) => received,
                None => {
                    #[cfg(test)]
                    self.receipt_progress_reads
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let progress = self.context.db().topic_progress(key)?.received;
                    self.topics
                        .entry(topic.clone())
                        .or_default()
                        .receipt
                        .received_floor = Some(progress);
                    received = Some(progress);
                    progress
                }
            };
            if received >= target {
                continue;
            }
            unfinished = true;
            let wait_until = scope.receipt_wait_started + settings.receiver_fallback_interval;
            if receive_policy == IncomingReceivePolicy::ImmediateQuery
                || !covered
                || deadline.is_some_and(|deadline| wait_until >= deadline)
                || now >= wait_until
            {
                ready = true;
                first_read |= last.is_none_or(|last| last < scope.receipt_wait_started);
            }
        }
        if unfinished {
            return Ok(ready
                && (first_read
                    || last.is_none_or(|last| {
                        now.duration_since(last) >= settings.active_database_poll_interval
                    })));
        }
        // Completed barriers need processing only. Other scopes can still receive new traffic.
        if barrier
            && !self.extra_topics.contains(topic)
            && !self.scopes.values().any(|scope| {
                !matches!(scope.scope, ScopeKind::Barrier { .. }) && scope.topics.contains(topic)
            })
        {
            return Ok(false);
        }
        if covered || suspended {
            return Ok(false);
        }
        Ok(last.is_none_or(|last| now.duration_since(last) >= settings.receiver_fallback_interval))
    }

    /// A Welcome barrier also owns the group prefix required by its pending parents.
    fn barrier_receipt_target(&self, topic: &Topic, targets: &TopicCursor) -> Option<Cursor> {
        let welcome = Topic::new_welcome_message(self.context.installation_id());
        let prefix = targets.get(&welcome).and_then(|target| {
            self.dependency_registry
                .prefixes()
                .filter(|(parent, group, _)| {
                    *parent <= *target
                        && topic.kind() == TopicKind::GroupMessagesV1
                        && topic.identifier() == group.as_slice()
                })
                .map(|(_, _, anchor)| anchor)
                .max()
        });
        targets.get(topic).copied().into_iter().chain(prefix).max()
    }

    /// Bounded sync remains explicit network work; suspended live scopes do not poll.
    pub(super) fn live_suspended(&self) -> bool {
        self.transport
            .factory
            .as_ref()
            .is_some_and(|factory| factory.is_suspended())
    }

    pub(super) fn read_finished(&mut self, (topic, cursor, outcome): ReadResult) {
        let result = match outcome {
            ReadOutcome::Fetched(result) => result,
            ReadOutcome::Rejected(cause) => {
                self.apply_rejected_query(&topic, cursor.expect("rejected cursor"), cause);
                return;
            }
        };
        if self
            .receipt(&topic)
            .rejected_at
            .is_some_and(|old| Some(old) != cursor)
        {
            self.topics
                .entry(topic.clone())
                .or_default()
                .receipt
                .rejected_at = None;
            self.clear_receipt_failure(&topic);
        }
        match result {
            Ok(page) => {
                self.clear_receipt_failure(&topic);
                self.topics
                    .entry(topic.clone())
                    .or_default()
                    .receipt
                    .rejected_at = None;
                let mut consumers = self.state.consumer_recovery.lock();
                for (id, scope) in &self.scopes {
                    if scope.topics.contains(&topic)
                        && let Some(state) = consumers.get_mut(id)
                    {
                        state.clear_query_error(&topic);
                    }
                }
                drop(consumers);
                if page.has_more {
                    self.topics
                        .entry(topic.clone())
                        .or_default()
                        .receipt
                        .last_read = None;
                }
                if let Some(subscription) = self.transport.subscription() {
                    subscription.acknowledge_received(
                        page.admissions
                            .into_iter()
                            .map(|(topic, admitted)| (topic, admitted.received))
                            .collect(),
                    );
                }
                self.topics.entry(topic.clone()).or_default().error = None;
            }
            Err(error) => {
                let rejected = matches!(&error, xmtp_mls_core::mls_store::MlsStoreError::Api(_))
                    && crate::subscriptions::recovery::rejected_request(&error);
                let cause = Arc::new(IncomingError::from(error));
                if rejected && let Some(cursor) = cursor {
                    let request = RequestKey::Query(topic.clone(), cursor);
                    let mut permanent = self.rejected_requests.lock();
                    if !permanent.iter().any(|(key, _)| key == &request) {
                        permanent.push((request, cause.clone()));
                    }
                }
                if rejected {
                    self.topics
                        .entry(topic.clone())
                        .or_default()
                        .receipt
                        .rejected_at = cursor;
                }
                self.receive_error_shared(topic, cause);
            }
        }
    }

    fn apply_rejected_query(&mut self, topic: &Topic, cursor: Cursor, cause: Arc<IncomingError>) {
        let state = self.topics.entry(topic.clone()).or_default();
        state.receipt.rejected_at = Some(cursor);
        state.error = Some(cause.clone());
        let mut changed = false;
        let mut consumers = self.state.consumer_recovery.lock();
        for (id, scope) in &self.scopes {
            if scope.topics.contains(topic)
                && let Some(consumer) = consumers.get_mut(id)
                && consumer.snapshot.terminal.is_none()
            {
                consumer.reject(cause.clone());
                changed = true;
            }
        }
        drop(consumers);
        if changed {
            self.state.notify();
        }
    }

    /// Reopen from durable receipt after any failed admission. Keep pending work and scope targets.
    pub(super) fn receive_error(&mut self, topic: Topic, error: IncomingError) {
        self.receive_error_shared(topic, Arc::new(error));
    }

    fn receive_error_shared(&mut self, topic: Topic, error: Arc<IncomingError>) {
        let query_failure = matches!(
            error.as_ref(),
            IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Api(_))
        );
        // A refused read leaves the stream untouched: only a request was denied,
        // and the stream still delivers. The topic's read waits out the delay.
        // implements: RATE-041
        let rate_limit = error.rate_limit_delay();
        let rate_limited = rate_limit.is_some();
        if let Some(delay) = rate_limit {
            self.topics
                .entry(topic.clone())
                .or_default()
                .receipt
                .rate_limited_until =
                Some(xmtp_common::time::saturating_after(Instant::now(), delay));
        } else {
            // The transport can advance its read cursor before storage commits. Drop
            // that registration even when reconciliation immediately clears a pause.
            self.transport.disconnect(
                self.context
                    .incoming_runtime()
                    .policy()
                    .receiver_fallback_interval,
            );
        }
        let now = Instant::now();
        self.topics
            .entry(topic.clone())
            .or_default()
            .receipt
            .last_read = Some(now);
        if capacity(error.as_ref()) {
            // Capacity is a storage condition, not a bad response. It
            // supersedes any permanent-error backoff, which would otherwise
            // keep gating the topic after reconciliation unpauses it.
            let receipt = &mut self.topics.entry(topic.clone()).or_default().receipt;
            receipt.paused = true;
            receipt.blocked_until = None;
            receipt.blocked_failures = 0;
        } else if !error.as_ref().is_retryable() {
            let policy = self.context.incoming_runtime().policy();
            let backoff = RetryBackoff {
                initial: policy.permanent_retry_initial,
                max: policy.permanent_retry_max,
            };
            let receipt = &mut self.topics.entry(topic.clone()).or_default().receipt;
            receipt.blocked_failures = receipt.blocked_failures.saturating_add(1);
            receipt.blocked_until = Some(now + backoff.delay(receipt.blocked_failures));
        } else if !rate_limited {
            // A rate limit says nothing about the topic, so its permanent-error
            // backoff stays.
            self.clear_receipt_failure(&topic);
        }
        self.topics.entry(topic.clone()).or_default().error = Some(error);
        if query_failure {
            let cause = self.topics[&topic]
                .error
                .as_ref()
                .expect("query error stored")
                .clone();
            let now = Instant::now();
            let mut consumers = self.state.consumer_recovery.lock();
            for (id, scope) in &self.scopes {
                if scope.topics.contains(&topic)
                    && let Some(state) = consumers.get_mut(id)
                {
                    if xmtp_api::preflight::failure(cause.as_ref()).is_some() {
                        state.reject(cause.clone());
                    } else {
                        state.record_query_failure(
                            &topic,
                            cause.clone(),
                            self.transport.recovery.failures,
                            now,
                        );
                    }
                }
            }
            drop(consumers);
            self.state.notify();
        }
    }
}
