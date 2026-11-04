//! One receiver and processing scheduler for each client context.

mod controller;
mod status;
pub mod stream_api;
use super::connection_state::ConnectionStates;

use crate::context::XmtpSharedContext;
use controller::Controller;
use parking_lot::Mutex;
pub use status::*;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{mpsc, watch};
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync, time::Instant};
use xmtp_proto::{
    api::NetworkError,
    types::{Cursor, GroupId, IncomingBatchLimits, IncomingSubscription, Topic, TopicCursor},
};

/// The pending open of one subscription a [`SubscriptionFactory`] makes.
pub type SubscriptionFuture =
    BoxDynFuture<'static, Result<IncomingSubscription<NetworkError>, NetworkError>>;

/// Exact method and wire inputs for a permanent server rejection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RequestKey {
    Bidi(TopicCursor),
    QueryNewest(HashSet<Topic>),
    /// Query can reduce its wire limit after a size error. Hold all limits at
    /// this cursor because the caller cannot see the final attempted limit.
    Query(Topic, Cursor),
}

pub(crate) type RejectedRequests = Arc<Mutex<Vec<(RequestKey, Arc<IncomingError>)>>>;

/// Opens one client's incoming subscriptions over its API client.
pub trait SubscriptionFactory: MaybeSend + MaybeSync {
    /// Subscribe to `cursors`, delivering batches within `limits`.
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture;

    /// Bind a fresh factory to one client's admitted snapshot when it needs it.
    fn bind_limits(
        &self,
        _limits: &xmtp_configuration::LimitsConfiguration,
    ) -> Option<Arc<dyn SubscriptionFactory>> {
        None
    }

    /// A suspended live receiver must not start automatic unary network work.
    fn is_suspended(&self) -> bool {
        false
    }
}

/// When this client's incoming controllers may next open a stream after a
/// refused open or a rate-limited stream end (RATE-041). The runtime keeps it,
/// so a replacement controller cannot reopen inside the delay. Refused reads
/// wait on their own request's floor instead.
#[derive(Debug, Default)]
pub(crate) struct RateLimitHold {
    opens: Option<Instant>,
}

pub(crate) type SharedRateLimitHold = Arc<Mutex<RateLimitHold>>;

impl RateLimitHold {
    fn until(delay: xmtp_common::time::Duration) -> Instant {
        xmtp_common::time::saturating_after(Instant::now(), delay)
    }

    /// No stream open until `delay` has passed. A pending later end stays.
    pub(crate) fn hold_opens(&mut self, delay: xmtp_common::time::Duration) {
        let until = Self::until(delay);
        self.opens = Some(self.opens.map_or(until, |at| at.max(until)));
    }

    /// The end of a pending open hold.
    pub(crate) fn opens_until(&self) -> Option<Instant> {
        self.opens.filter(|until| Instant::now() < *until)
    }
}

/// Shared client runtime. Transport capability and limits are fixed at construction.
/// The controller slot orders its final release with the next reader acquisition.
#[derive(Default)]
pub struct IncomingRuntime {
    policy: crate::core::policy::StreamPolicy,
    pub(crate) factory: Option<Arc<dyn SubscriptionFactory>>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) original_factory: Option<Arc<dyn SubscriptionFactory>>,
    pub(crate) coordinator: Mutex<Option<Arc<IncomingCoordinator>>>,
    /// A new controller on this client cannot resend a rejected wire request.
    pub(crate) rejected_requests: RejectedRequests,
    /// A new controller on this client cannot send inside a rate-limit delay.
    pub(crate) rate_limit_hold: SharedRateLimitHold,
}

impl IncomingRuntime {
    /// A runtime with `policy` whose controllers open subscriptions through
    /// `factory`; `None` gives a client that never receives.
    pub fn new(
        policy: crate::core::policy::StreamPolicy,
        factory: Option<Arc<dyn SubscriptionFactory>>,
    ) -> Self {
        Self {
            policy,
            #[cfg(any(test, feature = "test-utils"))]
            original_factory: factory.clone(),
            factory,
            coordinator: Mutex::new(None),
            rejected_requests: Arc::new(Mutex::new(Vec::new())),
            rate_limit_hold: Arc::default(),
        }
    }

