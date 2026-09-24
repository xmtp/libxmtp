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

mod dependencies;
mod processing;
mod snapshots;
mod transport;
use transport::{RetryBackoff, Transport, TransportEvent, TransportState};
#[cfg(test)]
mod tests;
use dependencies::{DependencyKey, DependencyParent, DependencyRegistry};
use processing::DependencyResult;

struct Scope {
    generation: u64,
    scope: ScopeKind,
    topics: HashSet<Topic>,
    targets: TopicCursor,
    target_error: Option<(HashSet<Topic>, Arc<IncomingError>)>,
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
            target_error: None,
            receipt_wait_started: Instant::now(),
        }
    }
}

enum Opened {
    Stream(IncomingSubscription<NetworkError>),
    Unary(TopicCursor),
}

type OpenFuture = BoxDynFuture<'static, Result<Opened, NetworkError>>;
enum ReadOutcome {
    Fetched(Result<ReceivedPage, crate::mls_store::MlsStoreError>),
    Rejected(Arc<IncomingError>),
}
type ReadResult = (Topic, Option<Cursor>, ReadOutcome);
type ReadFuture = BoxDynFuture<'static, ReadResult>;
type TargetsFuture = BoxDynFuture<'static, (Vec<(u64, u64)>, Result<TopicCursor, NetworkError>)>;

/// Receipt can continue while processing waits for a dependency.
#[derive(Default)]
struct TopicSchedule {
    receipt: ReceiptSchedule,
    processing: ProcessingSchedule,
    error: Option<Arc<IncomingError>>,
}

#[derive(Clone, Copy, Default)]
struct ReceiptSchedule {
    paused: bool,
    /// Earliest retry after a permanent receipt error. A permanent
    /// classification describes one response, so the topic always retries;
    /// only the delay grows.
    blocked_until: Option<Instant>,
    /// Consecutive permanent receipt errors, used to grow the retry delay.
    blocked_failures: u32,
    /// A rejected Query must not repeat from this unchanged durable cursor.
    rejected_at: Option<Cursor>,
    last_read: Option<Instant>,
}

impl ReceiptSchedule {
    /// True only while a scheduled retry has not come due.
    fn blocked(&self) -> bool {
        self.blocked_until.is_some_and(|at| Instant::now() < at)
    }

    /// True when a permanent error is still unresolved, due or not. Status
    /// reporting uses this so a retrying topic is not shown as healthy.
    fn failing(&self) -> bool {
        self.blocked_failures > 0
    }
}

#[derive(Default)]
struct ProcessingSchedule {
    retired: bool,
    retried_head: Option<Cursor>,
    missing_reference: Option<(Cursor, IdentityRequirement)>,
}

