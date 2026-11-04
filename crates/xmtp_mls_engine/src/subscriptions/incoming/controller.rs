use super::*;
use crate::{groups::MlsGroup, subscriptions::incoming::stream_api::GroupSetGate};
use futures::{StreamExt, stream::FuturesUnordered};
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    hash::Hash,
    time::Duration,
};
use xmtp_common::{RetryableError, time::Instant};
use xmtp_db::{
    group::{GroupMembershipState, GroupScope},
    incoming_envelope::{NetworkEntityKind, StreamTopic, kind_pool_usage},
    prelude::*,
};
use xmtp_mls_core::mls_store::{MlsStore, ReceivedPage};
use xmtp_mls_identity::identity_updates::IdentityRequirement;
use xmtp_proto::types::{Cursor, IncomingEvent, OrderedEnvelopeBatch, TopicKind};

mod dependencies;
mod processing;
mod snapshots;
mod transport;
use transport::{RetryBackoff, Transport, TransportEvent, TransportState};
#[cfg(test)]
mod tests;
use dependencies::{DependencyKey, DependencyParent, DependencyRegistry};
use processing::{DependencyResult, PrefixHold};

mod opening;
mod reading;
mod reconcile;

/// Commands applied in one run-loop step before the next pass.
const MAX_COMMANDS_PER_PASS: usize = 256;

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
    /// Its topics come from the discovery read at this gate's generation.
    AllGroups(GroupSetGate),
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
            IncomingScope::AllGroups => (
                ScopeKind::AllGroups(GroupSetGate::default()),
                TopicCursor::new(),
            ),
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
    Fetched(Result<ReceivedPage, xmtp_mls_core::mls_store::MlsStoreError>),
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
    /// A rate-limited Query is not sent again before this instant (RATE-041).
    rate_limited_until: Option<Instant>,
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
    /// Topics whose target capture was refused with a rate limit, and when
    /// that delay ends. Other topics' captures are not held (RATE-041).
    target_floors: HashMap<Topic, Instant>,
    read_queue: VecDeque<Topic>,
    dependencies: FuturesUnordered<BoxDynFuture<'static, DependencyResult<C>>>,
    dependency_registry: DependencyRegistry,
    /// Deadline and final catch-up of each Welcome with a group-prefix watch.
    prefix_holds: HashMap<Cursor, PrefixHold>,
    welcome_blocked_scan: Option<Cursor>,
    /// When the next full blocked-Welcome rescan is due. Blocked rows are
    /// otherwise only revisited on a new controller, so a long-lived process
    /// would never reach their retention deadline.
    welcome_blocked_rescan_at: Option<Instant>,
    extra_topics: HashSet<Topic>,
    /// Group topics of the last all-groups discovery read. Kept while no
    /// all-groups scope is open, so a later one at the same generation reuses it.
    discovered_groups: HashSet<Topic>,
    /// The generation of `discovered_groups`.
    discovery_gate: GroupSetGate,
    /// The generation of the last check of every retired topic.
    retired_gate: GroupSetGate,
    #[cfg(test)]
    discovery_reads: usize,
    #[cfg(test)]
    retired_checks: usize,
    topics: HashMap<Topic, TopicSchedule>,
    /// Selected group topics with no stored group row yet.
    missing_group_topics: HashSet<Topic>,
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
        let rate_limit_hold = context.incoming_runtime().rate_limit_hold.clone();
        Self {
            context,
            commands,
            state: state.clone(),
            scopes: HashMap::new(),
            transport: Transport::new(factory, state.recovery.clone(), rate_limit_hold),
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
            target_floors: HashMap::new(),
            read_queue: VecDeque::new(),
            dependencies: FuturesUnordered::new(),
            dependency_registry: DependencyRegistry::default(),
            prefix_holds: HashMap::new(),
            welcome_blocked_scan: Some(Cursor(0)),
            welcome_blocked_rescan_at: None,
            extra_topics: HashSet::new(),
            discovered_groups: HashSet::new(),
            discovery_gate: GroupSetGate::default(),
            retired_gate: GroupSetGate::default(),
            #[cfg(test)]
            discovery_reads: 0,
            #[cfg(test)]
            retired_checks: 0,
            topics: HashMap::new(),
            missing_group_topics: HashSet::new(),
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
            // The next pass re-evaluates network interest.
            // implements: PROC-051
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
                    Some(command) => self.command_burst(command),
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

    /// Apply queued commands before the next pass, so a burst of scope changes
    /// costs one pass. At most
    /// `MAX_COMMANDS_PER_PASS` apply in one step; the rest wait for the next one,
    /// so a steady stream of commands cannot starve transport and processing.
    fn command_burst(&mut self, first: Command) {
        self.command(first);
        for _ in 1..MAX_COMMANDS_PER_PASS {
            let Ok(command) = self.commands.try_recv() else {
                break;
            };
            self.command(command);
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

fn stored_membership<C: XmtpSharedContext>(
    context: &C,
    group_id: &GroupId,
) -> Result<Option<GroupMembershipState>, IncomingError> {
    Ok(context
        .db()
        .find_group(group_id)
        .map_err(|error| IncomingError::Storage(error.into()))?
        .map(|group| group.membership_state))
}

/// Keep only interested entries in `queue`, in their order, then append each
/// interested entry it lacks, sorted by `sort_key`. Each entry costs one hash
/// lookup, so a pass stays linear in the number of interested topics.
fn merge_read_queue<T: Clone + Eq + Hash, K: Ord>(
    queue: &mut VecDeque<T>,
    interested: &HashSet<T>,
    sort_key: impl FnMut(&T) -> K,
) {
    queue.retain(|entry| interested.contains(entry));
    let queued: HashSet<&T> = queue.iter().collect();
    let mut additions: Vec<T> = interested
        .iter()
        .filter(|entry| !queued.contains(entry))
        .cloned()
        .collect();
    additions.sort_by_key(sort_key);
    queue.extend(additions);
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
        IncomingError::Store(xmtp_mls_core::mls_store::MlsStoreError::Storage(
            xmtp_db::StorageError::Stream(
                xmtp_db::stream_storage::StreamStorageError::Capacity { .. }
            )
        ))
    )
}