    /// This runtime with every subscription open first passing `api`'s
    /// preflight check.
    pub fn with_preflight<A: xmtp_proto::api_client::XmtpBackendClient + 'static>(
        mut self,
        api: xmtp_api::preflight::GuardedApi<A>,
    ) -> Self {
        self.factory = self
            .factory
            .map(|inner| Arc::new(GuardedFactory { inner, api }) as Arc<dyn SubscriptionFactory>);
        self
    }

    /// The subscription factory this runtime was built with, before
    /// [`Self::with_preflight`] wrapped it.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn original_factory(&self) -> Option<Arc<dyn SubscriptionFactory>> {
        self.original_factory.clone()
    }

    /// The stream policy the runtime was built with.
    pub fn policy(&self) -> &crate::core::policy::StreamPolicy {
        &self.policy
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn active_lease_count_for_test(&self) -> usize {
        self.coordinator
            .lock()
            .as_ref()
            .map_or(0, |coordinator| coordinator.state.statuses.lock().len())
    }

    /// The subscription factory this runtime's controllers open through.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn factory_for_test(&self) -> Option<&Arc<dyn SubscriptionFactory>> {
        self.factory.as_ref()
    }

    /// The coordinator of this runtime's controllers, while one is held.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn coordinator_for_test(&self) -> Option<Arc<IncomingCoordinator>> {
        self.coordinator.lock().clone()
    }

    /// The errors of the wire requests a new controller may not resend.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn rejected_requests_for_test(&self) -> Vec<Arc<IncomingError>> {
        self.rejected_requests
            .lock()
            .iter()
            .map(|(_, error)| error.clone())
            .collect()
    }
}

struct GuardedFactory<A> {
    inner: Arc<dyn SubscriptionFactory>,
    api: xmtp_api::preflight::GuardedApi<A>,
}
impl<A: xmtp_proto::api_client::XmtpBackendClient + 'static> SubscriptionFactory
    for GuardedFactory<A>
{
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        let inner = self.inner.clone();
        let api = self.api.clone();
        Box::pin(async move {
            api.check_preflight().await.map_err(NetworkError::new)?;
            inner.open(cursors, limits).await
        })
    }
    fn is_suspended(&self) -> bool {
        self.inner.is_suspended()
    }
}

xmtp_common::if_wasm! {
/// The receiver of a browser API client: a lazy static stream. The factory
/// owns the API client, not the client context.
pub fn streaming_factory<A>(api: A) -> Arc<dyn SubscriptionFactory>
where
    A: xmtp_proto::api_client::XmtpMlsStreams + Clone + 'static,
{
    Arc::new(move |cursors: TopicCursor, limits| -> SubscriptionFuture {
        let api = api.clone();
        Box::pin(async move {
            api.subscribe_envelopes_with_cursors(&cursors, limits)
                .await
                .map(|subscription| subscription.map_error(NetworkError::new))
                .map_err(NetworkError::new)
        })
    })
}
}

impl<F> SubscriptionFactory for F
where
    F: Fn(TopicCursor, IncomingBatchLimits) -> SubscriptionFuture + MaybeSend + MaybeSync,
{
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        self(cursors, limits)
    }
}

