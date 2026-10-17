use super::*;

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn refresh_statuses(&self) {
        self.refresh_statuses_at(Instant::now());
    }

    pub(super) fn refresh_statuses_at(&self, now: Instant) {
        *self.state.recovery.lock() = self.transport.recovery.clone();
        let conn = self.context.db();
        let mut snapshots = self.state.statuses.lock();
        let mut connections = Vec::new();
        for (id, scope) in &self.scopes {
            let Some(snapshot) = snapshots.get_mut(id) else {
                continue;
            };
            if snapshot.scope_generation != scope.generation {
                continue;
            }
            {
                let mut consumers = self.state.consumer_recovery.lock();
                let state = consumers.entry(*id).or_default();
                if self.transport.recovery.failures > state.last_transport_failures {
                    state.query_error = None;
                }
                state.last_transport_failures = self.transport.recovery.failures;
                let failures = self
                    .transport
                    .recovery
                    .failures
                    .saturating_add(state.query_failures);
                let recovery = &mut state.snapshot;
                recovery.idle = scope.topics.is_empty();
                let registered = !scope.topics.is_empty()
                    && scope.target_error.is_none()
                    && self.transport.connection() == IncomingConnection::Connected
                    && scope.topics.iter().all(|topic| {
                        (self.transport.registered.contains(topic)
                            && scope.targets.contains_key(topic))
                            || self.is_retired(topic)
                    });
                if !registered || recovery.failures != failures {
                    recovery.healthy_since = None;
                    recovery.outage_since.get_or_insert(now);
                }
                if registered {
                    recovery.healthy_since.get_or_insert(now);
                }
                if recovery.outage_since.is_some()
                    && recovery.healthy_since.is_some_and(|since| {
                        now.saturating_duration_since(since)
                            >= crate::subscriptions::recovery::HEALTHY_PERIOD
                    })
                {
                    recovery.outage_since = None;
                }
                if recovery.idle {
                    recovery.outage_since = None;
                }
                recovery.failures = failures;
                recovery.error = state
                    .query_error
                    .as_ref()
                    .map(|(_, error)| error.clone())
                    .or_else(|| self.transport.recovery.error.clone());
                let _ = state.check(now);
            }
            snapshot.error = self
                .storage_error
                .clone()
                .or_else(|| scope.target_error.as_ref().map(|(_, error)| error.clone()))
                .or_else(|| self.transport.error.clone());
            let mut topics = Vec::new();
            for topic in &scope.topics {
                let target = scope.targets.get(topic).copied();
                let key = match topic_key(topic) {
                    Ok(key) => key,
                    Err(error) => {
                        topics.push(IncomingTopicStatus {
                            topic: topic.clone(),
                            scope_generation: scope.generation,
                            registration: IncomingRegistration::Pending,
                            target: None,
                            received: Cursor(0),
                            processed: Cursor(0),
                            unresolved_welcomes: 0,
                            processing: IncomingProcessing::Blocked,
                            blocked: Some(error.code().into()),
                            error: Some(Arc::new(error)),
                        });
                        continue;
                    }
                };
                let progress = match conn.topic_progress(&key) {
                    Ok(progress) => progress,
                    Err(error) => {
                        snapshot.error = Some(Arc::new(error.into()));
                        continue;
                    }
                };
                let pending =
                    match conn.pending_states_through(&key, target.unwrap_or(progress.received)) {
                        Ok(pending) => pending,
                        Err(error) => {
                            snapshot.error = Some(Arc::new(error.into()));
                            continue;
                        }
                    };
                let blocked = pending.iter().find(|row| row.blocked).map(|row| {
                    row.error_code
                        .clone()
                        .unwrap_or_else(|| "processing_blocked".into())
                });
                let removed = self.is_retired(topic);
                let registered = self.transport.registered.contains(topic);
                let complete = crate::subscriptions::barrier::durable_complete(
                    key.kind,
                    target,
                    progress,
                    pending.len(),
                );
                let error = self
                    .topics
                    .get(topic)
                    .and_then(|state| state.error.clone())
                    .or_else(|| {
                        scope.target_error.as_ref().and_then(|(affected, error)| {
                            affected.contains(topic).then(|| error.clone())
                        })
                    })
                    .or_else(|| self.storage_error.clone())
                    .or_else(|| self.transport.error.clone());
                let runnable_welcome = key.kind == NetworkEntityKind::Welcome
                    && pending.iter().any(|row| !row.blocked);
                // Receipt is only pending while a source can still deliver it. A
                // source that keeps failing permanently reports Blocked rather
                // than hiding the stall, even though it does keep retrying.
                let welcome_receipt_pending = key.kind == NetworkEntityKind::Welcome
                    && target.is_none_or(|target| progress.received < target)
                    && !self.receipt(topic).failing()
                    && self.transport.permanent_failures == 0;
                let processing = if removed {
                    IncomingProcessing::Cancelled
                } else if complete && registered {
                    IncomingProcessing::Complete
                } else if !runnable_welcome
                    && !welcome_receipt_pending
                    && (blocked.is_some()
                        || error.as_ref().is_some_and(|error| !error.is_retryable()))
                {
                    IncomingProcessing::Blocked
                } else {
                    IncomingProcessing::Pending
                };
                topics.push(IncomingTopicStatus {
                    topic: topic.clone(),
                    scope_generation: scope.generation,
                    registration: if removed {
                        IncomingRegistration::Removed
                    } else if registered {
                        IncomingRegistration::Active
                    } else {
                        IncomingRegistration::Pending
                    },
                    target,
                    received: progress.received,
                    processed: progress.processed,
                    unresolved_welcomes: if key.kind == NetworkEntityKind::Welcome {
                        pending.len() as u64
                    } else {
                        0
                    },
                    processing,
                    blocked,
                    error,
                });
            }
            topics.sort_by_key(|status| status.topic.cloned_vec());
            snapshot.scope_generation = scope.generation;
            snapshot.connection_generation = self.transport.generation;
            snapshot.connection = self.transport.connection();
            connections.push((*id, snapshot.connection));
            snapshot.discovery_pending = matches!(
                scope.scope,
                ScopeKind::AllGroups | ScopeKind::DeviceSyncGroups
            ) && topics.iter().any(|topic| {
                topic.registration == IncomingRegistration::Pending
                    || (topic.topic.kind() == TopicKind::WelcomeMessagesV1
                        && topic.processing != IncomingProcessing::Complete)
            });
            snapshot.processing = if topics
                .iter()
                .any(|topic| topic.processing == IncomingProcessing::Blocked)
                || (topics.len() != scope.topics.len()
                    && snapshot
                        .error
                        .as_ref()
                        .is_some_and(|error| !error.is_retryable()))
            {
                IncomingProcessing::Blocked
            } else if !snapshot.discovery_pending
                && topics.len() == scope.topics.len()
                && topics.iter().all(|topic| {
                    matches!(
                        topic.processing,
                        IncomingProcessing::Complete | IncomingProcessing::Cancelled
                    )
                })
            {
                IncomingProcessing::Complete
            } else {
                IncomingProcessing::Pending
            };
            snapshot.topics = topics;
        }
        drop(snapshots);
        for (id, connection) in connections {
            self.state.connection_states.update(id, connection);
        }
        self.state.notify();
    }
}
