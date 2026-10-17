//! One receiver and processing scheduler for each client context.

mod controller;
mod status;
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

pub(crate) type SubscriptionFuture =
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

pub(crate) trait SubscriptionFactory: MaybeSend + MaybeSync {
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture;

    /// A suspended live receiver must not start automatic unary network work.
    fn is_suspended(&self) -> bool {
        false
    }
}

/// Shared client runtime. Transport capability and limits are fixed at construction.
/// The controller slot orders its final release with the next reader acquisition.
#[derive(Default)]
pub struct IncomingRuntime {
    policy: super::policy::StreamPolicy,
    pub(crate) factory: Option<Arc<dyn SubscriptionFactory>>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) original_factory: Option<Arc<dyn SubscriptionFactory>>,
    pub(crate) coordinator: Mutex<Option<Arc<IncomingCoordinator>>>,
    /// A new controller on this client cannot resend a rejected wire request.
    pub(crate) rejected_requests: RejectedRequests,
    /// A closed reader whose owner token must be released after storage repair.
    pub(crate) retired_delivery_owner: Mutex<Option<xmtp_db::delivery::DeliveryOwner>>,
}

impl IncomingRuntime {
    pub(crate) fn new(
        policy: super::policy::StreamPolicy,
        factory: Option<Arc<dyn SubscriptionFactory>>,
    ) -> Self {
        Self {
            policy,
            #[cfg(any(test, feature = "test-utils"))]
            original_factory: factory.clone(),
            factory,
            coordinator: Mutex::new(None),
            rejected_requests: Arc::new(Mutex::new(Vec::new())),
            retired_delivery_owner: Mutex::new(None),
        }
    }

    pub(crate) fn with_preflight<A: xmtp_proto::api_client::XmtpBackendClient + 'static>(
        mut self,
        api: xmtp_api::preflight::GuardedApi<A>,
    ) -> Self {
        self.factory = self
            .factory
            .map(|inner| Arc::new(GuardedFactory { inner, api }) as Arc<dyn SubscriptionFactory>);
        self
    }

    pub(crate) fn policy(&self) -> &super::policy::StreamPolicy {
        &self.policy
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn active_lease_count_for_test(&self) -> usize {
        self.coordinator
            .lock()
            .as_ref()
            .map_or(0, |coordinator| coordinator.state.statuses.lock().len())
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

impl<F> SubscriptionFactory for F
where
    F: Fn(TopicCursor, IncomingBatchLimits) -> SubscriptionFuture + MaybeSend + MaybeSync,
{
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        self(cursors, limits)
    }
}

xmtp_common::if_native! {
pub(crate) struct BidiSubscriptionFactory<A> {
    api: A,
    transport: std::sync::OnceLock<Arc<xmtp_api_backend::BidiTransport<xmtp_api_backend::BackendBinding>>>,
}

impl<A> BidiSubscriptionFactory<A> {
    pub(crate) fn new(api: A) -> Self {
        Self { api, transport: std::sync::OnceLock::new() }
    }
}

impl<A> SubscriptionFactory for BidiSubscriptionFactory<A>
where
    A: xmtp_proto::api_client::XmtpMlsBidiStreams
        + super::router_callbacks::ApiClientIdentity
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
            super::router_callbacks::shared_transport(self.api.clone())
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

    fn is_suspended(&self) -> bool {
        super::router_callbacks::bidi_streams_suspended()
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

    /// Application streams own a finite outage budget. Internal workers and
    /// bounded sync operations keep their existing lifetime and deadline rules.
    pub(crate) fn acquire_stream(self: &Arc<Self>, scope: IncomingScope) -> IncomingLease {
        self.acquire_with_recovery(scope, true)
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

    pub(crate) fn check_recovery(&self) -> Result<(), super::recovery::RecoveryFailure> {
        let mut consumers = self.coordinator.state.consumer_recovery.lock();
        match consumers.get_mut(&self.id) {
            Some(state) => state.check(Instant::now()),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn fail_recovery_for_test(&self, failure: super::recovery::RecoveryFailure) {
        self.coordinator
            .state
            .consumer_recovery
            .lock()
            .get_mut(&self.id)
            .expect("test lease must have a recovery snapshot")
            .snapshot
            .terminal = Some(failure);
        self.coordinator.state.notify();
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

    #[cfg(test)]
    pub(crate) fn notify_change_for_test(&self) {
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

#[cfg(test)]
mod lease_observer_tests {
    use super::*;

    // verifies: PROC-018
    #[xmtp_common::test(unwrap_try = true)]
    fn replacement_failure_keeps_cancelled_scope_generation() {
        use crate::subscriptions::barrier::{
            BarrierCause, BarrierError, BarrierFailure, BarrierTopic,
        };
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
            .get_mut(&lease.id)?
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
        assert_eq!(previous.scope_generation, old_generation);
        assert_eq!(previous.processing, IncomingProcessing::Cancelled);
        assert_eq!(previous.topics[0].scope_generation, old_generation);
        assert_eq!(previous.topics[0].processing, IncomingProcessing::Cancelled);
        assert!(current.scope_generation > old_generation);
        assert_eq!(current.previous.as_ref()?.scope_generation, old_generation);
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
        let details = crate::subscriptions::stream_failure::StreamBarrierFailure::from(
            &BarrierError::Incomplete {
                reason: BarrierFailure::Cancelled,
                unfinished: vec![unfinished],
            },
        );
        let reported = &details.unfinished[0];
        assert_eq!(
            reported.scope_generation.as_deref(),
            Some(old_generation.to_string().as_str())
        );
        assert_ne!(
            reported.scope_generation.as_deref(),
            Some(current.scope_generation.to_string().as_str())
        );
        assert_eq!(reported.target.as_deref(), Some("9"));
        assert_eq!(reported.received, "9");
        assert_eq!(reported.processed, "8");
        lease.close();
        assert_eq!(sibling.snapshot().processing, IncomingProcessing::Pending);
        assert!(coordinator.state.statuses.lock().contains_key(&sibling.id));
    }

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