xmtp_common::if_native! {
/// The receiver of a native API client: subscriptions over one lazily
/// started shared Bidi transport.
pub struct BidiSubscriptionFactory<A> {
    api: A,
    mutate: Option<xmtp_api_backend::MutateLimits>,
    transport: std::sync::OnceLock<Arc<xmtp_api_backend::BidiTransport<xmtp_api_backend::BackendBinding>>>,
}

impl<A> BidiSubscriptionFactory<A> {
    /// A factory over `api` whose transport starts with its first open.
    pub fn new(api: A) -> Self {
        Self { api, mutate: None, transport: std::sync::OnceLock::new() }
    }
}

/// The receiver of a native API client: a lazy shared Bidi transport. No
/// connection starts until the first receiving interest.
pub fn streaming_factory<A>(api: A) -> Arc<dyn SubscriptionFactory>
where
    BidiSubscriptionFactory<A>: SubscriptionFactory + 'static,
{
    Arc::new(BidiSubscriptionFactory::new(api))
}

impl<A> SubscriptionFactory for BidiSubscriptionFactory<A>
where
    A: xmtp_proto::api_client::XmtpMlsBidiStreams
        + stream_api::shared_wires::ApiClientIdentity
        + Clone
        + Send
        + Sync
        + 'static,
    A::SubscribeStream: 'static,
{
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        // Keep the shared transport alive between this client's receiving calls.
        // The process registry must not keep credentials after factories and subscriptions drop.
        let transport = self.transport.get_or_init(|| {
            let mutate = self.mutate.unwrap_or_else(|| {
                xmtp_api_backend::MutateLimits::from_limits(&self.api.bidi_limits())
            });
            stream_api::shared_wires::shared_transport_within(self.api.clone(), mutate)
        }).clone();
        Box::pin(async move {
            transport
                .lease_ordered(
                    cursors
                        .into_iter()
                        .map(|(topic, cursor)| (topic, cursor.0))
                        .collect(),
                    xmtp_api_backend::DEFAULT_LEASE_DEPTH,
                    limits,
                )
                .await
                .map(|lease| {
                    lease.into_incoming_subscription().map_error(move |error| {
                        // The event stream retains this exact shared transport until it drops.
                        let _owner = &transport;
                        NetworkError::new(error)
                    })
                })
                .map_err(NetworkError::new)
        })
    }

    fn bind_limits(
        &self,
        limits: &xmtp_configuration::LimitsConfiguration,
    ) -> Option<Arc<dyn SubscriptionFactory>> {
        Some(Arc::new(Self {
            api: self.api.clone(),
            mutate: Some(xmtp_api_backend::MutateLimits::from_limits(limits)),
            transport: std::sync::OnceLock::new(),
        }))
    }

    fn is_suspended(&self) -> bool {
        stream_api::shared_wires::bidi_streams_suspended()
    }
}
}

/// When a fixed-target operation may query beyond durable receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingReceivePolicy {
    /// Give a healthy receiver one bounded wait before using Query fallback.
    StreamFirst,
    /// Query immediately when receipt is below the target, even with a healthy receiver.
    ImmediateQuery,
}

/// Network interest held by one reader or bounded sync run.
#[derive(Clone, Debug)]
pub enum IncomingScope {
    /// Fixed topic membership; other stored groups are not enrolled.
    Topics(Vec<Topic>),
    /// Live group interest. A removed group can recover through its next Welcome.
    /// Welcome recovery does not add status targets or widen local delivery.
    Groups(Vec<GroupId>),
    /// Uses caller-sampled targets and one deadline across receiver changes.
    Barrier {
        /// Caller-sampled heads H; reconnects must not replace them.
        targets: TopicCursor,
        /// One absolute deadline, including target capture and processing.
        deadline: Instant,
        /// This operation's preference does not restrict reads required by other operations.
        receive_policy: IncomingReceivePolicy,
    },
    /// Includes the installation's welcome topic and every stored group.
    AllGroups,
    /// Keeps device-sync groups and new installation Welcomes receiving without app delivery.
    DeviceSyncGroups,
}

/// Shares receipt and ordered processing across all readers in one context.
/// Database transactions provide the cross-process safety boundary.
pub struct IncomingCoordinator {
    commands: mpsc::UnboundedSender<Command>,
    generations: AtomicU64,
    state: Arc<SharedState>,
}

