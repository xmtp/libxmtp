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

impl<F> SubscriptionFactory for F
where
    F: Fn(TopicCursor, IncomingBatchLimits) -> SubscriptionFuture + MaybeSend + MaybeSync,
{
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        self(cursors, limits)
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct BidiSubscriptionFactory {
    transport: xmtp_api_backend::BidiTransport<xmtp_api_backend::BackendBinding>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SubscriptionFactory for BidiSubscriptionFactory {
    fn open(&self, cursors: TopicCursor, limits: IncomingBatchLimits) -> SubscriptionFuture {
        let transport = self.transport.clone();
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

#[derive(Default, PartialEq, Eq)]
enum TransportMode {
    #[default]
    Unary,
    ApiStream,
    #[cfg(not(target_arch = "wasm32"))]
    Bidi,
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
    },
    /// Includes the installation's welcome topic and every stored group.
    AllGroups,
}

/// Shares receipt and ordered processing across all readers in one context.
/// Database transactions provide the cross-process safety boundary.
pub struct IncomingCoordinator {
    commands: mpsc::UnboundedSender<Command>,
    generations: AtomicU64,
    /// Orders mode selection with factory commands so delayed setup cannot downgrade bidi.
    transport_mode: Mutex<TransportMode>,
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
    SetFactory(Arc<dyn SubscriptionFactory>),
    Wake,
}

impl IncomingCoordinator {
    /// Reuse the context's live controller, or start one with an owned context handle.
    pub fn for_context<C: XmtpSharedContext>(context: &C) -> Arc<Self> {
        let mut slot = context.incoming_coordinator().lock();
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
            transport_mode: Mutex::new(TransportMode::Unary),
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

    fn set_subscription_factory(&self, factory: Arc<dyn SubscriptionFactory>) {
        let _ = self.commands.send(Command::SetFactory(factory));
    }

    /// Enable the API stream transport unless bidi is already selected. Never downgrade bidi.
    /// Keep the returned handle until the first reader acquires its network lease.
    #[must_use = "keep this handle until the first network lease is acquired"]
    pub fn enable_stream_transport<C: XmtpSharedContext>(context: &C) -> Arc<Self>
    where
        C::ApiClient: xmtp_proto::api_client::XmtpMlsStreams,
    {
        use xmtp_proto::api_client::XmtpMlsStreams;
        let coordinator = Self::for_context(context);
        let mut mode = coordinator.transport_mode.lock();
        if *mode == TransportMode::Unary {
            *mode = TransportMode::ApiStream;
            let context = context.context_ref().clone();
            coordinator.set_subscription_factory(Arc::new(
                move |cursors: TopicCursor, limits| -> SubscriptionFuture {
                    let context = context.clone();
                    Box::pin(async move {
                        context
                            .api()
                            .api_client
                            .subscribe_envelopes_with_cursors(&cursors, limits)
                            .await
                            .map(|subscription| subscription.map_error(NetworkError::new))
                            .map_err(NetworkError::new)
                    })
                },
            ));
        }
        drop(mode);
        coordinator
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Use the shared native bidi wire for every scope in this context.
    /// Keep the returned handle until the first reader acquires its network lease.
    #[must_use = "keep this handle until the first network lease is acquired"]
    pub fn enable_bidi_transport<C: XmtpSharedContext>(context: &C) -> Arc<Self>
    where
        C::ApiClient: xmtp_proto::api_client::XmtpMlsBidiStreams
            + super::router_callbacks::ApiClientIdentity
            + Clone
            + 'static,
        <C::ApiClient as xmtp_proto::api_client::XmtpMlsBidiStreams>::SubscribeStream: 'static,
    {
        let coordinator = Self::for_context(context);
        let mut mode = coordinator.transport_mode.lock();
        if *mode != TransportMode::Bidi {
            *mode = TransportMode::Bidi;
            let transport =
                super::router_callbacks::shared_transport(context.api().api_client.clone());
            coordinator.set_subscription_factory(Arc::new(BidiSubscriptionFactory { transport }));
        }
        drop(mode);
        coordinator
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
