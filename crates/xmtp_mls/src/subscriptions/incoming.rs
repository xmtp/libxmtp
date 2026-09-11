//! One receiver and processing scheduler for each client context.

mod controller;
mod status;

use crate::context::XmtpSharedContext;
use controller::Controller;
use parking_lot::Mutex;
pub use status::*;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{mpsc, watch};
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync, time::Instant};
use xmtp_proto::{
    api::NetworkError,
    types::{GroupId, IncomingBatchLimits, IncomingSubscription, Topic, TopicCursor},
};

pub(crate) type SubscriptionFuture =
    BoxDynFuture<'static, Result<IncomingSubscription<NetworkError>, NetworkError>>;

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
    pub(crate) coordinator: Mutex<Option<Arc<IncomingCoordinator>>>,
}

impl IncomingRuntime {
    pub(crate) fn new(
        policy: super::policy::StreamPolicy,
        factory: Option<Arc<dyn SubscriptionFactory>>,
    ) -> Self {
        Self {
            policy,
            factory,
            coordinator: Mutex::new(None),
        }
    }

    pub(crate) fn policy(&self) -> &super::policy::StreamPolicy {
        &self.policy
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
    pub(crate) api: A,
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
        let api = self.api.clone();
        Box::pin(async move {
            super::router_callbacks::shared_transport(api)
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
                    lease
                        .into_incoming_subscription()
                        .map_error(NetworkError::new)
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
    changed: watch::Sender<u64>,
}

impl Default for SharedState {
    fn default() -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            statuses: Mutex::new(HashMap::new()),
            changed,
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
        let id = self.generations.fetch_add(1, Ordering::Relaxed) + 1;
        self.state
            .statuses
            .lock()
            .insert(id, IncomingStatus::pending(id));
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

    /// Release interest even if another task still holds this lease to watch status.
    pub fn close(&self) {
        let mut statuses = self.coordinator.state.statuses.lock();
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        statuses.remove(&self.id);
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