pub(super) struct Controller<C: XmtpSharedContext> {
    context: C,
    commands: mpsc::UnboundedReceiver<Command>,
    state: Arc<SharedState>,
    scopes: HashMap<u64, Scope>,
    transport: Transport,
    read: Option<ReadFuture>,
    targets: Option<TargetsFuture>,
    targets_request: Option<HashSet<Topic>>,
    rejected_requests: RejectedRequests,
    /// Classify one new bounded caller against retained open rejections even
    /// while the shared transport is already open or waiting to retry.
    classify_new_caller: bool,
    #[cfg(test)]
    open_cursor_reads: usize,
    #[cfg(test)]
    receipt_progress_reads: std::sync::atomic::AtomicUsize,
    targets_retry_at: Option<Instant>,
    read_queue: VecDeque<Topic>,
    dependencies: FuturesUnordered<BoxDynFuture<'static, DependencyResult<C>>>,
    dependency_registry: DependencyRegistry,
    welcome_blocked_scan: Option<Cursor>,
    /// When the next full blocked-Welcome rescan is due. Blocked rows are
    /// otherwise only revisited on a new controller, so a long-lived process
    /// would never reach their retention deadline.
    welcome_blocked_rescan_at: Option<Instant>,
    extra_topics: HashSet<Topic>,
    topics: HashMap<Topic, TopicSchedule>,
    storage_error: Option<Arc<IncomingError>>,
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
        let factory = context.incoming_runtime().factory.clone();
        let rejected_requests = context.incoming_runtime().rejected_requests.clone();
        Self {
            context,
            commands,
            state: state.clone(),
            scopes: HashMap::new(),
            transport: Transport::new(factory, state.recovery.clone()),
            read: None,
            targets: None,
            targets_request: None,
            rejected_requests,
            classify_new_caller: false,
            #[cfg(test)]
            open_cursor_reads: 0,
            #[cfg(test)]
            receipt_progress_reads: std::sync::atomic::AtomicUsize::new(0),
            targets_retry_at: None,
            read_queue: VecDeque::new(),
            dependencies: FuturesUnordered::new(),
            dependency_registry: DependencyRegistry::default(),
            welcome_blocked_scan: Some(Cursor(0)),
            welcome_blocked_rescan_at: None,
            extra_topics: HashSet::new(),
            topics: HashMap::new(),
            storage_error: None,
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
            self.retire_exhausted_streams();
            self.start_open();
            self.start_read();
            self.start_targets();
            let interval = if progress {
                Duration::ZERO
            } else {
                self.context
                    .incoming_runtime()
                    .policy()
                    .active_database_poll_interval
            };
            tokio::select! {
                _ = cancellation.cancelled() => break,
                command = self.commands.recv() => match command {
                    Some(command) => self.command(command),
                    None => break,
                },
                event = self.transport.next() => match event {
                    TransportEvent::Opened(result) => self.opened(result),
                    TransportEvent::Incoming(event) => self.incoming(event),
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
        for id in self.state.connection_states.open_ids() {
            self.state.connection_states.close(id);
        }
        self.state.notify();
    }

    fn stop_if_idle(&mut self) -> bool {
        if !self.scopes.is_empty() || !self.commands.is_empty() {
            return false;
        }
        // Acquisition clones the coordinator under this same registry lock.
        // A caller between lookup and acquire must keep this controller alive.
        let mut registered = self.context.incoming_runtime().coordinator.lock();
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

    fn retire_exhausted_streams(&mut self) {
        // Exhaustion retires only an application stream's interest. Internal
        // workers and barriers have no stream budget and remain registered.
        self.scopes.retain(|id, _| {
            self.state
                .consumer_recovery
                .lock()
                .get(id)
                .is_none_or(|state| state.snapshot.terminal.is_none())
        });
        for id in self.state.connection_states.open_ids() {
            if self
                .state
                .consumer_recovery
                .lock()
                .get(&id)
                .is_some_and(|state| state.snapshot.terminal.is_some())
            {
                self.state.connection_states.close(id);
            }
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Acquire { id, scope } => {
                if self
                    .state
                    .consumer_recovery
                    .lock()
                    .get(&id)
                    .is_some_and(crate::subscriptions::recovery::RecoveryState::is_bounded)
                {
                    self.targets_retry_at = None;
                    self.classify_new_caller = true;
                }
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
            Command::Wake => {
                self.clear_read_times();
                self.transport.wake();
            }
        }
    }

    fn reconcile(&mut self) -> Result<(), IncomingError> {
        let retired: Vec<_> = self
            .topics
            .iter()
            .filter(|(_, state)| state.processing.retired)
            .map(|(topic, _)| topic.clone())
            .collect();
        for topic in retired {
            let group_id = topic
                .identifier()
                .try_into()
                .map_err(|_| IncomingError::UnsupportedTopic)?;
            if MlsStore::new(self.context.clone())
                .group(&group_id)?
                .is_active()?
            {
                self.topics
                    .entry(topic.clone())
                    .or_default()
                    .processing
                    .retired = false;
                self.dependency_registry
                    .retain_parents(|parent| match parent {
                        DependencyParent::GroupHead(current, _) => current != &topic,
                        _ => true,
                    });
                self.topics
                    .entry(topic.clone())
                    .or_default()
                    .processing
                    .missing_reference = None;
                self.topics.entry(topic.clone()).or_default().error = None;
                self.topics
                    .entry(topic.clone())
                    .or_default()
                    .receipt
                    .last_read = None;
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
                            self.topics.entry(topic).or_default().processing.retired = true;
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
            .dependency_registry
            .prefixes()
            .map(|(_, group, _)| Topic::new_group_message(group))
            .collect();
        if self.scopes.values().any(|scope| {
            matches!(scope.scope, ScopeKind::Groups(_))
                && scope.topics.iter().any(|topic| self.is_retired(topic))
        }) {
            // This is a processing dependency, not a new fixed target or delivery scope.
            self.extra_topics
                .insert(Topic::new_welcome_message(self.context.installation_id()));
        }
        let interested = self.interested();
        self.dependency_registry
            .retain_parents(|parent| match parent {
                DependencyParent::GroupHead(topic, _)
                | DependencyParent::IdentityHead(topic, _) => interested.contains(topic),
                DependencyParent::Welcome(_) => true,
            });
        for (topic, state) in &mut self.topics {
            if !interested.contains(topic) {
                state.processing.missing_reference = None;
                state.error = None;
                state.receipt.last_read = None;
            }
        }
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
            let limits = self
                .context
                .incoming_runtime()
                .policy()
                .incoming_limits(kind);
            let kind_rows: u64 = usage.iter().map(|usage| usage.rows.max(0) as u64).sum();
            let kind_bytes: u64 = usage.iter().map(|usage| usage.bytes.max(0) as u64).sum();
            for (topic, state) in &mut self.topics {
                if !state.receipt.paused {
                    continue;
                }
                let Ok(stream_topic) = topic_key(topic) else {
                    continue;
                };
                if stream_topic.kind != kind {
                    continue;
                }
                let own = usage
                    .iter()
                    .find(|usage| usage.entity_id == stream_topic.entity_id);
                // Leave room for the next bounded batch before reception resumes.
                state.receipt.paused = kind_rows > limits.kind.rows / 2
                    || kind_bytes > limits.kind.bytes / 2
                    || own.is_some_and(|usage| {
                        usage.rows.max(0) as u64 > limits.topic.rows / 2
                            || usage.bytes.max(0) as u64 > limits.topic.bytes / 2
                    });
            }
        }
        Ok(())
    }

    fn interested(&self) -> HashSet<Topic> {
        self.scopes
            .values()
            .flat_map(|scope| scope.topics.iter())
            .chain(self.extra_topics.iter())
            .filter(|topic| !self.is_retired(topic) && topic_key(topic).is_ok())
            .cloned()
            .collect()
    }

    fn fetched_limits(&self) -> IncomingBatchLimits {
        let settings = self.context.incoming_runtime().policy();
        IncomingBatchLimits {
            max_rows: settings.max_fetched_rows as usize,
            max_bytes: settings.max_fetched_bytes.min(usize::MAX as u64) as usize,
        }
    }

    fn start_open(&mut self) {
        let topics: HashSet<_> = self
            .interested()
            .into_iter()
            .filter(|topic| !self.receipt(topic).paused && !self.receipt(topic).blocked())
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
        };
        self.transport.start(future, request);
    }

    fn opened(&mut self, result: Result<Opened, NetworkError>) {
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

    fn registered(&mut self, targets: TopicCursor) {
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
    fn start_targets(&mut self) {
        if self.targets.is_some() {
            return;
        }
        let mut scopes = Vec::new();
        let mut topics = HashSet::new();
        for (id, scope) in &self.scopes {
            let missing: Vec<_> = scope
                .topics
                .iter()
                .filter(|topic| {
                    self.transport.registered.contains(*topic)
                        && !scope.targets.contains_key(*topic)
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

    fn targets_finished(
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
                    self.targets_retry_at = Some(
                        Instant::now()
                            + self
                                .context
                                .incoming_runtime()
                                .policy()
                                .receiver_fallback_interval,
                    );
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
                if attempted {
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

    fn incoming(&mut self, event: Option<Result<IncomingEvent, NetworkError>>) {
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
    fn admit_received_batch(&mut self, batch: OrderedEnvelopeBatch) -> Result<(), IncomingError> {
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
            if let Some(subscription) = self.transport.subscription() {
                subscription
                    .acknowledge_received([(batch.topic.clone(), admitted.received)].into());
            }
        }
        Ok(())
    }

    fn source_error(&mut self, error: NetworkError) {
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

    fn open_error(&mut self, error: NetworkError) {
        let rejected = crate::subscriptions::recovery::rejected_request(&error);
        self.source_error(error);
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

    /// Unary reads are independent of the stream receiver. A failing or
    /// backing-off stream must not stop bounded Query recovery.
    // implements: API-284
    fn start_read(&mut self) {
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
                || (self.receipt(&topic).blocked() && self.receipt(&topic).rejected_at.is_none())
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
    fn read_due(
        &self,
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
            let received = match received {
                Some(received) => received,
                None => {
                    #[cfg(test)]
                    self.receipt_progress_reads
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    fn live_suspended(&self) -> bool {
        self.transport
            .factory
            .as_ref()
            .is_some_and(|factory| factory.is_suspended())
    }

    fn read_finished(&mut self, (topic, cursor, outcome): ReadResult) {
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
                let rejected = matches!(&error, crate::mls_store::MlsStoreError::Api(_))
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
    fn receive_error(&mut self, topic: Topic, error: IncomingError) {
        self.receive_error_shared(topic, Arc::new(error));
    }

    fn receive_error_shared(&mut self, topic: Topic, error: Arc<IncomingError>) {
        let query_failure = matches!(
            error.as_ref(),
            IncomingError::Store(crate::mls_store::MlsStoreError::Api(_))
        );
        // The transport can advance its read cursor before storage commits. Drop
        // that registration even when reconciliation immediately clears a pause.
        self.transport.disconnect(
            self.context
                .incoming_runtime()
                .policy()
                .receiver_fallback_interval,
        );
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
        } else {
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
                    state.record_query_failure(
                        &topic,
                        cause.clone(),
                        self.transport.recovery.failures,
                        now,
                    );
                }
            }
            drop(consumers);
            self.state.notify();
        }
    }

    fn receipt(&self, topic: &Topic) -> ReceiptSchedule {
        self.topics
            .get(topic)
            .map(|state| state.receipt)
            .unwrap_or_default()
    }

    fn is_retired(&self, topic: &Topic) -> bool {
        self.topics
            .get(topic)
            .is_some_and(|state| state.processing.retired)
    }

    /// A successful or retryable outcome ends a permanent-failure streak.
    fn clear_receipt_failure(&mut self, topic: &Topic) {
        if let Some(state) = self.topics.get_mut(topic) {
            state.receipt.blocked_until = None;
            state.receipt.blocked_failures = 0;
        }
    }

    fn clear_read_times(&mut self) {
        for state in self.topics.values_mut() {
            state.receipt.last_read = None;
        }
    }

    fn topic_error(&mut self, topic: Topic, error: IncomingError) {
        if capacity(&error) {
            self.topics.entry(topic.clone()).or_default().receipt.paused = true;
        }
        self.topics.entry(topic).or_default().error = Some(Arc::new(error));
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
