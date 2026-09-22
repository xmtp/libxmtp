use super::*;

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn refresh_statuses(&self) {
        *self.state.recovery.lock() = self.transport.recovery.clone();
        let conn = self.context.db();
        let mut snapshots = self.state.statuses.lock();
        for (id, scope) in &self.scopes {
            let Some(snapshot) = snapshots.get_mut(id) else {
                continue;
            };
            if snapshot.scope_generation != scope.generation {
                continue;
            }
            {
                let mut consumers = self.state.consumer_recovery.lock();
                let recovery = consumers.entry(*id).or_default();
                recovery.idle = scope.topics.is_empty();
                let registered = !scope.topics.is_empty()
                    && self.transport.connection() == IncomingConnection::Connected
                    && scope.topics.iter().all(|topic| {
                        self.transport.registered.contains(topic)
                            || self.is_retired(topic)
                            || self.receipt(topic).paused
                            || self.receipt(topic).blocked()
                    });
                if !registered || recovery.failures != self.transport.recovery.failures {
                    recovery.healthy_since = None;
                    recovery.outage_since.get_or_insert_with(Instant::now);
                }
                if registered {
                    recovery.healthy_since.get_or_insert_with(Instant::now);
                }
                if recovery.outage_since.is_some()
                    && recovery.healthy_since.is_some_and(|since| {
                        since.elapsed() >= crate::subscriptions::recovery::HEALTHY_PERIOD
                    })
                {
                    recovery.healthy_generation = recovery.healthy_generation.saturating_add(1);
                    recovery.healthy_failure_baseline = self.transport.recovery.failures;
                    recovery.outage_since = None;
                }
                if recovery.idle {
                    recovery.outage_since = None;
                }
                recovery.failures = self.transport.recovery.failures;
                recovery.error = self.transport.recovery.error.clone();
                if recovery.terminal.is_none()
                    && let Some(budget) = self.state.consumer_budgets.lock().get_mut(id)
                    && let Err(failure) = budget.check(recovery, Instant::now())
                {
                    recovery.terminal = Some(failure);
                }
            }
            snapshot.error = self
                .storage_error
                .clone()
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
        self.state.notify();
    }
}
