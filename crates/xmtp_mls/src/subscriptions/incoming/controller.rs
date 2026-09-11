use super::*;
use crate::{
    identity_updates::IdentityRequirement,
    mls_store::{MlsStore, ReceivedPage},
};
use futures::{StreamExt, stream::FuturesUnordered};
use prost::Message;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};
use xmtp_common::{RetryableError, time::Instant};
use xmtp_db::{
    group::GroupQueryArgs,
    incoming_envelope::{NetworkEntityKind, StreamTopic},
    prelude::*,
};
use xmtp_proto::types::{Cursor, IncomingEvent, OrderedEnvelopeBatch, TopicKind};

mod processing;
mod snapshots;
#[cfg(test)]
mod tests;
use processing::{DependencyKey, DependencyResult};

struct Scope {
    generation: u64,
    scope: ScopeKind,
    topics: HashSet<Topic>,
    targets: TopicCursor,
    receipt_wait_started: Instant,
}

enum ScopeKind {
    Topics(Vec<Topic>),
    Groups(Vec<GroupId>),
    Barrier {
        deadline: Instant,
        receive_policy: IncomingReceivePolicy,
    },
    AllGroups,
    DeviceSyncGroups,
}

impl Scope {
    fn new(generation: u64, scope: IncomingScope) -> Self {
        let (scope, targets) = match scope {
            IncomingScope::Barrier {
                targets,
                deadline,
                receive_policy,
            } => (
                ScopeKind::Barrier {
                    deadline,
                    receive_policy,
                },
                targets,
            ),
            IncomingScope::Topics(topics) => (ScopeKind::Topics(topics), TopicCursor::new()),
            IncomingScope::Groups(groups) => (ScopeKind::Groups(groups), TopicCursor::new()),
            IncomingScope::AllGroups => (ScopeKind::AllGroups, TopicCursor::new()),
            IncomingScope::DeviceSyncGroups => (ScopeKind::DeviceSyncGroups, TopicCursor::new()),
        };
        Self {
            generation,
            scope,
            topics: HashSet::new(),
            targets,
            receipt_wait_started: Instant::now(),
        }
    }
}

enum Opened {
    Stream(IncomingSubscription<NetworkError>),
    Unary(TopicCursor),
}

type OpenFuture = BoxDynFuture<'static, Result<Opened, NetworkError>>;
type ReadFuture =
    BoxDynFuture<'static, (Topic, Result<ReceivedPage, crate::mls_store::MlsStoreError>)>;
type TargetsFuture = BoxDynFuture<'static, (Vec<(u64, u64)>, Result<TopicCursor, NetworkError>)>;