struct SharedState {
    statuses: Mutex<HashMap<u64, IncomingStatus>>,
    recovery: Arc<Mutex<super::recovery::RecoverySnapshot>>,
    consumer_recovery: Mutex<HashMap<u64, super::recovery::RecoveryState>>,
    changed: watch::Sender<u64>,
    connection_states: ConnectionStates,
}

impl Default for SharedState {
    fn default() -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            statuses: Mutex::new(HashMap::new()),
            recovery: Arc::new(Mutex::new(Default::default())),
            consumer_recovery: Mutex::new(HashMap::new()),
            changed,
            connection_states: ConnectionStates::default(),
        }
    }
}

impl SharedState {
    fn notify(&self) {
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

enum Command {
    Acquire {
        id: u64,
        scope: IncomingScope,
    },
    Replace {
        id: u64,
        generation: u64,
        scope: IncomingScope,
    },
    Release(u64),
    Wake,
}

impl IncomingCoordinator {
    /// Reuse the context's live controller, or start one with an owned context handle.
    pub fn for_context<C: XmtpSharedContext>(context: &C) -> Arc<Self> {
        let mut slot = context.incoming_runtime().coordinator.lock();
        if let Some(coordinator) = slot
            .as_ref()
            .filter(|coordinator| !coordinator.commands.is_closed())
        {
            return coordinator.clone();
        }
        let (commands, receiver) = mpsc::unbounded_channel();
        let state = Arc::new(SharedState::default());
        state
            .connection_states
            .set_writer(Arc::new(xmtp_events::PublicBusWriter::new(
                context.events(),
            )));
        let coordinator = Arc::new(Self {
            commands,
            generations: AtomicU64::new(0),
            state: state.clone(),
        });
        let context = context.context_ref().clone();
        xmtp_common::spawn(None, Controller::new(context, receiver, state).run());
        *slot = Some(coordinator.clone());
        coordinator
    }

    /// Keep this scope active until the returned lease closes or drops.
    pub fn acquire(self: &Arc<Self>, scope: IncomingScope) -> IncomingLease {
        self.acquire_with_recovery(scope, false)
    }

    fn acquire_with_recovery(
        self: &Arc<Self>,
        scope: IncomingScope,
        bounded: bool,
    ) -> IncomingLease {
        let id = self.generations.fetch_add(1, Ordering::Relaxed) + 1;
        if bounded {
            self.state.connection_states.open(id);
        }
        self.state
            .statuses
            .lock()
            .insert(id, IncomingStatus::pending(id));
        let recovery = self.state.recovery.lock().clone();
        self.state.consumer_recovery.lock().insert(
            id,
            super::recovery::RecoveryState::new(recovery, bounded, Instant::now()),
        );
        let _ = self.commands.send(Command::Acquire { id, scope });
        IncomingLease {
            id,
            coordinator: self.clone(),
            changes: tokio::sync::Mutex::new(self.state.changed.subscribe()),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Request a fresh database check. This hint is not proof of processing.
    pub fn wake(&self) {
        let _ = self.commands.send(Command::Wake);
    }
}

/// Holds network interest and scope status; application acknowledgement is separate.
pub struct IncomingLease {
    id: u64,
    coordinator: Arc<IncomingCoordinator>,
    changes: tokio::sync::Mutex<watch::Receiver<u64>>,
    closed: std::sync::atomic::AtomicBool,
}

impl IncomingLease {
    #[cfg(test)]
    pub(crate) fn recovery_snapshot(&self) -> super::recovery::RecoverySnapshot {
        self.coordinator
            .state
            .consumer_recovery
            .lock()
            .get(&self.id)
            .map(|state| state.snapshot.clone())
            .unwrap_or_default()
    }

    /// Publish to this lease alone the failure a fresh recovery budget reports
    /// after `failures` failed attempts during an outage that has lasted
    /// `outage`. Returns whether that failure is exhaustion; nothing is
    /// published when the budget still holds.
    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub fn exhaust_recovery_for_test(
        &self,
        failures: u64,
        outage: xmtp_common::time::Duration,
    ) -> bool {
        let now = Instant::now();
        let mut budget = super::recovery::RecoveryBudget::new(
            &super::recovery::RecoverySnapshot::default(),
            now,
        );
        let snapshot = super::recovery::RecoverySnapshot {
            failures,
            ..Default::default()
        };
        let Err(failure) = budget.check(&snapshot, now + outage) else {
            return false;
        };
        let exhausted = matches!(failure, super::recovery::RecoveryFailure::Exhausted { .. });
        self.coordinator
            .state
            .consumer_recovery
            .lock()
            .get_mut(&self.id)
            .expect("test lease must have a recovery snapshot")
            .snapshot
            .terminal = Some(failure);
        self.coordinator.state.notify();
        exhausted
    }

    /// Read the latest status for this lease's current scope generation.
    pub fn snapshot(&self) -> IncomingStatus {
        self.coordinator
            .state
            .statuses
            .lock()
            .get(&self.id)
            .cloned()
            .unwrap_or_else(|| IncomingStatus::cancelled(self.id))
    }

    /// Return the old obligations as cancelled before starting the new generation.
    pub fn replace_scope(&self, scope: IncomingScope) -> IncomingStatus {
        let mut statuses = self.coordinator.state.statuses.lock();
        if self.closed.load(Ordering::Acquire) {
            return statuses
                .get(&self.id)
                .cloned()
                .unwrap_or_else(|| IncomingStatus::cancelled(self.id));
        }
        let generation = self.coordinator.generations.fetch_add(1, Ordering::Relaxed) + 1;
        let mut previous = statuses
            .get(&self.id)
            .cloned()
            .unwrap_or_else(|| IncomingStatus::cancelled(self.id));
        previous.cancel();
        let mut next = IncomingStatus::pending(generation);
        next.previous = Some(Box::new(previous.without_previous()));
        statuses.insert(self.id, next);
        let _ = self.coordinator.commands.send(Command::Replace {
            id: self.id,
            generation,
            scope,
        });
        drop(statuses);
        self.coordinator.state.notify();
        previous
    }

    pub fn replace_topics(&self, topics: Vec<Topic>) -> IncomingStatus {
        self.replace_scope(IncomingScope::Topics(topics))
    }

    /// A notification is a hint. Read a fresh snapshot after it arrives.
    pub async fn changed(&self) {
        let mut changes = self.changes.lock().await;
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let _ = changes.changed().await;
    }

    /// Subscribe before reading a status so an update cannot be missed.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.coordinator.state.changed.subscribe()
    }

    /// Wakes every reader that observes this lease's coordinator.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn notify_change_for_test(&self) {
        self.coordinator.state.notify();
    }

    /// Hold the shared observer to test that another reader uses its own observer.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn lock_change_receiver_for_test(
        &self,
    ) -> tokio::sync::MutexGuard<'_, watch::Receiver<u64>> {
        self.changes.lock().await
    }

