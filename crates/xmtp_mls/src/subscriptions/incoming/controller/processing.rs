use super::*;
use crate::{
    groups::{
        MlsGroup,
        mls_sync::GroupHeadOutcome,
        welcome_sync::{WelcomeHeadOutcome, WelcomeRequirement, WelcomeService},
    },
    identity_updates::{IdentityDependencyError, resolve_identity_requirement},
};
use prost::Message;
use xmtp_db::{identity_update::StoredIdentityUpdate, incoming_envelope::IncomingRetry};

pub(super) enum DependencyResult<C> {
    Identity(IdentityRequirement, Result<(), IdentityDependencyError>),
    Welcome(
        Cursor,
        Result<WelcomeHeadOutcome<C>, crate::groups::GroupError>,
    ),
}

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn process_ready(&mut self) -> bool {
        let mut progress = false;
        let topics: Vec<_> = self.read_queue.iter().cloned().collect();
        for topic in topics {
            if self.is_retired(&topic) {
                continue;
            }
            match topic.kind() {
                TopicKind::GroupMessagesV1 => {
                    let group_id = match topic.identifier().try_into() {
                        Ok(group) => group,
                        Err(_) => continue,
                    };
                    let group = match MlsStore::new(self.context.clone()).group(&group_id) {
                        Ok(group) => group,
                        // A welcome can install this group on a later pass.
                        Err(crate::mls_store::MlsStoreError::NotFound(_)) => continue,
                        Err(error) => {
                            self.topic_error(topic, error.into());
                            continue;
                        }
                    };
                    let head = match self
                        .context
                        .db()
                        .first_pending_envelope(&StreamTopic::group(group_id))
                    {
                        Ok(Some(head)) => head,
                        Ok(None) => continue,
                        Err(error) => {
                            self.topic_error(topic, error.into());
                            continue;
                        }
                    };
                    let cursor = Cursor(head.sequence_id as u64);
                    if self
                        .dependency_registry
                        .contains(&DependencyParent::GroupHead(topic.clone(), cursor))
                    {
                        continue;
                    }
                    let retry_blocked = self.retry_blocked_head(&topic, cursor, head.blocked);
                    let missing = self
                        .topics
                        .get(&topic)
                        .and_then(|state| state.processing.missing_reference.as_ref())
                        .filter(|(head, _)| *head == cursor)
                        .map(|(_, requirement)| requirement);
                    match group.process_pending_group_head_with_retry(missing, retry_blocked) {
                        Ok(GroupHeadOutcome::Progress { cursor, result }) => {
                            tracing::trace!(%group_id, sequence_id = cursor.0, accepted = result.is_ok(), "group head completed");
                            self.topics
                                .entry(topic.clone())
                                .or_default()
                                .processing
                                .missing_reference = None;
                            self.topics.entry(topic.clone()).or_default().error = None;
                            progress = true;
                            if result.as_ref().is_ok_and(|outcome| !outcome.group_active) {
                                self.topics
                                    .entry(topic.clone())
                                    .or_default()
                                    .processing
                                    .retired = true;
                            }
                            if let Ok(outcome) = result
                                && let Some(change) = outcome.app_data_change
                            {
                                self.dispatch_change(group, change);
                            }
                        }
                        Ok(GroupHeadOutcome::Need {
                            cursor,
                            requirement,
                        }) => {
                            tracing::trace!(%group_id, sequence_id = cursor.0, "group head needs an identity proof");
                            self.dependency_registry.attach(
                                DependencyParent::GroupHead(topic, cursor),
                                DependencyKey::Identity(requirement),
                            );
                        }
                        Ok(GroupHeadOutcome::Inactive) => {
                            self.topics.entry(topic).or_default().processing.retired = true;
                        }
                        Ok(GroupHeadOutcome::Waiting {
                            cursor,
                            code,
                            blocked,
                        }) => {
                            tracing::trace!(%group_id, sequence_id = cursor.0, code, blocked, "group head waits for processing");
                        }
                        Ok(GroupHeadOutcome::Idle) => {}
                        Err(error) => self.topic_error(topic, error.into()),
                    }
                }
                TopicKind::WelcomeMessagesV1 => {
                    if topic.identifier() != self.context.installation_id().as_slice() {
                        continue;
                    }
                    let service = WelcomeService::new(self.context.clone());
                    // A blocked row is only revisited by this scan. Rearm it on
                    // an interval so a client that stays up reaches the
                    // retention deadline of work it cannot process.
                    if self.welcome_blocked_scan.is_none()
                        && self
                            .welcome_blocked_rescan_at
                            .is_some_and(|at| Instant::now() >= at)
                    {
                        self.welcome_blocked_scan = Some(Cursor(0));
                        self.welcome_blocked_rescan_at = None;
                    }
                    let outcomes = match self.welcome_blocked_scan {
                        Some(after) => service.retry_blocked_welcomes_after(after),
                        None => service.process_pending_welcomes_once(),
                    };
                    match outcomes {
                        Ok(outcomes) => {
                            if self.welcome_blocked_scan.is_some() {
                                self.welcome_blocked_scan = outcomes
                                    .iter()
                                    .map(|outcome| match outcome {
                                        WelcomeHeadOutcome::Idle { cursor }
                                        | WelcomeHeadOutcome::Waiting { cursor, .. }
                                        | WelcomeHeadOutcome::Need { cursor, .. }
                                        | WelcomeHeadOutcome::Progress { cursor, .. } => *cursor,
                                    })
                                    .max();
                                if self.welcome_blocked_scan.is_none() {
                                    // The scan is exhausted. Schedule the next one.
                                    self.welcome_blocked_rescan_at = Some(
                                        Instant::now()
                                            + self
                                                .context
                                                .incoming_runtime()
                                                .policy()
                                                .blocked_welcome_rescan_interval,
                                    );
                                }
                            }
                            for outcome in outcomes {
                                progress |= self.welcome_outcome(outcome);
                            }
                        }
                        Err(error) => self.topic_error(topic, error.into()),
                    }
                }
                TopicKind::IdentityUpdatesV1 => {
                    if let Err(error) = self.prepare_identity_head(&topic) {
                        self.defer_head(&topic, "identity_invalid", !error.is_retryable());
                        self.topic_error(topic, error);
                    }
                }
                _ => self.topic_error(topic, IncomingError::UnsupportedTopic),
            }
        }
        let due: Vec<_> = self
            .dependency_registry
            .prefixes()
            .filter_map(|(cursor, group, anchor)| {
                self.context
                    .db()
                    .topic_progress(&StreamTopic::group(group))
                    .ok()
                    .filter(|progress| progress.processed >= anchor)
                    .map(|_| cursor)
            })
            .collect();
        for cursor in due {
            self.dependency_registry
                .detach(&DependencyParent::Welcome(cursor));
            if !self.parent_is_pending(&DependencyParent::Welcome(cursor)) {
                continue;
            }
            match WelcomeService::new(self.context.clone()).retry_pending_welcome(cursor, None) {
                Ok(outcome) => progress |= self.welcome_outcome(outcome),
                Err(error) => self.topic_error(self.welcome_topic(), error.into()),
            }
        }
        self.start_dependencies();
        progress
    }

    fn welcome_topic(&self) -> Topic {
        Topic::new_welcome_message(self.context.installation_id())
    }

    fn welcome_outcome(&mut self, outcome: WelcomeHeadOutcome<C>) -> bool {
        match outcome {
            WelcomeHeadOutcome::Progress { cursor, result } => {
                tracing::trace!(
                    sequence_id = cursor.0,
                    accepted = result.is_ok(),
                    "Welcome completed"
                );
                self.dependency_registry
                    .detach(&DependencyParent::Welcome(cursor));
                self.topics.entry(self.welcome_topic()).or_default().error = None;
                true
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::Identity(requirement),
            } => {
                self.dependency_registry.attach(
                    DependencyParent::Welcome(cursor),
                    DependencyKey::Identity(requirement),
                );
                false
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::GroupPrefix { group_id, anchor },
            } => {
                self.dependency_registry.attach(
                    DependencyParent::Welcome(cursor),
                    DependencyKey::GroupPrefix(group_id, anchor),
                );
                self.extra_topics.insert(Topic::new_group_message(group_id));
                false
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::Pointee,
            } => {
                self.dependency_registry.attach(
                    DependencyParent::Welcome(cursor),
                    DependencyKey::Welcome(cursor),
                );
                false
            }
            WelcomeHeadOutcome::Waiting {
                cursor,
                code,
                blocked,
            } => {
                self.dependency_registry
                    .detach(&DependencyParent::Welcome(cursor));
                tracing::trace!(
                    sequence_id = cursor.0,
                    code,
                    blocked,
                    "Welcome waits for processing"
                );
                false
            }
            WelcomeHeadOutcome::Idle { cursor } => {
                self.dependency_registry
                    .detach(&DependencyParent::Welcome(cursor));
                false
            }
        }
    }

    pub(super) fn start_dependencies(&mut self) {
        let available = self
            .context
            .incoming_runtime()
            .policy()
            .max_dependency_requests
            .saturating_sub(self.dependencies.len());
        for key in self.dependency_registry.start_queued(available) {
            let context = self.context.clone();
            self.dependencies.push(Box::pin(async move {
                match key {
                    DependencyKey::Identity(requirement) => {
                        let result = resolve_identity_requirement(&context, &requirement).await;
                        DependencyResult::Identity(requirement, result)
                    }
                    DependencyKey::Welcome(cursor) => DependencyResult::Welcome(
                        cursor,
                        WelcomeService::new(context)
                            .resolve_pending_welcome(cursor)
                            .await,
                    ),
                    DependencyKey::GroupPrefix(..) => {
                        unreachable!("prefix watches do not start requests")
                    }
                }
            }));
        }
    }

    fn prepare_identity_head(&mut self, topic: &Topic) -> Result<(), IncomingError> {
        let key = topic_key(topic)?;
        let Some(pending) = self.context.db().first_pending_envelope(&key)? else {
            return Ok(());
        };
        let retry_blocked =
            self.retry_blocked_head(topic, Cursor(pending.sequence_id as u64), pending.blocked);
        if !retry_blocked && (pending.blocked || pending.retry_at_ns > xmtp_common::time::now_ns())
        {
            return Ok(());
        }
        let wire = xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice())
            .map_err(|_| IncomingError::Storage(xmtp_db::StorageError::DbDeserialize))?;
        let entry = xmtp_api_backend::envelope::decode_identity_update(wire).map_err(|error| {
            IncomingError::Store(crate::mls_store::MlsStoreError::Api(error.into()))
        })?;
        self.context
            .db()
            .insert_or_ignore_identity_updates(&[StoredIdentityUpdate::new(
                entry.update.inbox_id.clone(),
                pending.sequence_id,
                entry.meta.server_ns as i64,
                entry.update.encode_to_vec(),
            )])
            .map_err(|error| IncomingError::Storage(error.into()))?;
        let requirement = IdentityRequirement {
            inbox_id: entry.update.inbox_id,
            sequence_id: pending.sequence_id as u64,
        };
        self.dependency_registry.attach(
            DependencyParent::IdentityHead(topic.clone(), Cursor(pending.sequence_id as u64)),
            DependencyKey::Identity(requirement),
        );
        Ok(())
    }

    pub(super) fn dependency_finished(&mut self, result: DependencyResult<C>) {
        match result {
            DependencyResult::Welcome(cursor, result) => {
                let parents = self
                    .dependency_registry
                    .finish(&DependencyKey::Welcome(cursor));
                if !parents.contains(&DependencyParent::Welcome(cursor)) {
                    return;
                }
                match result {
                    Ok(outcome) => {
                        // Resolution can complete its own parent in the transaction.
                        if matches!(outcome, WelcomeHeadOutcome::Progress { .. })
                            || self.parent_is_pending(&DependencyParent::Welcome(cursor))
                        {
                            self.welcome_outcome(outcome);
                        }
                    }
                    Err(error) if self.parent_is_pending(&DependencyParent::Welcome(cursor)) => {
                        self.topic_error(self.welcome_topic(), error.into());
                    }
                    Err(_) => {}
                }
            }
            DependencyResult::Identity(requirement, result) => {
                let parents = self
                    .dependency_registry
                    .finish(&DependencyKey::Identity(requirement.clone()));
                let missing = matches!(result, Err(IdentityDependencyError::MissingReference(_)));
                let error = result
                    .err()
                    .map(|error| Arc::new(IncomingError::Identity(error)));
                for parent in parents {
                    if !self.parent_is_pending(&parent) {
                        continue;
                    }
                    match parent {
                        DependencyParent::GroupHead(topic, cursor) if missing => {
                            self.topics
                                .entry(topic)
                                .or_default()
                                .processing
                                .missing_reference = Some((cursor, requirement.clone()));
                        }
                        DependencyParent::Welcome(cursor) if error.is_none() || missing => {
                            match WelcomeService::new(self.context.clone())
                                .retry_pending_welcome(cursor, missing.then_some(&requirement))
                            {
                                Ok(outcome) => {
                                    self.welcome_outcome(outcome);
                                }
                                Err(error) => self.topic_error(self.welcome_topic(), error.into()),
                            }
                        }
                        DependencyParent::IdentityHead(topic, cursor) if error.is_none() => {
                            if let Ok(key) = topic_key(&topic)
                                && let Err(error) =
                                    self.context.db().complete_pending_envelope(&key, cursor)
                            {
                                self.topic_error(topic, error.into());
                            }
                        }
                        parent => {
                            if let Some(error) = &error {
                                let (topic, cursor, code) = match parent {
                                    DependencyParent::GroupHead(topic, cursor) => {
                                        (topic, cursor, "identity_dependency")
                                    }
                                    DependencyParent::IdentityHead(topic, cursor) => {
                                        (topic, cursor, "identity_invalid")
                                    }
                                    DependencyParent::Welcome(cursor) => {
                                        (self.welcome_topic(), cursor, "identity_dependency")
                                    }
                                };
                                self.defer_cursor(&topic, cursor, code, !error.is_retryable());
                                self.topics.entry(topic).or_default().error = Some(error.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn parent_is_pending(&self, parent: &DependencyParent) -> bool {
        match parent {
            DependencyParent::GroupHead(topic, cursor)
            | DependencyParent::IdentityHead(topic, cursor) => topic_key(topic)
                .ok()
                .and_then(|key| {
                    self.context
                        .db()
                        .first_pending_envelope(&key)
                        .ok()
                        .flatten()
                })
                .is_some_and(|head| head.sequence_id as u64 == cursor.0),
            DependencyParent::Welcome(cursor) => topic_key(&self.welcome_topic())
                .ok()
                .and_then(|key| {
                    self.context
                        .db()
                        .pending_envelope(&key, *cursor)
                        .ok()
                        .flatten()
                })
                .is_some(),
        }
    }

    fn defer_head(&self, topic: &Topic, code: &'static str, blocked: bool) {
        if let Ok(key) = topic_key(topic)
            && let Ok(Some(head)) = self.context.db().first_pending_envelope(&key)
        {
            self.defer_cursor(topic, Cursor(head.sequence_id as u64), code, blocked);
        }
    }

    fn retry_blocked_head(&mut self, topic: &Topic, cursor: Cursor, blocked: bool) -> bool {
        let previous = self
            .topics
            .entry(topic.clone())
            .or_default()
            .processing
            .retried_head
            .replace(cursor);
        blocked && previous != Some(cursor)
    }

    fn defer_cursor(&self, topic: &Topic, cursor: Cursor, code: &'static str, blocked: bool) {
        if let Ok(key) = topic_key(topic) {
            let retry_at_ns = xmtp_common::time::now_ns().saturating_add(
                self.context
                    .incoming_runtime()
                    .policy()
                    .receiver_fallback_interval
                    .as_nanos() as i64,
            );
            let _ = self.context.db().set_incoming_retry(
                &key,
                cursor,
                &IncomingRetry {
                    retry_at_ns,
                    blocked,
                    error_code: Some(code.into()),
                    retry_expires_at_ns: None,
                },
            );
        }
    }

    fn dispatch_change(
        &mut self,
        group: MlsGroup<C>,
        change: crate::groups::change_callbacks::AppDataChange,
    ) {
        let group_id = group.group_id;
        let sender = self.callbacks.entry(group_id).or_insert_with(|| {
            let (sender, mut receiver) = mpsc::channel(32);
            let cancellation = self.context.cancellation_token().clone();
            xmtp_common::spawn(None, async move {
                loop {
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        change = receiver.recv() => match change {
                            Some(change) => group.dispatch_app_data_changes(vec![change]).await,
                            None => break,
                        },
                    }
                }
            });
            sender
        });
        if sender.try_send(change).is_err() {
            tracing::warn!(%group_id, "app data callback queue is full");
        }
    }
}