pub(super) struct Controller<C: XmtpSharedContext> {
    context: C,
    commands: mpsc::UnboundedReceiver<Command>,
    state: Arc<SharedState>,
    scopes: HashMap<u64, Scope>,
    factory: Option<Arc<dyn SubscriptionFactory>>,
    subscription: Option<IncomingSubscription<NetworkError>>,
    opening: Option<OpenFuture>,
    read: Option<ReadFuture>,
    targets: Option<TargetsFuture>,
    read_queue: VecDeque<Topic>,
    dependencies: FuturesUnordered<BoxDynFuture<'static, DependencyResult<C>>>,
    dependency_keys: HashSet<DependencyKey>,
    missing_references: HashMap<Topic, IdentityRequirement>,
    waiting_identity: HashMap<Topic, IdentityRequirement>,
    welcome_identity: HashMap<Cursor, IdentityRequirement>,
    identity_heads: HashMap<IdentityRequirement, Topic>,
    welcome_prefixes: HashMap<Cursor, (xmtp_proto::types::GroupId, Cursor)>,
    welcome_blocked_scan: Option<Cursor>,
    extra_topics: HashSet<Topic>,
    subscribed: HashSet<Topic>,
    active: HashSet<Topic>,
    paused: HashSet<Topic>,
    receive_blocked: HashSet<Topic>,
    retried_blocked_heads: HashMap<Topic, Cursor>,
    retired: HashSet<Topic>,
    topic_errors: HashMap<Topic, Arc<IncomingError>>,
    last_read: HashMap<Topic, Instant>,
    connection: IncomingConnection,
    connection_generation: u64,
    error: Option<Arc<IncomingError>>,
    storage_error: Option<Arc<IncomingError>>,
    open_at: Instant,
    source_failed: bool,
    callbacks: HashMap<
        xmtp_proto::types::GroupId,
        mpsc::Sender<crate::groups::change_callbacks::AppDataChange>,
    >,
}

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn new(
        context: C,
        commands: mpsc::UnboundedReceiver<Command>,
        state: Arc<SharedState>,
    ) -> Self {
        Self {
            context,
            commands,
            state,
            scopes: HashMap::new(),
            factory: None,
            subscription: None,
            opening: None,
            read: None,
            targets: None,
            read_queue: VecDeque::new(),
            dependencies: FuturesUnordered::new(),
            dependency_keys: HashSet::new(),
            missing_references: HashMap::new(),
            waiting_identity: HashMap::new(),
            welcome_identity: HashMap::new(),
            identity_heads: HashMap::new(),
            welcome_prefixes: HashMap::new(),
            welcome_blocked_scan: Some(Cursor(0)),
            extra_topics: HashSet::new(),
            subscribed: HashSet::new(),
            active: HashSet::new(),
            paused: HashSet::new(),
            receive_blocked: HashSet::new(),
            retried_blocked_heads: HashMap::new(),
            retired: HashSet::new(),
            topic_errors: HashMap::new(),
            last_read: HashMap::new(),
            connection: IncomingConnection::Connecting,
            connection_generation: 0,
            error: None,
            storage_error: None,
            open_at: Instant::now(),
            source_failed: false,
            callbacks: HashMap::new(),
        }
    }

    pub(super) async fn run(mut self) {
        let cancellation = self.context.cancellation_token().clone();
        loop {
            if self.stop_if_idle() {
                break;
            }
            self.storage_error = self.reconcile().err().map(Arc::new);
            let progress = self.process_ready();
            self.refresh_statuses();
            self.start_open();
            self.start_read();
            self.start_targets();
            let interval = if progress {
                Duration::ZERO
            } else {
                self.context.stream_settings().active_database_poll_interval
            };
            tokio::select! {
                _ = cancellation.cancelled() => break,
                command = self.commands.recv() => match command {
                    Some(command) => self.command(command),
                    None => break,
                },
                result = async { self.opening.as_mut().expect("open exists").await }, if self.opening.is_some() => {
                    self.opening = None;
                    self.opened(result);
                },
                event = async { self.subscription.as_mut().expect("subscription exists").events.next().await }, if self.subscription.is_some() => {
                    self.incoming(event);
                },
                result = async { self.read.as_mut().expect("read exists").await }, if self.read.is_some() => {
                    self.read = None;
                    self.read_finished(result);
                },
                result = async { self.targets.as_mut().expect("targets exist").await }, if self.targets.is_some() => {
                    self.targets = None;
                    self.targets_finished(result);
                },
                result = self.dependencies.next(), if !self.dependencies.is_empty() => {
                    if let Some(result) = result { self.dependency_finished(result); }
                },
                _ = xmtp_common::time::sleep(interval) => {},
            }
        }
        for status in self.state.statuses.lock().values_mut() {
            status.cancel();
        }
        self.state.notify();
    }

    fn stop_if_idle(&mut self) -> bool {
        if !self.scopes.is_empty() || !self.commands.is_empty() {
            return false;
        }
        // Acquisition clones the coordinator under this same registry lock.
        // A caller between lookup and acquire must keep this controller alive.
        let mut registered = self.context.incoming_coordinator().lock();
        match registered.as_ref() {
            Some(coordinator) if Arc::ptr_eq(&coordinator.state, &self.state) => {
                if Arc::strong_count(coordinator) != 1 {
                    return false;
                }
                self.commands.close();
                registered.take();
                true
            }
            _ => true,
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Acquire { id, scope } => {
                self.scopes.insert(id, Scope::new(id, scope));
            }
            Command::Replace {
                id,
                generation,
                scope,
            } => {
                if let Some(current) = self.scopes.get_mut(&id) {
                    *current = Scope::new(generation, scope);
                }
            }
            Command::Release(id) => {
                self.scopes.remove(&id);
            }
            Command::SetFactory(factory) => {
                self.factory = Some(factory);
                self.subscription = None;
                self.opening = None;
                self.subscribed.clear();
                self.active.clear();
                self.source_failed = false;
                self.open_at = Instant::now();
            }
            Command::Wake => {
                self.last_read.clear();
                self.open_at = Instant::now();
            }
        }
    }

    fn reconcile(&mut self) -> Result<(), IncomingError> {
        for topic in self.retired.iter().cloned().collect::<Vec<_>>() {
            let group_id = topic
                .identifier()
                .try_into()
                .map_err(|_| IncomingError::UnsupportedTopic)?;
            if MlsStore::new(self.context.clone())
                .group(&group_id)?
                .is_active()?
            {
                self.retired.remove(&topic);
                self.waiting_identity.remove(&topic);
                self.missing_references.remove(&topic);
                self.topic_errors.remove(&topic);
                self.last_read.remove(&topic);
            }
        }
        let discoveries = if self
            .scopes
            .values()
            .any(|scope| matches!(scope.scope, ScopeKind::AllGroups))
        {
            self.context
                .db()
                .find_groups(GroupQueryArgs {
                    include_sync_groups: true,
                    include_duplicate_dms: true,
                    ..Default::default()
                })
                .map_err(|error| IncomingError::Storage(error.into()))?
                .into_iter()
                .map(|group| Topic::new_group_message(group.id))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        let sync_groups = if self
            .scopes
            .values()
            .any(|scope| matches!(scope.scope, ScopeKind::DeviceSyncGroups))
        {
            self.context
                .db()
                .all_sync_groups()
                .map_err(|error| IncomingError::Storage(error.into()))?
                .into_iter()
                .map(|group| Topic::new_group_message(group.id))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        for scope in self.scopes.values_mut() {
            if let ScopeKind::Groups(groups) = &scope.scope {
                for group_id in groups {
                    let topic = Topic::new_group_message(group_id);
                    if !scope.topics.contains(&topic) {
                        // An empty pending queue cannot report an already inactive group.
                        let group = match MlsStore::new(self.context.clone()).group(group_id) {
                            Ok(group) => group,
                            Err(crate::mls_store::MlsStoreError::NotFound(_)) => continue,
                            Err(error) => return Err(error.into()),
                        };
                        if !group.is_active()? {
                            self.retired.insert(topic);
                        }
                    }
                }
            }
            scope.topics = match &scope.scope {
                ScopeKind::Topics(topics) => topics.iter().cloned().collect(),
                ScopeKind::Groups(groups) => groups.iter().map(Topic::new_group_message).collect(),
                ScopeKind::Barrier { .. } => scope.targets.keys().cloned().collect(),
                ScopeKind::AllGroups => discoveries
                    .iter()
                    .cloned()
                    .chain(std::iter::once(Topic::new_welcome_message(
                        self.context.installation_id(),
                    )))
                    .collect(),
                ScopeKind::DeviceSyncGroups => sync_groups
                    .iter()
                    .cloned()
                    .chain(std::iter::once(Topic::new_welcome_message(
                        self.context.installation_id(),
                    )))
                    .collect(),
            };
            scope
                .targets
                .retain(|topic, _| scope.topics.contains(topic));
        }
        self.extra_topics = self
            .welcome_prefixes
            .values()
            .map(|(group, _)| Topic::new_group_message(*group))
            .collect();
        if self.scopes.values().any(|scope| {
            matches!(scope.scope, ScopeKind::Groups(_))
                && scope
                    .topics
                    .iter()
                    .any(|topic| self.retired.contains(topic))
        }) {
            // This is a processing dependency, not a new fixed target or delivery scope.
            self.extra_topics
                .insert(Topic::new_welcome_message(self.context.installation_id()));
        }
        let interested = self.interested();
        self.waiting_identity
            .retain(|topic, _| interested.contains(topic));
        self.missing_references
            .retain(|topic, _| interested.contains(topic));
        self.topic_errors
            .retain(|topic, _| interested.contains(topic));
        self.last_read.retain(|topic, _| interested.contains(topic));
        self.callbacks
            .retain(|group, _| interested.contains(&Topic::new_group_message(*group)));
        self.read_queue.retain(|topic| interested.contains(topic));
        let mut additions: Vec<_> = interested
            .iter()
            .filter(|topic| !self.read_queue.contains(topic))
            .cloned()
            .collect();
        additions.sort_by_key(Topic::cloned_vec);
        self.read_queue.extend(additions);
        // A drained topic can rejoin reception. Other kinds keep their own capacity.
        for kind in [
            NetworkEntityKind::Group,
            NetworkEntityKind::Welcome,
            NetworkEntityKind::Identity,
        ] {
            let usage = self.context.db().pending_topic_usage(kind)?;
            let limits = self.context.stream_settings().incoming_limits(kind);
            let kind_rows: u64 = usage.iter().map(|usage| usage.rows.max(0) as u64).sum();
            let kind_bytes: u64 = usage.iter().map(|usage| usage.bytes.max(0) as u64).sum();
            self.paused.retain(|topic| {
                let Ok(stream_topic) = topic_key(topic) else {
                    return true;
                };
                if stream_topic.kind != kind {
                    return true;
                }
                let own = usage
                    .iter()
                    .find(|usage| usage.entity_id == stream_topic.entity_id);
                // Leave room for the next bounded batch before reception resumes.
                kind_rows > limits.kind.rows / 2
                    || kind_bytes > limits.kind.bytes / 2
                    || own.is_some_and(|usage| {
                        usage.rows.max(0) as u64 > limits.topic.rows / 2
                            || usage.bytes.max(0) as u64 > limits.topic.bytes / 2
                    })
            });
        }
        Ok(())
    }

    fn interested(&self) -> HashSet<Topic> {
        self.scopes
            .values()
            .flat_map(|scope| scope.topics.iter())
            .chain(self.extra_topics.iter())
            .filter(|topic| !self.retired.contains(*topic) && topic_key(topic).is_ok())
            .cloned()
            .collect()
    }

    fn fetched_limits(&self) -> IncomingBatchLimits {
        let settings = self.context.stream_settings();
        IncomingBatchLimits {
            max_rows: settings.max_fetched_rows as usize,
            max_bytes: settings.max_fetched_bytes.min(usize::MAX as u64) as usize,
        }
    }

    fn start_open(&mut self) {
        let topics: HashSet<_> = self
            .interested()
            .into_iter()
            .filter(|topic| !self.paused.contains(topic) && !self.receive_blocked.contains(topic))
            .collect();
        if topics != self.subscribed {
            self.subscription = None;
            self.opening = None;
            self.active.clear();
            self.subscribed = topics.clone();
            // A changed topic set must not bypass a failed receiver's retry delay.
        }
        if topics.is_empty()
            || self.source_failed
            || self.opening.is_some()
            || self.subscription.is_some()
            || (self.factory.is_none() && self.active == topics)
            || Instant::now() < self.open_at
        {
            return;
        }
        let topics: Vec<_> = topics.into_iter().collect();
        let cursors = match MlsStore::new(self.context.clone()).received_cursors(&topics) {
            Ok(cursors) => cursors,
            Err(error) => {
                self.error = Some(Arc::new(error.into()));
                return;
            }
        };
        self.connection_generation += 1;
        self.connection = if self.connection_generation == 1 {
            IncomingConnection::Connecting
        } else {
            IncomingConnection::Reconnecting
        };
        self.opening = Some(if let Some(factory) = &self.factory {
            let future = factory.open(cursors, self.fetched_limits());
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
        });
    }

    fn opened(&mut self, result: Result<Opened, NetworkError>) {
        match result {
            Ok(Opened::Stream(subscription)) => {
                self.subscription = Some(subscription);
                self.connection = IncomingConnection::Connected;
                self.error = None;
            }
            Ok(Opened::Unary(targets)) => {
                self.registered(targets);
                self.connection = IncomingConnection::Connected;
                self.error = None;
            }
            Err(error) => self.source_error(error),
        }
    }

    fn registered(&mut self, targets: TopicCursor) {
        self.active.extend(targets.keys().cloned());
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
    fn start_targets(&mut self) {
        if self.targets.is_some()
            || self.source_failed
            || self.live_suspended()
            || Instant::now() < self.open_at
        {
            return;
        }
        let mut scopes = Vec::new();
        let mut topics = HashSet::new();
        for (id, scope) in &self.scopes {
            let missing: Vec<_> = scope
                .topics
                .iter()
                .filter(|topic| self.active.contains(*topic) && !scope.targets.contains_key(*topic))
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

    fn targets_finished(
        &mut self,
        (scopes, result): (Vec<(u64, u64)>, Result<TopicCursor, NetworkError>),
    ) {
        match result {
            Ok(targets) => {
                for (id, generation) in scopes {
                    if let Some(scope) = self
                        .scopes
                        .get_mut(&id)
                        .filter(|scope| scope.generation == generation)
                    {
                        for (topic, target) in &targets {
                            if scope.topics.contains(topic) {
                                scope.targets.entry(topic.clone()).or_insert(*target);
                            }
                        }
                    }
                }
            }
            Err(error) => self.source_error(error),
        }
    }

    fn incoming(&mut self, event: Option<Result<IncomingEvent, NetworkError>>) {
        match event {
            Some(Ok(IncomingEvent::Registered { targets, .. })) => self.registered(targets),
            Some(Ok(IncomingEvent::OrderedBatch(batch))) => {
                let topic = batch.topic.clone();
                match self.admit_received_batch(batch) {
                    Ok(()) => {
                        self.topic_errors.remove(&topic);
                    }
                    Err(error) => self.receive_error(topic, error),
                }
            }
            Some(Err(error)) => self.source_error(error),
            None | Some(Ok(IncomingEvent::Disconnected)) => {
                self.subscription = None;
                self.active.clear();
                self.connection = IncomingConnection::Reconnecting;
                self.open_at =
                    Instant::now() + self.context.stream_settings().receiver_fallback_interval;
                self.last_read.clear();
            }
        }
    }

    /// Commit bounded prefixes. Uncommitted suffixes replay from durable receipt.
    fn admit_received_batch(&mut self, batch: OrderedEnvelopeBatch) -> Result<(), IncomingError> {
        let key = topic_key(&batch.topic)?;
        let limits = self.context.stream_settings().incoming_limits(key.kind);
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
            crate::mls_store::MlsStoreError::Api(xmtp_api::ApiError::Envelope(
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
            self.fetched_limits(),
        )
        .map_err(xmtp_api::ApiError::from)
        .map_err(crate::mls_store::MlsStoreError::from)?;
        let mut pending = batches
            .into_iter()
            .flat_map(|batch| batch.envelopes)
            .peekable();
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
                let next_bytes = bytes.saturating_add(envelope.encoded_len() as u64);
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
                .and_then(|envelope| envelope.meta.as_ref())
                .and_then(|meta| meta.cursor.as_ref())
                .map(|cursor| Cursor(cursor.sequence_id))
                .ok_or_else(|| {
                    crate::mls_store::MlsStoreError::Api(xmtp_api::ApiError::InvalidResponse(
                        "incoming cursor",
                    ))
                })?;
            let admitted = store.admit_incoming_batch(&chunk, limits)?;
            if let Some(subscription) = &self.subscription {
                subscription
                    .acknowledge_received([(batch.topic.clone(), admitted.received)].into());
            }
        }
        Ok(())
    }

    fn source_error(&mut self, error: NetworkError) {
        self.source_failed = !error.is_retryable();
        self.connection = if self.source_failed {
            IncomingConnection::Failed
        } else {
            IncomingConnection::Reconnecting
        };
        self.error = Some(Arc::new(error.into()));
        self.subscription = None;
        self.active.clear();
        self.open_at = Instant::now() + self.context.stream_settings().receiver_fallback_interval;
        self.last_read.clear();
    }

    fn start_read(&mut self) {
        if self.read.is_some() || self.source_failed {
            return;
        }
        let now = Instant::now();
        for _ in 0..self.read_queue.len() {
            let Some(topic) = self.read_queue.pop_front() else {
                break;
            };
            self.read_queue.push_back(topic.clone());
            if self.paused.contains(&topic)
                || self.receive_blocked.contains(&topic)
                || self.retired.contains(&topic)
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
            self.last_read.insert(topic.clone(), now);
            let context = self.context.clone();
            let limits = context.stream_settings().incoming_limits(key.kind);
            self.read = Some(Box::pin(async move {
                let result = MlsStore::new(context)
                    .receive_topics_once(std::slice::from_ref(&topic), limits)
                    .await;
                (topic, result)
            }));
            break;
        }
    }

    /// Explicit sync queries immediately. Stream-first targets keep one fixed receiver wait.
    fn read_due(
        &self,
        topic: &Topic,
        key: &StreamTopic,
        now: Instant,
    ) -> Result<bool, IncomingError> {
        let settings = self.context.stream_settings();
        let last = self.last_read.get(topic);
        let covered = self.subscription.is_some()
            && self.connection == IncomingConnection::Connected
            && self.active.contains(topic);
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
            let received = match received {
                Some(received) => received,
                None => {
                    let progress = self.context.db().topic_progress(key)?.received;
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
                first_read |= last.is_none_or(|last| *last < scope.receipt_wait_started);
            }
        }
        if unfinished {
            return Ok(ready
                && (first_read
                    || last.is_none_or(|last| {
                        now.duration_since(*last) >= settings.active_database_poll_interval
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
        Ok(
            last.is_none_or(|last| {
                now.duration_since(*last) >= settings.receiver_fallback_interval
            }),
        )
    }

    /// A Welcome barrier also owns the group prefix required by its pending parents.
    fn barrier_receipt_target(&self, topic: &Topic, targets: &TopicCursor) -> Option<Cursor> {
        let welcome = Topic::new_welcome_message(self.context.installation_id());
        let prefix = targets.get(&welcome).and_then(|target| {
            self.welcome_prefixes
                .iter()
                .filter(|(parent, (group, _))| {
                    **parent <= *target
                        && topic.kind() == TopicKind::GroupMessagesV1
                        && topic.identifier() == group.as_slice()
                })
                .map(|(_, (_, anchor))| *anchor)
                .max()
        });
        targets.get(topic).copied().into_iter().chain(prefix).max()
    }

    /// Bounded sync remains explicit network work; suspended live scopes do not poll.
    fn live_suspended(&self) -> bool {
        self.factory
            .as_ref()
            .is_some_and(|factory| factory.is_suspended())
    }

    fn read_finished(
        &mut self,
        (topic, result): (Topic, Result<ReceivedPage, crate::mls_store::MlsStoreError>),
    ) {
        match result {
            Ok(page) => {
                if page.has_more {
                    self.last_read.remove(&topic);
                }
                if let Some(subscription) = &self.subscription {
                    subscription.acknowledge_received(
                        page.admissions
                            .into_iter()
                            .map(|(topic, admitted)| (topic, admitted.received))
                            .collect(),
                    );
                }
                self.topic_errors.remove(&topic);
            }
            Err(error) => self.receive_error(topic, error.into()),
        }
    }

    /// Reopen from durable receipt after any failed admission. Keep pending work and scope targets.
    fn receive_error(&mut self, topic: Topic, error: IncomingError) {
        // The transport can advance its read cursor before storage commits. Drop
        // that registration even when reconciliation immediately clears a pause.
        self.subscription = None;
        self.opening = None;
        self.active.clear();
        if !self.source_failed {
            self.connection = IncomingConnection::Reconnecting;
        }
        let now = Instant::now();
        self.open_at = now + self.context.stream_settings().receiver_fallback_interval;
        self.last_read.insert(topic.clone(), now);
        if capacity(&error) {
            self.paused.insert(topic.clone());
        } else if !error.is_retryable() {
            self.receive_blocked.insert(topic.clone());
        }
        self.topic_error(topic, error);
    }

    fn topic_error(&mut self, topic: Topic, error: IncomingError) {
        if capacity(&error) {
            self.paused.insert(topic.clone());
        }
        self.topic_errors.insert(topic, Arc::new(error));
    }
}

pub(crate) fn topic_key(topic: &Topic) -> Result<StreamTopic, IncomingError> {
    let kind = match topic.kind() {
        TopicKind::GroupMessagesV1 => NetworkEntityKind::Group,
        TopicKind::WelcomeMessagesV1 => NetworkEntityKind::Welcome,
        TopicKind::IdentityUpdatesV1 => NetworkEntityKind::Identity,
        _ => return Err(IncomingError::UnsupportedTopic),
    };
    Ok(StreamTopic {
        entity_id: topic.identifier().to_vec(),
        kind,
    })
}

fn capacity(error: &IncomingError) -> bool {
    matches!(
        error,
        IncomingError::Store(crate::mls_store::MlsStoreError::Storage(
            xmtp_db::StorageError::Stream(
                xmtp_db::stream_storage::StreamStorageError::Capacity { .. }
            )
        ))
    )
}