    /// Release interest even if another task still holds this lease to watch status.
    pub fn close(&self) {
        let mut statuses = self.coordinator.state.statuses.lock();
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.coordinator.state.connection_states.close(self.id);
        statuses.remove(&self.id);
        self.coordinator
            .state
            .consumer_recovery
            .lock()
            .remove(&self.id);
        let _ = self.coordinator.commands.send(Command::Release(self.id));
        drop(statuses);
        self.coordinator.state.notify();
    }
}

impl Drop for IncomingLease {
    fn drop(&mut self) {
        self.close();
    }
}

/// A topics lease whose scope was replaced while one of its topics was
/// unfinished, beside a sibling lease of the same coordinator. Stream failure
/// tests read how the barrier failure reports the cancelled scope.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub struct ReplacedScope {
    /// The lease's scope generation before the replacement.
    pub old_generation: u64,
    /// The status that the replacement returned.
    pub previous: IncomingStatus,
    /// The lease's status after the replacement.
    pub current: IncomingStatus,
    /// The unfinished topic, with the scope generation it captured from
    /// `previous`.
    pub unfinished: super::barrier::BarrierTopic,
    sibling: IncomingLease,
    lease: IncomingLease,
    coordinator: Arc<IncomingCoordinator>,
}

#[cfg(any(test, feature = "test-utils"))]
impl ReplacedScope {
    /// Replace the scope of a lease whose topic received through 9 and
    /// processed through 8 of target 9.
    pub fn create() -> Self {
        use super::barrier::{BarrierCause, BarrierTopic};
        use xmtp_proto::types::Cursor;

        let (commands, _receiver) = mpsc::unbounded_channel();
        let coordinator = Arc::new(IncomingCoordinator {
            commands,
            generations: AtomicU64::new(0),
            state: Arc::new(SharedState::default()),
        });
        let sibling = coordinator.acquire(IncomingScope::AllGroups);
        let topic = Topic::new_group_message([7; 32]);
        let lease = coordinator.acquire(IncomingScope::Topics(vec![topic.clone()]));
        let old_generation = lease.snapshot().scope_generation;
        coordinator
            .state
            .statuses
            .lock()
            .get_mut(&lease.id)
            .expect("the lease's status")
            .topics
            .push(IncomingTopicStatus {
                topic: topic.clone(),
                scope_generation: old_generation,
                registration: IncomingRegistration::Active,
                target: Some(Cursor(9)),
                received: Cursor(9),
                processed: Cursor(8),
                unresolved_welcomes: 0,
                processing: IncomingProcessing::Pending,
                blocked: None,
                error: None,
            });
        let previous = lease.replace_scope(IncomingScope::Topics(vec![Topic::new_group_message(
            [8; 32],
        )]));
        let current = lease.snapshot();
        let mut unfinished = BarrierTopic {
            topic,
            scope_generation: None,
            target: Some(Cursor(9)),
            received: Cursor(9),
            processed: Cursor(8),
            unresolved_welcomes: Vec::new(),
            inactive: false,
            cause: Some(BarrierCause::ProcessingPending),
        };
        unfinished.capture_scope_generation(&previous);
        Self {
            old_generation,
            previous,
            current,
            unfinished,
            sibling,
            lease,
            coordinator,
        }
    }

