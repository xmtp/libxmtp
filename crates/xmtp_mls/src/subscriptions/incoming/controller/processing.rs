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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum DependencyKey {
    Identity(IdentityRequirement),
    Welcome(Cursor),
}

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
            if self.retired.contains(&topic) {
                continue;
            }
            match topic.kind() {
                TopicKind::GroupMessagesV1 => {
                    if self.waiting_identity.contains_key(&topic) {
                        continue;
                    }
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
                    let retry_blocked = match self
                        .context
                        .db()
                        .first_pending_envelope(&StreamTopic::group(group_id))
                    {
                        Ok(Some(head)) => self.retry_blocked_head(
                            &topic,
                            Cursor(head.sequence_id as u64),
                            head.blocked,
                        ),
                        Ok(_) => false,
                        Err(error) => {
                            self.topic_error(topic, error.into());
                            continue;
                        }
                    };
                    match group.process_pending_group_head_with_retry(
                        self.missing_references.get(&topic),
                        retry_blocked,
                    ) {
                        Ok(GroupHeadOutcome::Progress { cursor, result }) => {
                            tracing::trace!(%group_id, sequence_id = cursor.0, accepted = result.is_ok(), "group head completed");
                            self.missing_references.remove(&topic);
                            self.topic_errors.remove(&topic);
                            progress = true;
                            if result.as_ref().is_ok_and(|outcome| !outcome.group_active) {
                                self.retired.insert(topic.clone());
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
                            self.waiting_identity.insert(topic, requirement.clone());
                            self.queue_identity(requirement);
                        }
                        Ok(GroupHeadOutcome::Inactive) => {
                            self.retired.insert(topic);
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
            .welcome_prefixes
            .iter()
            .filter_map(|(cursor, (group, anchor))| {
                self.context
                    .db()
                    .topic_progress(&StreamTopic::group(*group))
                    .ok()
                    .filter(|progress| progress.processed >= *anchor)
                    .map(|_| *cursor)
            })
            .collect();
        for cursor in due {
            self.welcome_prefixes.remove(&cursor);
            match WelcomeService::new(self.context.clone()).retry_pending_welcome(cursor, None) {
                Ok(outcome) => progress |= self.welcome_outcome(outcome),
                Err(error) => self.topic_error(self.welcome_topic(), error.into()),
            }
        }
        // Requests that exceeded the dependency limit remain queued, not active.
        let queued: Vec<_> = self
            .waiting_identity
            .values()
            .chain(self.welcome_identity.values())
            .cloned()
            .collect();
        for requirement in queued {
            self.queue_identity(requirement);
        }
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
                self.welcome_identity.remove(&cursor);
                self.welcome_prefixes.remove(&cursor);
                self.topic_errors.remove(&self.welcome_topic());
                true
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::Identity(requirement),
            } => {
                self.welcome_identity.insert(cursor, requirement.clone());
                self.queue_identity(requirement);
                false
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::GroupPrefix { group_id, anchor },
            } => {
                self.welcome_prefixes.insert(cursor, (group_id, anchor));
                self.extra_topics.insert(Topic::new_group_message(group_id));
                false
            }
            WelcomeHeadOutcome::Need {
                cursor,
                requirement: WelcomeRequirement::Pointee,
            } => {
                let key = DependencyKey::Welcome(cursor);
                if self.dependencies.len() < self.context.stream_settings().max_dependency_requests
                    && self.dependency_keys.insert(key)
                {
                    let context = self.context.clone();
                    self.dependencies.push(Box::pin(async move {
                        DependencyResult::Welcome(
                            cursor,
                            WelcomeService::new(context)
                                .resolve_pending_welcome(cursor)
                                .await,
                        )
                    }));
                }
                false
            }
            WelcomeHeadOutcome::Waiting {
                cursor,
                code,
                blocked,
            } => {
                tracing::trace!(
                    sequence_id = cursor.0,
                    code,
                    blocked,
                    "Welcome waits for processing"
                );
                false
            }
            WelcomeHeadOutcome::Idle { .. } => false,
        }
    }

    fn queue_identity(&mut self, requirement: IdentityRequirement) {
        let key = DependencyKey::Identity(requirement.clone());
        if self.dependencies.len() >= self.context.stream_settings().max_dependency_requests
            || !self.dependency_keys.insert(key)
        {
            return;
        }
        let context = self.context.clone();
        self.dependencies.push(Box::pin(async move {
            let result = resolve_identity_requirement(&context, &requirement).await;
            DependencyResult::Identity(requirement, result)
        }));
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
        self.identity_heads
            .insert(requirement.clone(), topic.clone());
        self.queue_identity(requirement);
        Ok(())
    }

    pub(super) fn dependency_finished(&mut self, result: DependencyResult<C>) {
        match result {
            DependencyResult::Welcome(cursor, result) => {
                self.dependency_keys.remove(&DependencyKey::Welcome(cursor));
                match result {
                    Ok(outcome) => {
                        self.welcome_outcome(outcome);
                    }
                    Err(error) => self.topic_error(self.welcome_topic(), error.into()),
                }
            }
            DependencyResult::Identity(requirement, result) => {
                self.dependency_keys
                    .remove(&DependencyKey::Identity(requirement.clone()));
                let groups: Vec<_> = self
                    .waiting_identity
                    .iter()
                    .filter_map(|(topic, waiting)| {
                        (waiting == &requirement).then_some(topic.clone())
                    })
                    .collect();
                let welcomes: Vec<_> = self
                    .welcome_identity
                    .iter()
                    .filter_map(|(cursor, waiting)| (waiting == &requirement).then_some(*cursor))
                    .collect();
                for topic in &groups {
                    self.waiting_identity.remove(topic);
                }
                for cursor in &welcomes {
                    self.welcome_identity.remove(cursor);
                }
                match result {
                    Ok(()) => {
                        if let Some(topic) = self.identity_heads.remove(&requirement)
                            && let Ok(key) = topic_key(&topic)
                            && let Err(error) = self
                                .context
                                .db()
                                .complete_pending_envelope(&key, Cursor(requirement.sequence_id))
                        {
                            self.topic_error(topic, error.into());
                        }
                        for cursor in welcomes {
                            match WelcomeService::new(self.context.clone())
                                .retry_pending_welcome(cursor, None)
                            {
                                Ok(outcome) => {
                                    self.welcome_outcome(outcome);
                                }
                                Err(error) => self.topic_error(self.welcome_topic(), error.into()),
                            }
                        }
                    }
                    Err(IdentityDependencyError::MissingReference(_)) => {
                        for topic in groups {
                            self.missing_references.insert(topic, requirement.clone());
                        }
                        for cursor in welcomes {
                            match WelcomeService::new(self.context.clone())
                                .retry_pending_welcome(cursor, Some(&requirement))
                            {
                                Ok(outcome) => {
                                    self.welcome_outcome(outcome);
                                }
                                Err(error) => self.topic_error(self.welcome_topic(), error.into()),
                            }
                        }
                        if let Some(topic) = self.identity_heads.remove(&requirement) {
                            self.defer_head(&topic, "identity_invalid", true);
                            self.topic_errors.insert(
                                topic,
                                Arc::new(IncomingError::Identity(
                                    IdentityDependencyError::MissingReference(requirement),
                                )),
                            );
                        }
                    }
                    Err(error) => {
                        let blocked = !error.is_retryable();
                        let error = Arc::new(IncomingError::Identity(error));
                        for topic in groups {
                            self.defer_head(&topic, "identity_dependency", blocked);
                            self.topic_errors.insert(topic, error.clone());
                        }
                        for cursor in welcomes {
                            self.defer_cursor(
                                &self.welcome_topic(),
                                cursor,
                                "identity_dependency",
                                blocked,
                            );
                            self.topic_errors
                                .insert(self.welcome_topic(), error.clone());
                        }
                        if let Some(topic) = self.identity_heads.remove(&requirement) {
                            self.defer_head(&topic, "identity_invalid", blocked);
                            self.topic_errors.insert(topic, error);
                        }
                    }
                }
            }
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
        let previous = self.retried_blocked_heads.insert(topic.clone(), cursor);
        blocked && previous != Some(cursor)
    }

    fn defer_cursor(&self, topic: &Topic, cursor: Cursor, code: &'static str, blocked: bool) {
        if let Ok(key) = topic_key(topic) {
            let retry_at_ns = xmtp_common::time::now_ns().saturating_add(
                self.context
                    .stream_settings()
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