    /// Close the replaced lease, then the sibling's status and whether the
    /// coordinator still holds it.
    pub fn close(&self) -> (IncomingStatus, bool) {
        self.lease.close();
        let registered = self
            .coordinator
            .state
            .statuses
            .lock()
            .contains_key(&self.sibling.id);
        (self.sibling.snapshot(), registered)
    }
}

#[cfg(test)]
mod lease_observer_tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn lease_change_after_snapshot_is_not_lost() {
        let (commands, _receiver) = mpsc::unbounded_channel();
        let coordinator = Arc::new(IncomingCoordinator {
            commands,
            generations: AtomicU64::new(0),
            state: Arc::new(SharedState::default()),
        });
        let lease = coordinator.acquire(IncomingScope::AllGroups);
        let _status = lease.snapshot();
        coordinator.state.notify();
        xmtp_common::time::timeout(std::time::Duration::from_secs(1), lease.changed()).await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn lease_change_wakes_independent_observers() {
        let (commands, _receiver) = mpsc::unbounded_channel();
        let coordinator = Arc::new(IncomingCoordinator {
            commands,
            generations: AtomicU64::new(0),
            state: Arc::new(SharedState::default()),
        });
        let lease = coordinator.acquire(IncomingScope::AllGroups);
        let mut first_changes = lease.subscribe_changes();
        let mut second_changes = lease.subscribe_changes();
        let _status = lease.snapshot();
        coordinator.state.notify();
        let (first, second) =
            xmtp_common::time::timeout(std::time::Duration::from_secs(1), async {
                futures::join!(first_changes.changed(), second_changes.changed())
            })
            .await?;
        first?;
        second?;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    mod preflight;
}
