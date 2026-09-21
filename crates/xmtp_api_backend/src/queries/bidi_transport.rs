//! One backend subscription connection serves multiple topic leases.
//!
//! Each registration has a fixed catch-up target from Applied. Each lease
//! completes after its targets are met. Updates that lack acknowledgements
//! keep resume callers waiting, including updates that only remove topics.
//!
//! Messages carry no registration tag. Route them by their metadata topic.
//! Each lease keeps a monotonic delivery position above its requested floor.
//! A lease that needs older messages removes and re-adds the held topic.
//! Until the remove acknowledgement, only old holders receive queued messages.
//! At that boundary, current interest determines the re-add cursor and holders.
//! After the add acknowledgement, those holders receive the new registration.
//! The delivery positions discard overlap. Ordered leases retain at most one
//! wire frame while a full receive queue pauses wire reads for the connection.
//! This global gate affects every lease on the connection. A progressing
//! backlog can therefore delay reads for other leases longer than the pause
//! timeout.
//!
//! A connection failure keeps leases alive. Reconnect uses the current topic
//! set and the minimum durable receipt position of its leases. Suspend releases
//! the connection and keeps this state. Resume waits for all acknowledgements
//! and targets. A lease with no successful handoff for the pause deadline is
//! closed so its consumer can recover from storage.
//!
//! Ordered leases raise their floors only after durable receipt. A reconnect
//! resets delivery positions to these floors so uncommitted batches replay.
//! The consumer owns durable progress. This module does not decode MLS data
//! or promise exactly-once callbacks across a process crash.

use std::collections::{HashMap, HashSet, VecDeque};

use prost::Message;
use tokio::sync::{mpsc, oneshot};
use xmtp_common::rate_limit::Bucket;
use xmtp_common::time::Instant;
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync, RetryableError};
#[cfg(not(test))]
use xmtp_configuration::AUTH_LOCKOUT_COOLDOWN;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::{
    backend_v1::ServerEnvelope,
    types::{
        Cursor, IncomingBatchLimits, IncomingEvent, IncomingSubscription, OrderedEnvelopeBatch,
        Topic, TopicCursor,
    },
};

/// Keep the cool-down wait short in tests. Match the middleware's test value,
/// so the wire waits exactly one cool-down and no longer.
#[cfg(test)]
const AUTH_LOCKOUT_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(500);

use super::bidi::{BidiBinding, Connection, Event, TryMutateError};

/// Default event capacity for one lease. Slow consumers must recover from storage.
pub const DEFAULT_LEASE_DEPTH: usize = 64;
/// Retry a queued update when the connection is quiet and capacity may be free.
const OUTBOX_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
/// Bound a continuous stall while a consumer accepts no handoff. Thirty seconds
/// leaves half a 60-second sync budget for recovery from durable receipts. A
/// progressing backlog can keep the global wire gate active beyond this period.
const INCOMING_PAUSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn update_budget() -> Bucket {
    Bucket::new(
        xmtp_configuration::BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND,
        xmtp_configuration::BACKEND_DEFAULT_MAX_UPDATE_BURST,
    )
}

/// Maximum adds per backend update. The wire also has a separate topic cap.
pub(crate) const MAX_MUTATE_TOPICS: usize = xmtp_configuration::BACKEND_DEFAULT_MAX_UPDATE_ADDS;
/// Leave space for the request wrapper within the backend byte limit.
pub(crate) const MAX_MUTATE_BYTES: usize =
    xmtp_configuration::BACKEND_DEFAULT_MAX_REQUEST_BYTES - PER_ENTRY_OVERHEAD;
/// Conservative protobuf overhead per topic, including cursor and length fields.
const PER_ENTRY_OVERHEAD: usize = 64;

/// The interest-update frame shapes one deployment accepts. Adds and
/// removes have separate caps because the backend enforces them separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutateLimits {
    pub add_cap: usize,
    pub remove_cap: usize,
    pub byte_cap: usize,
}

impl Default for MutateLimits {
    fn default() -> Self {
        Self {
            add_cap: MAX_MUTATE_TOPICS,
            remove_cap: xmtp_configuration::BACKEND_DEFAULT_MAX_UPDATE_REMOVES,
            byte_cap: MAX_MUTATE_BYTES,
        }
    }
}

impl MutateLimits {
    /// Take the caps a deployment published, leaving the same room for the
    /// request wrapper the compiled default leaves.
    pub fn from_limits(limits: &xmtp_configuration::LimitsConfiguration) -> Self {
        Self {
            add_cap: limits.max_update_adds.max(1),
            remove_cap: limits.max_update_removes.max(1),
            byte_cap: limits
                .max_request_bytes
                .saturating_sub(PER_ENTRY_OVERHEAD)
                .max(1),
        }
    }
}

fn topic_wire_cost(topic: &Topic) -> usize {
    1 + topic.identifier().len() + PER_ENTRY_OVERHEAD
}

/// Keep input order while splitting at either the topic count or byte budget.
fn chunk_by_budget<T>(
    items: Vec<T>,
    max_count: usize,
    max_bytes: usize,
    wire_cost: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut current: Vec<T> = Vec::new();
    let mut current_bytes = 0usize;
    for item in items {
        let cost = wire_cost(&item);
        if !current.is_empty() && (current.len() >= max_count || current_bytes + cost > max_bytes) {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += cost;
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Split an add set into updates within the backend count and byte limits.
/// The caller must also enforce the total topic limit for its connection.
pub fn chunk_mutate_adds<C>(adds: Vec<(Topic, C)>) -> Vec<Vec<(Topic, C)>> {
    chunk_by_budget(adds, MAX_MUTATE_TOPICS, MAX_MUTATE_BYTES, |(topic, _)| {
        topic_wire_cost(topic)
    })
}

const RECONNECT_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
const RECONNECT_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(30);
const MIN_STABLE_UPTIME: std::time::Duration = std::time::Duration::from_secs(10);
/// Bound the background drain after request half-close.
const GRACEFUL_CLOSE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Keep the source error and its retry decision after type erasure.
#[derive(Debug)]
pub struct OpenError {
    retryable: bool,
    locked_out: bool,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl OpenError {
    pub fn new<E>(e: E) -> Self
    where
        E: std::error::Error + xmtp_common::RetryableError + Send + Sync + 'static,
    {
        Self {
            retryable: e.is_retryable(),
            locked_out: Self::locked_out_of(&e),
            source: Box::new(e),
        }
    }

    /// True while an authentication cool-down runs. The open cannot succeed now
    /// but will become possible again without any caller action.
    fn is_locked_out(&self) -> bool {
        self.locked_out
    }

    #[cfg(test)]
    pub fn retryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: true,
            locked_out: false,
            source: e.into(),
        }
    }

    #[cfg(test)]
    pub fn unretryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: false,
            locked_out: false,
            source: e.into(),
        }
    }
}

impl OpenError {
    /// Capture the lockout state of a concrete error before it is erased.
    fn locked_out_of<E: 'static>(error: &E) -> bool {
        (error as &dyn std::any::Any)
            .downcast_ref::<ApiClientError>()
            .is_some_and(ApiClientError::is_locked_out)
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.source)
    }
}

impl xmtp_common::RetryableError for OpenError {
    fn is_retryable(&self) -> bool {
        self.retryable
    }
}

/// Map backend envelopes to topics and positions for the lease ledger.
pub trait TransportBinding: BidiBinding
where
    Self::GroupMessage: Clone,
    Self::WelcomeMessage: Clone,
{
    /// A position that can represent each fixed backend target.
    type Cursor: Copy + Send + std::fmt::Debug + From<u64> + Into<u64> + 'static;

    /// Build an update with a nonzero ID. IDs increase on each connection.
    fn build_mutate(
        adds: impl IntoIterator<Item = (Topic, Self::Cursor)>,
        removes: impl IntoIterator<Item = Topic>,
        mutate_id: u64,
    ) -> Self::Mutate;

    fn group_topic(msg: &Self::GroupMessage) -> Option<Topic>;
    fn welcome_topic(msg: &Self::WelcomeMessage) -> Option<Topic>;

    fn group_cursor(msg: &Self::GroupMessage) -> Option<Self::Cursor>;
    fn welcome_cursor(msg: &Self::WelcomeMessage) -> Option<Self::Cursor>;
    fn group_envelope(msg: &Self::GroupMessage) -> &ServerEnvelope;
    fn welcome_envelope(msg: &Self::WelcomeMessage) -> &ServerEnvelope;
    fn advance(position: &mut Self::Cursor, delivered: Self::Cursor);
    /// Return true when the position is at or above the delivered cursor.
    fn covers(position: &Self::Cursor, delivered: &Self::Cursor) -> bool;
    /// Return the greatest cursor covered by both positions.
    fn meet(a: Self::Cursor, b: Self::Cursor) -> Self::Cursor;
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("the bidi transport is closed")]
    Closed,
    #[error("opening the bidi wire failed: {0}")]
    Open(#[source] OpenError),
    #[error("a lease must name at least one topic")]
    Empty,
    /// The lease exceeds the topic limit. This error is not retryable.
    #[error("the bidi wire topic limit would be exceeded")]
    TooManyTopics,
    #[error("invalid subscription frame: {0}")]
    Protocol(&'static str),
    /// One envelope cannot fit. Reopen from durable receipt positions.
    #[error("incoming subscription envelope exceeds its receive limit")]
    Capacity,
    /// The receive queue is full. Reopen from durable receipt positions.
    #[error("incoming subscription delivery queue is full")]
    Backpressure,
    #[error(transparent)]
    Wire(#[from] std::sync::Arc<super::bidi::ConnectionFailure>),
}

type IncomingFailure = std::sync::Arc<parking_lot::Mutex<Option<TransportError>>>;
/// One delivery fits the lease's row and byte limits, across all its topics.
type IncomingFrame = Result<Vec<IncomingEvent>, TransportError>;

impl xmtp_common::RetryableError for TransportError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Backpressure | Self::Capacity => true,
            Self::Open(e) => e.is_retryable(),
            Self::Wire(e) => e.is_retryable(),
            Self::Closed | Self::Empty | Self::TooManyTopics | Self::Protocol(_) => false,
        }
    }
}

/// Encrypted envelopes and initial catch-up completion for one lease.
pub enum LeaseEvent<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Every topic has met its initial target. This is emitted once per lease.
    CatchUpComplete,
    GroupMessages(Vec<B::GroupMessage>),
    WelcomeMessages(Vec<B::WelcomeMessage>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LeaseId(u64);

/// A cloneable handle to the connection and topic ledger task.
pub struct BidiTransport<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    cmds: mpsc::UnboundedSender<Cmd<B>>,
}

impl<B: TransportBinding> Clone for BidiTransport<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn clone(&self) -> Self {
        Self {
            cmds: self.cmds.clone(),
        }
    }
}

/// Local topic interest. Dropping the handle removes its interest immediately.
pub struct TopicLease<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    id: LeaseId,
    topics: Vec<Topic>,
    events: mpsc::Receiver<LeaseEvent<B>>,
    incoming: Option<mpsc::Receiver<IncomingFrame>>,
    incoming_pending: std::vec::IntoIter<IncomingEvent>,
    incoming_failure: IncomingFailure,
    cmds: mpsc::UnboundedSender<Cmd<B>>,
}

impl<B: TransportBinding> TopicLease<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Read the next event. A closed channel requires recovery from storage.
    pub async fn next(&mut self) -> Option<LeaseEvent<B>> {
        self.events.recv().await
    }

    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }

    /// Read raw ordered events, including a terminal receive error.
    pub async fn next_incoming(&mut self) -> Option<Result<IncomingEvent, TransportError>> {
        loop {
            if let Some(event) = self.incoming_pending.next() {
                return Some(Ok(event));
            }
            match self.incoming.as_mut()?.recv().await {
                Some(Ok(events)) => self.incoming_pending = events.into_iter(),
                Some(Err(error)) => return Some(Err(error)),
                None => return self.incoming_failure.lock().take().map(Err),
            }
        }
    }

    /// Call this only after the supplied positions commit to local storage.
    pub fn acknowledge_received(&self, cursors: TopicCursor) {
        let _ = self.cmds.send(Cmd::Received {
            id: self.id,
            cursors,
        });
    }

    /// Keep this lease alive until its owned event stream is dropped.
    pub fn into_incoming_subscription(self) -> IncomingSubscription<TransportError> {
        let cmds = self.cmds.clone();
        let id = self.id;
        let events = futures::stream::unfold(self, |mut lease| async move {
            lease.next_incoming().await.map(|event| (event, lease))
        });
        IncomingSubscription::new(Box::pin(events), move |cursors| {
            let _ = cmds.send(Cmd::Received { id, cursors });
        })
    }
}

impl<B: TransportBinding> Drop for TopicLease<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn drop(&mut self) {
        let _ = self.cmds.send(Cmd::Deref(self.id));
    }
}

impl<B: TransportBinding> BidiTransport<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    /// Open lazily through `opener`. A suspended transport waits for resume.
    pub fn new<O, Fut>(opener: O, initially_suspended: bool) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        Self::spawn(opener, initially_suspended, MutateLimits::default())
    }

    /// Open lazily, chunking interest updates to what the deployment published.
    pub fn new_within<O, Fut>(opener: O, initially_suspended: bool, mutate: MutateLimits) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        Self::spawn(opener, initially_suspended, mutate)
    }

    #[cfg(test)]
    pub(crate) fn new_with_chunk_limits<O, Fut>(
        opener: O,
        initially_suspended: bool,
        chunk_cap: usize,
        chunk_bytes: usize,
    ) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        Self::spawn(
            opener,
            initially_suspended,
            MutateLimits {
                add_cap: chunk_cap,
                remove_cap: chunk_cap,
                byte_cap: chunk_bytes,
            },
        )
    }

    fn spawn<O, Fut>(opener: O, initially_suspended: bool, mutate: MutateLimits) -> Self
    where
        O: Fn(B::Mutate) -> Fut + MaybeSend + MaybeSync + 'static,
        Fut: Future<Output = Result<Connection<B>, OpenError>> + MaybeSend + 'static,
    {
        let (cmds, cmds_rx) = mpsc::unbounded_channel();
        let opener: Opener<B> = Box::new(
            move |initial| -> BoxDynFuture<'static, Result<Connection<B>, OpenError>> {
                Box::pin(opener(initial))
            },
        );
        xmtp_common::spawn(
            None,
            run_ledger::<B>(
                opener,
                cmds_rx,
                cmds.clone().downgrade(),
                initially_suspended,
                mutate,
            ),
        );
        Self { cmds }
    }

    /// Register topics with exclusive floors and a bounded event channel.
    /// Refuse empty leases and leases that would exceed the wire topic limit.
    /// While suspended, register locally without opening a connection.
    pub async fn lease(
        &self,
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
    ) -> Result<TopicLease<B>, TransportError> {
        if subs.is_empty() {
            return Err(TransportError::Empty);
        }
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Lease { subs, depth, reply })
            .map_err(|_| TransportError::Closed)?;
        response.await.map_err(|_| TransportError::Closed)?
    }

    /// Register an ordered raw receiver. Receipt acknowledgements raise its resume floors.
    pub async fn lease_ordered(
        &self,
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        limits: IncomingBatchLimits,
    ) -> Result<TopicLease<B>, TransportError> {
        if subs.is_empty() {
            return Err(TransportError::Empty);
        }
        if limits.max_rows == 0 || limits.max_bytes == 0 {
            return Err(TransportError::Capacity);
        }
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::LeaseOrdered {
                subs,
                depth,
                limits,
                reply,
            })
            .map_err(|_| TransportError::Closed)?;
        response.await.map_err(|_| TransportError::Closed)?
    }

    /// Half-close the connection. Keep leases and positions for resume.
    #[xmtp_common::span(prefix = "bidi")]
    pub async fn suspend(&self) -> Result<(), TransportError> {
        self.enqueue_suspend()?
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// Queue suspend synchronously so callers can order lifecycle commands.
    pub fn enqueue_suspend(&self) -> Result<oneshot::Receiver<()>, TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Suspend { reply })
            .map_err(|_| TransportError::Closed)?;
        Ok(response)
    }

    /// Reopen and wait until every update is acknowledged and every target is met.
    /// A later suspend or removal of the last lease also releases this waiter.
    #[xmtp_common::span(prefix = "bidi")]
    pub async fn resume(&self) -> Result<(), TransportError> {
        self.enqueue_resume()?
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// Queue resume without waiting for catch-up. Return its completion receiver.
    pub fn enqueue_resume(&self) -> Result<oneshot::Receiver<()>, TransportError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Resume { reply })
            .map_err(|_| TransportError::Closed)?;
        Ok(response)
    }
}

enum Cmd<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    Lease {
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        reply: oneshot::Sender<Result<TopicLease<B>, TransportError>>,
    },
    LeaseOrdered {
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        limits: IncomingBatchLimits,
        reply: oneshot::Sender<Result<TopicLease<B>, TransportError>>,
    },
    Received {
        id: LeaseId,
        cursors: TopicCursor,
    },
    Deref(LeaseId),
    Suspend {
        reply: oneshot::Sender<()>,
    },
    Resume {
        reply: oneshot::Sender<()>,
    },
}

trait OpenWire<B: BidiBinding>: MaybeSend + MaybeSync {
    fn open(&self, initial: B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>>;
}

impl<B: BidiBinding, F> OpenWire<B> for F
where
    F: Fn(B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>>
        + MaybeSend
        + MaybeSync,
{
    fn open(&self, initial: B::Mutate) -> BoxDynFuture<'static, Result<Connection<B>, OpenError>> {
        self(initial)
    }
}

type Opener<B> = Box<dyn OpenWire<B>>;
#[derive(Debug, Clone, Copy)]
enum DeliveryKind {
    Group,
    Welcome,
}

/// An update that still needs its acknowledgement.
struct PendingUpdate<C> {
    adds: Vec<(Topic, C)>,
    removes: Vec<Topic>,
}

/// One topic registration on the current connection.
struct TopicRegistration<C> {
    target: Option<u64>,
    delivered: C,
    holders: HashSet<LeaseId>,
    state: RegistrationState,
}

enum RegistrationState {
    Adding,
    Active,
    /// Old holders receive queued frames until the remove acknowledgement.
    Removing,
}

/// The enclosing lease and topic map identify this obligation.
struct LeaseObligation {
    satisfied: bool,
}

struct LeaseState<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    floors: HashMap<Topic, B::Cursor>,
    delivered: HashMap<Topic, B::Cursor>,
    obligations: HashMap<Topic, LeaseObligation>,
    unmet: usize,
    notified: bool,
    events: mpsc::Sender<LeaseEvent<B>>,
    incoming: Option<(mpsc::Sender<IncomingFrame>, IncomingBatchLimits)>,
    incoming_failure: IncomingFailure,
    incoming_pending: VecDeque<Vec<IncomingEvent>>,
    paused_at: Option<Instant>,
    pause_entries: u64,
}

impl<B: TransportBinding> LeaseState<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn finish_pause(&mut self, now: Instant, reason: &'static str) {
        if let Some(started) = self.paused_at.take() {
            tracing::info!(
                pause_entries = self.pause_entries,
                pause_duration_ms = now.saturating_duration_since(started).as_millis() as u64,
                backlog_depth = self.incoming_pending.len(),
                reason,
                "bidi incoming: pause ended"
            );
        }
    }

    /// Return true only when the consumer must reopen from durable receipts.
    fn flush_incoming(&mut self, now: Instant) -> bool {
        let Some((sender, _)) = &self.incoming else {
            return false;
        };
        let sender = sender.clone();
        while !self.incoming_pending.is_empty() {
            let permit = match sender.try_reserve() {
                Ok(permit) => permit,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    if self.paused_at.is_some_and(|started| {
                        now.saturating_duration_since(started) >= INCOMING_PAUSE_TIMEOUT
                    }) {
                        tracing::warn!(
                            backlog_depth = self.incoming_pending.len(),
                            "bidi incoming: pause deadline exceeded; closing lease"
                        );
                        *self.incoming_failure.lock() = Some(TransportError::Backpressure);
                        self.finish_pause(now, "deadline");
                        return true;
                    }
                    if self.paused_at.is_none() {
                        self.paused_at = Some(now);
                        self.pause_entries += 1;
                        tracing::info!(
                            pause_entries = self.pause_entries,
                            backlog_depth = self.incoming_pending.len(),
                            "bidi incoming: paused"
                        );
                    }
                    return false;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    *self.incoming_failure.lock() = Some(TransportError::Backpressure);
                    self.finish_pause(now, "consumer_closed");
                    return true;
                }
            };
            let Some(events) = self.incoming_pending.pop_front() else {
                unreachable!()
            };
            let positions: Vec<_> = events
                .iter()
                .filter_map(|event| {
                    let IncomingEvent::OrderedBatch(batch) = event else {
                        return None;
                    };
                    let last = batch
                        .envelopes
                        .last()?
                        .meta
                        .as_ref()?
                        .cursor
                        .as_ref()?
                        .sequence_id;
                    Some((batch.topic.clone(), last))
                })
                .collect();
            // No await or fallible operation separates handoff and cursor update.
            // Pending chunks never change delivered. Floors change only on receipt.
            permit.send(Ok(events));
            for (topic, last) in positions {
                if let Some(delivered) = self.delivered.get_mut(&topic) {
                    B::advance(delivered, last.into());
                }
            }
            // A successful handoff ends the stall, even with chunks pending.
            self.finish_pause(now, "progress");
        }
        self.finish_pause(now, "drained");
        false
    }
}

struct Ledger<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    leases: HashMap<LeaseId, LeaseState<B>>,
    by_topic: HashMap<Topic, HashSet<LeaseId>>,
    last_seen: HashMap<Topic, B::Cursor>,
    registrations: HashMap<Topic, TopicRegistration<B::Cursor>>,
    pending_updates: HashMap<u64, PendingUpdate<B::Cursor>>,
    dirty_topics: HashSet<Topic>,
    failed_incoming: HashSet<LeaseId>,
    next_lease: u64,
    next_update: u64,
    mutate: MutateLimits,
}

impl<B: TransportBinding> Default for Ledger<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn default() -> Self {
        Self {
            leases: HashMap::new(),
            by_topic: HashMap::new(),
            last_seen: HashMap::new(),
            registrations: HashMap::new(),
            pending_updates: HashMap::new(),
            dirty_topics: HashSet::new(),
            failed_incoming: HashSet::new(),
            next_lease: 0,
            next_update: 0,
            mutate: MutateLimits::default(),
        }
    }
}

impl<B: TransportBinding> Ledger<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    fn has_pending_incoming(&self) -> bool {
        self.leases
            .values()
            .any(|lease| !lease.incoming_pending.is_empty())
    }

    fn flush_incoming(&mut self, now: Instant) -> Vec<LeaseId> {
        self.leases
            .iter_mut()
            .filter_map(|(id, lease)| {
                if !lease.incoming_pending.is_empty() {
                    self.dirty_topics.extend(lease.floors.keys().cloned());
                }
                lease.flush_incoming(now).then_some(*id)
            })
            .collect()
    }

    /// Raise reconnect floors from committed receipt, never from stream delivery.
    fn received(&mut self, id: LeaseId, cursors: TopicCursor) {
        let Some(lease) = self.leases.get_mut(&id) else {
            return;
        };
        if lease.incoming.is_none() {
            return;
        }
        for (topic, cursor) in cursors {
            if cursor.0 > i64::MAX as u64 {
                continue;
            }
            if let Some(floor) = lease.floors.get_mut(&topic) {
                B::advance(floor, cursor.0.into());
                if let Some(delivered) = lease.delivered.get_mut(&topic) {
                    B::advance(delivered, cursor.0.into());
                }
                self.dirty_topics.insert(topic);
            }
        }
    }

    /// Emit the accepted read positions and fixed targets before raw data.
    fn incoming_registered(&mut self, id: LeaseId, topics: impl IntoIterator<Item = Topic>) {
        let Some(lease) = self.leases.get_mut(&id) else {
            return;
        };
        let Some(_) = &lease.incoming else {
            return;
        };
        let mut starts = TopicCursor::new();
        let mut targets = TopicCursor::new();
        for topic in topics {
            let Some(registration) = self.registrations.get(&topic) else {
                continue;
            };
            let Some(target) = registration.target else {
                continue;
            };
            let Some(start) = lease.delivered.get(&topic) else {
                continue;
            };
            starts.insert(topic.clone(), Cursor((*start).into()));
            targets.insert(topic, Cursor(target));
        }
        if !starts.is_empty() {
            lease
                .incoming_pending
                .push_back(vec![IncomingEvent::Registered { starts, targets }]);
            if lease.flush_incoming(Instant::now()) {
                self.failed_incoming.insert(id);
            }
        }
    }

    /// Require exactly one fixed target for each topic added by this update.
    fn valid_targets(&self, id: u64, targets: &[(Topic, u64)]) -> bool {
        let Some(update) = self.pending_updates.get(&id) else {
            return false;
        };
        let unique: HashSet<_> = targets.iter().map(|(topic, _)| topic).collect();
        targets.len() == update.adds.len()
            && unique.len() == targets.len()
            && targets.iter().all(|(topic, target)| {
                *target <= i64::MAX as u64 && update.adds.iter().any(|(added, _)| added == topic)
            })
    }

    /// Validate the full frame before copying it into any raw receive channel.
    fn demux_incoming<M>(
        &mut self,
        messages: &[M],
        envelope_of: impl Fn(&M) -> &ServerEnvelope,
    ) -> Result<Vec<LeaseId>, &'static str> {
        let mut positions = HashMap::new();
        for message in messages {
            let envelope = envelope_of(message);
            let meta = envelope.meta.as_ref().ok_or("metadata")?;
            let topic =
                Topic::parse(&meta.topic.as_ref().ok_or("topic")?.topic).map_err(|_| "topic")?;
            let (_, cursor, _) =
                crate::envelope::metadata(meta, topic.kind()).map_err(|_| "metadata")?;
            let Some(registration) = self.registrations.get(&topic) else {
                continue;
            };
            if registration.target.is_none() {
                return Err("messages before Applied");
            }
            let previous = positions
                .entry(topic)
                .or_insert_with(|| Cursor(registration.delivered.into()));
            if cursor <= *previous {
                return Err("cursor order");
            }
            *previous = cursor;
        }
        let mut dropped = Vec::new();
        for (id, lease) in &mut self.leases {
            let Some((_, limits)) = &lease.incoming else {
                continue;
            };
            let mut selected = Vec::new();
            let mut impossible = false;
            for message in messages {
                let envelope = envelope_of(message);
                let meta = envelope.meta.as_ref().ok_or("metadata")?;
                let topic = Topic::parse(&meta.topic.as_ref().ok_or("topic")?.topic)
                    .map_err(|_| "topic")?;
                let Some(registration) = self.registrations.get(&topic) else {
                    continue;
                };
                if !registration.holders.contains(id) {
                    continue;
                }
                let Some(delivered) = lease.delivered.get(&topic) else {
                    continue;
                };
                if meta.cursor.as_ref().ok_or("cursor")?.sequence_id <= (*delivered).into() {
                    continue;
                }
                // Validate every selected envelope before sending any part of the frame.
                // A zero row budget cannot admit even one envelope either.
                impossible |= limits.max_rows == 0 || envelope.encoded_len() > limits.max_bytes;
                selected.push((topic, envelope));
            }
            if impossible {
                tracing::error!(
                    max_rows = limits.max_rows,
                    max_bytes = limits.max_bytes,
                    "bidi incoming: envelope cannot fit receive limits; closing lease"
                );
                *lease.incoming_failure.lock() = Some(TransportError::Capacity);
                dropped.push(*id);
                continue;
            }
            if selected.is_empty() {
                continue;
            }
            // The wire is gated while pending work exists. This retains at most
            // one validated wire frame per lease, split across bounded deliveries.
            // Project batch boundaries separately: these are NOT delivery cursors.
            let mut projected = lease.delivered.clone();
            for chunk in chunk_by_budget(
                selected,
                limits.max_rows,
                limits.max_bytes,
                |(_, envelope)| envelope.encoded_len(),
            ) {
                let mut by_topic: HashMap<Topic, Vec<ServerEnvelope>> = HashMap::new();
                for (topic, envelope) in chunk {
                    by_topic.entry(topic).or_default().push(envelope.clone());
                }
                let mut events = Vec::with_capacity(by_topic.len());
                for (topic, envelopes) in by_topic {
                    let position = projected.get_mut(&topic).ok_or("lease topic")?;
                    let after = Cursor((*position).into());
                    let last = envelopes
                        .last()
                        .and_then(|envelope| envelope.meta.as_ref()?.cursor.as_ref())
                        .ok_or("cursor")?
                        .sequence_id;
                    B::advance(position, last.into());
                    events.push(IncomingEvent::OrderedBatch(OrderedEnvelopeBatch {
                        topic,
                        after,
                        envelopes,
                    }));
                }
                lease.incoming_pending.push_back(events);
            }
            if lease.flush_incoming(Instant::now()) {
                dropped.push(*id);
            }
        }
        Ok(dropped)
    }

    fn next_update_id(&mut self) -> u64 {
        self.next_update += 1;
        self.next_update
    }

    /// Create local interest before opening or changing a connection.
    fn register(
        &mut self,
        subs: &[(Topic, B::Cursor)],
        events: mpsc::Sender<LeaseEvent<B>>,
    ) -> LeaseId {
        self.next_lease += 1;
        let id = LeaseId(self.next_lease);
        let floors: HashMap<_, _> = subs.iter().cloned().collect();
        for topic in floors.keys() {
            self.by_topic.entry(topic.clone()).or_default().insert(id);
        }
        self.leases.insert(
            id,
            LeaseState {
                delivered: floors.clone(),
                obligations: floors
                    .keys()
                    .map(|topic| (topic.clone(), LeaseObligation { satisfied: false }))
                    .collect(),
                unmet: floors.len(),
                floors,
                notified: false,
                events,
                incoming: None,
                incoming_failure: IncomingFailure::default(),
                incoming_pending: VecDeque::new(),
                paused_at: None,
                pause_entries: 0,
            },
        );
        id
    }

    /// Forget connection state. Keep each lease's delivery guard.
    fn reset_wire(&mut self) {
        self.registrations.clear();
        self.pending_updates.clear();
        self.dirty_topics.clear();
        for lease in self.leases.values_mut() {
            lease.obligations.clear();
            lease.unmet = lease.floors.len();
            if lease.incoming.is_some() {
                lease.finish_pause(Instant::now(), "wire_reset");
                lease.incoming_pending.clear();
                lease.delivered.clone_from(&lease.floors);
            }
        }
    }

    fn reset_obligation(&mut self, id: LeaseId, topic: &Topic) {
        if let Some(lease) = self.leases.get_mut(&id) {
            if let Some(obligation) = lease.obligations.get_mut(topic) {
                if obligation.satisfied {
                    obligation.satisfied = false;
                    lease.unmet += 1;
                }
            } else {
                lease
                    .obligations
                    .insert(topic.clone(), LeaseObligation { satisfied: false });
            }
        }
    }

    /// Reopen at the meet of the observed position and all requested floors.
    ///
    /// A floor never rises, so this meet is the floor in practice. See the
    /// module header for why that makes reopen cost grow with lease age.
    fn resume_cursor(&self, topic: &Topic) -> Option<B::Cursor> {
        let holders = self.by_topic.get(topic)?;
        holders
            .iter()
            .filter_map(|id| self.leases.get(id)?.floors.get(topic).copied())
            .reduce(B::meet)
    }

    fn resume_adds(&self) -> Vec<(Topic, B::Cursor)> {
        self.by_topic
            .keys()
            .filter_map(|topic| Some((topic.clone(), self.resume_cursor(topic)?)))
            .collect()
    }

    /// Record acknowledgements before the updates enter the connection queue.
    fn prepare_adds(&mut self, adds: Vec<(Topic, B::Cursor)>) -> Vec<(u64, B::Mutate)> {
        let chunks = chunk_by_budget(
            adds,
            self.mutate.add_cap,
            self.mutate.byte_cap,
            |(topic, _)| topic_wire_cost(topic),
        );
        chunks
            .into_iter()
            .map(|adds| {
                for (topic, cursor) in &adds {
                    self.dirty_topics.insert(topic.clone());
                    let holders = self.by_topic.get(topic).cloned().unwrap_or_default();
                    for id in &holders {
                        self.reset_obligation(*id, topic);
                    }
                    self.registrations
                        .entry(topic.clone())
                        .or_insert_with(|| TopicRegistration {
                            target: None,
                            delivered: *cursor,
                            holders,
                            state: RegistrationState::Adding,
                        });
                }
                let id = self.next_update_id();
                let update = B::build_mutate(adds.clone(), [], id);
                self.pending_updates.insert(
                    id,
                    PendingUpdate {
                        adds,
                        removes: vec![],
                    },
                );
                (id, update)
            })
            .collect()
    }

    fn prepare_removes(&mut self, removes: Vec<Topic>) -> Vec<(u64, B::Mutate)> {
        chunk_by_budget(
            removes,
            self.mutate.remove_cap,
            self.mutate.byte_cap,
            topic_wire_cost,
        )
        .into_iter()
        .map(|removes| {
            let id = self.next_update_id();
            let update = B::build_mutate([], removes.clone(), id);
            self.pending_updates.insert(
                id,
                PendingUpdate {
                    adds: vec![],
                    removes,
                },
            );
            (id, update)
        })
        .collect()
    }

    /// Join an active registration, or remove it before requesting older data.
    fn join(&mut self, id: LeaseId, subs: Vec<(Topic, B::Cursor)>) -> Vec<(u64, B::Mutate)> {
        let mut adds = Vec::new();
        let mut removes = Vec::new();
        let mut joined = Vec::new();
        for (topic, floor) in subs {
            self.dirty_topics.insert(topic.clone());
            let Some(registration) = self.registrations.get_mut(&topic) else {
                adds.push((topic, floor));
                continue;
            };
            match &mut registration.state {
                RegistrationState::Removing => {}
                _ if !B::covers(&floor, &registration.delivered) => {
                    registration.state = RegistrationState::Removing;
                    removes.push(topic);
                }
                _ => {
                    registration.holders.insert(id);
                    joined.push(topic);
                }
            }
        }
        let mut updates = self.prepare_removes(removes);
        updates.extend(self.prepare_adds(adds));
        self.incoming_registered(id, joined);
        updates
    }

    /// Apply ordered acknowledgement boundaries and return topics to re-add.
    fn applied(&mut self, id: u64, targets: Vec<(Topic, u64)>) -> Vec<(Topic, B::Cursor)> {
        let Some(update) = self.pending_updates.remove(&id) else {
            tracing::warn!(id, "received Applied for an unknown update");
            return Vec::new();
        };
        let targets: HashMap<_, _> = targets.into_iter().collect();
        let mut readds = Vec::new();
        for topic in update.removes {
            self.registrations.remove(&topic);
            if let Some(cursor) = self.resume_cursor(&topic) {
                readds.push((topic, cursor));
            }
        }
        let mut registered: HashMap<LeaseId, Vec<Topic>> = HashMap::new();
        for (topic, _) in update.adds {
            self.dirty_topics.insert(topic.clone());
            if let Some(registration) = self.registrations.get_mut(&topic) {
                if let Some(target) = targets.get(&topic) {
                    registration.target = Some(*target);
                }
                // An absent target leaves an existing registration unchanged.
                if matches!(registration.state, RegistrationState::Adding) {
                    registration.state = RegistrationState::Active;
                }
                for holder in &registration.holders {
                    registered.entry(*holder).or_default().push(topic.clone());
                }
            }
        }
        for (holder, topics) in registered {
            self.incoming_registered(holder, topics);
        }
        readds
    }

    /// Check changed topics only. Send completion after their message batches.
    fn recheck(&mut self) -> Vec<LeaseId> {
        let failed: Vec<_> = self.failed_incoming.drain().collect();
        let mut candidates = HashSet::new();
        for topic in self.dirty_topics.drain() {
            let Some(registration) = self.registrations.get(&topic) else {
                continue;
            };
            let Some(target) = registration.target else {
                continue;
            };
            let target_cursor = B::Cursor::from(target);
            for id in &registration.holders {
                candidates.insert(*id);
                let Some(lease) = self.leases.get_mut(id) else {
                    continue;
                };
                let Some(obligation) = lease.obligations.get_mut(&topic) else {
                    continue;
                };
                if !obligation.satisfied
                    && (target == 0
                        || if lease.incoming.is_some() {
                            lease
                                .delivered
                                .get(&topic)
                                .is_some_and(|position| B::covers(position, &target_cursor))
                        } else {
                            B::covers(&registration.delivered, &target_cursor)
                        }
                        || lease
                            .floors
                            .get(&topic)
                            .is_some_and(|floor| B::covers(floor, &target_cursor)))
                {
                    obligation.satisfied = true;
                    lease.unmet -= 1;
                }
            }
        }
        candidates
            .into_iter()
            .filter_map(|id| {
                let lease = self.leases.get_mut(&id)?;
                if !lease.notified && lease.unmet == 0 {
                    if lease.incoming.is_none()
                        && lease.events.try_send(LeaseEvent::CatchUpComplete).is_err()
                    {
                        return Some(id);
                    }
                    lease.notified = true;
                }
                None
            })
            .chain(failed)
            .collect()
    }

    /// Remove local interest immediately. The caller queues wire removals.
    fn deref(&mut self, id: LeaseId) -> Vec<Topic> {
        let Some(mut lease) = self.leases.remove(&id) else {
            return vec![];
        };
        lease.finish_pause(Instant::now(), "lease_removed");
        let mut removes = Vec::new();
        for topic in lease.floors.keys() {
            if let Some(holders) = self.by_topic.get_mut(topic) {
                holders.remove(&id);
                if holders.is_empty() {
                    self.by_topic.remove(topic);
                    self.last_seen.remove(topic);
                    removes.push(topic.clone());
                }
            }
            if let Some(registration) = self.registrations.get_mut(topic) {
                registration.holders.remove(&id);
            }
        }
        removes
    }

    /// Route one frame by topic. A lease's position only moves forward.
    /// Reserve channel capacity before copying a lease's payload batch.
    fn demux<M: Clone>(
        &mut self,
        messages: Vec<M>,
        kind: DeliveryKind,
        topic_of: impl Fn(&M) -> Option<Topic>,
        cursor_of: impl Fn(&M) -> Option<B::Cursor>,
        event: impl Fn(Vec<M>) -> LeaseEvent<B>,
    ) -> Vec<LeaseId> {
        let mut batches: HashMap<LeaseId, Vec<&M>> = HashMap::new();
        for message in &messages {
            let Some(topic) = topic_of(message) else {
                continue;
            };
            let Some(registration) = self.registrations.get_mut(&topic) else {
                continue;
            };
            // A new registration cannot deliver before its add acknowledgement.
            if registration.target.is_none() {
                continue;
            }
            self.dirty_topics.insert(topic.clone());
            let cursor = cursor_of(message);
            if let Some(cursor) = cursor {
                B::advance(&mut registration.delivered, cursor);
                B::advance(
                    self.last_seen.entry(topic.clone()).or_insert(cursor),
                    cursor,
                );
            }
            for id in &registration.holders {
                let Some(lease) = self.leases.get_mut(id) else {
                    continue;
                };
                if lease.incoming.is_some() {
                    continue;
                }
                if let Some(cursor) = cursor {
                    let Some(position) = lease.delivered.get_mut(&topic) else {
                        continue;
                    };
                    if B::covers(position, &cursor) {
                        continue;
                    }
                    B::advance(position, cursor);
                }
                batches.entry(*id).or_default().push(message);
            }
        }
        batches
            .into_iter()
            .filter_map(|(id, batch)| {
                let lease = self.leases.get(&id)?;
                if let Ok(permit) = lease.events.try_reserve() {
                    permit.send(event(batch.into_iter().cloned().collect()));
                    None
                } else {
                    tracing::warn!(
                        lease = id.0,
                        ?kind,
                        "closing a lease whose delivery channel is full"
                    );
                    Some(id)
                }
            })
            .collect()
    }

    fn caught_up(&self) -> bool {
        self.pending_updates.is_empty()
            && !self.has_pending_incoming()
            && self.leases.values().all(|lease| lease.unmet == 0)
    }
}

/// The task owns both the wire and the ledger. Send updates with try_mutate.
/// Never wait for command capacity here: this task must keep reading events.
enum Step<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    Cmd(Option<Cmd<B>>),
    Wire(Option<Event<B::GroupMessage, B::WelcomeMessage>>),
    Retry,
    Reconnect,
}

async fn run_ledger<B: TransportBinding>(
    opener: Opener<B>,
    cmds: mpsc::UnboundedReceiver<Cmd<B>>,
    lease_cmds: mpsc::WeakUnboundedSender<Cmd<B>>,
    initially_suspended: bool,
    mutate: MutateLimits,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    LedgerTask {
        opener,
        cmds,
        lease_cmds,
        ledger: Ledger {
            mutate,
            ..Ledger::default()
        },
        conn: None,
        reconnect_delay: RECONNECT_INITIAL_DELAY,
        reconnect_at: tokio::time::Instant::now(),
        wire_opened_at: None,
        wire_span: None,
        suspended: initially_suspended,
        wire_opens: 0,
        resume_notify: Vec::new(),
        outbox: Outbox::default(),
        update_budget: update_budget(),
        deferred: std::collections::VecDeque::new(),
    }
    .run()
    .await
}

enum Flow {
    Continue,
    Shutdown,
}

struct LedgerTask<B: TransportBinding>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    opener: Opener<B>,
    cmds: mpsc::UnboundedReceiver<Cmd<B>>,
    lease_cmds: mpsc::WeakUnboundedSender<Cmd<B>>,
    ledger: Ledger<B>,
    conn: Option<Connection<B>>,
    reconnect_delay: std::time::Duration,
    reconnect_at: tokio::time::Instant,
    wire_opened_at: Option<tokio::time::Instant>,
    wire_span: Option<tracing::Span>,
    suspended: bool,
    wire_opens: u64,
    resume_notify: Vec<oneshot::Sender<()>>,
    outbox: Outbox<B::Mutate>,
    update_budget: Bucket,
    deferred: std::collections::VecDeque<Cmd<B>>,
}

impl<B: TransportBinding> LedgerTask<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    async fn run(mut self) {
        loop {
            // A finite snapshot lets queued leases share an update without
            // letting a continuous command producer starve wire events.
            let queued = self.deferred.len() + self.cmds.len();
            for _ in 0..queued {
                let cmd = self
                    .deferred
                    .pop_front()
                    .or_else(|| self.cmds.try_recv().ok());
                let Some(cmd) = cmd else { break };
                if let Flow::Shutdown = self.command(cmd).await {
                    return;
                }
            }
            let mut dropped = self.ledger.flush_incoming(Instant::now());
            dropped.extend(self.ledger.recheck());
            let removes = self.drop_leases(dropped);
            self.retire(removes);
            self.settle_idle_waiters();
            self.settle_caught_up_waiters();
            self.flush_outbox();
            let flow = match self.next_step().await {
                Step::Retry => Flow::Continue,
                Step::Cmd(None) => self.shutdown(),
                Step::Cmd(Some(cmd)) => self.command(cmd).await,
                Step::Wire(Some(event)) => self.wire_event(event),
                Step::Wire(None) => self.wire_died(),
                Step::Reconnect => self.reconnect().await,
            };
            if let Flow::Shutdown = flow {
                return;
            }
        }
    }

    async fn command(&mut self, cmd: Cmd<B>) -> Flow {
        match cmd {
            Cmd::Lease { subs, depth, reply } => self.lease(subs, depth, None, reply).await,
            Cmd::LeaseOrdered {
                subs,
                depth,
                limits,
                reply,
            } => self.lease(subs, depth, Some(limits), reply).await,
            Cmd::Received { id, cursors } => {
                self.ledger.received(id, cursors);
                Flow::Continue
            }
            Cmd::Deref(id) => self.deref(id),
            Cmd::Suspend { reply } => self.suspend(reply),
            Cmd::Resume { reply } => self.resume(reply).await,
        }
    }

    async fn next_step(&mut self) -> Step<B> {
        if let Some(cmd) = self.deferred.pop_front() {
            return Step::Cmd(Some(cmd));
        }
        let retry_after = self.update_budget.wait().max(OUTBOX_RETRY_INTERVAL);
        if self.ledger.has_pending_incoming() {
            // Only the ledger writes these channels. A reservation wakes this
            // task when a slot opens; the next loop flushes it before reading wire.
            let pending: Vec<_> = self
                .ledger
                .leases
                .values()
                .filter(|lease| !lease.incoming_pending.is_empty())
                .filter_map(|lease| {
                    let (sender, _) = lease.incoming.as_ref()?;
                    Some(Box::pin(sender.clone().reserve_owned()))
                })
                .collect();
            let until_deadline = self
                .ledger
                .leases
                .values()
                .filter_map(|lease| lease.paused_at)
                .map(|started| INCOMING_PAUSE_TIMEOUT.saturating_sub(started.elapsed()))
                .min()
                .unwrap_or(INCOMING_PAUSE_TIMEOUT);
            return tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                _ = futures::future::select_all(pending) => Step::Retry,
                _ = xmtp_common::time::sleep(until_deadline) => Step::Retry,
                _ = xmtp_common::time::sleep(retry_after), if !self.outbox.is_empty() => Step::Retry,
            };
        }
        match self.conn.as_mut() {
            Some(wire) => tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                event = wire.next() => Step::Wire(event),
                _ = xmtp_common::time::sleep(retry_after), if !self.outbox.is_empty() => Step::Retry,
            },
            None if !self.ledger.leases.is_empty() && !self.suspended => tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                _ = tokio::time::sleep_until(self.reconnect_at) => Step::Reconnect,
            },
            None => Step::Cmd(self.cmds.recv().await),
        }
    }

    fn shutdown(&mut self) -> Flow {
        self.outbox.clear();
        self.close_wire_span("shutdown");
        if let Some(wire) = self.conn.take() {
            close_gracefully(wire);
        }
        Flow::Shutdown
    }

    fn open_wire_span(&mut self) {
        self.update_budget = update_budget();
        // The opener already sent the first Update on this connection.
        self.update_budget.take();
        self.wire_opened_at = Some(tokio::time::Instant::now());
        self.wire_span = Some(tracing::info_span!(
            parent: None,
            "bidi_wire",
            operation = "bidi.wire_session",
            reason = tracing::field::Empty,
        ));
    }

    fn close_wire_span(&mut self, reason: &'static str) {
        if let Some(span) = self.wire_span.take() {
            span.record("reason", reason);
        }
    }

    async fn lease(
        &mut self,
        subs: Vec<(Topic, B::Cursor)>,
        depth: usize,
        incoming_limits: Option<IncomingBatchLimits>,
        reply: oneshot::Sender<Result<TopicLease<B>, TransportError>>,
    ) -> Flow {
        let mut positions = HashMap::new();
        let mut subs_unique: Vec<(Topic, B::Cursor)> = Vec::new();
        for (topic, floor) in subs {
            if let Some(index) = positions.get(&topic).copied() {
                let (_, cursor): &mut (Topic, B::Cursor) = &mut subs_unique[index];
                *cursor = B::meet(*cursor, floor);
            } else {
                positions.insert(topic.clone(), subs_unique.len());
                subs_unique.push((topic, floor));
            }
        }
        let subs = subs_unique;
        let added = subs
            .iter()
            .filter(|(topic, _)| !self.ledger.by_topic.contains_key(topic))
            .count();
        if self.ledger.by_topic.len() + added
            > xmtp_configuration::BACKEND_DEFAULT_MAX_STREAM_TOPICS
        {
            let _ = reply.send(Err(TransportError::TooManyTopics));
            return Flow::Continue;
        }
        let cold = self.conn.is_none() && self.ledger.leases.is_empty() && !self.suspended;
        let topics = subs.iter().map(|(topic, _)| topic.clone()).collect();
        let (tx, events) = mpsc::channel(depth.max(1));
        let id = self.ledger.register(&subs, tx);
        let incoming = incoming_limits.map(|limits| {
            let (sender, receiver) = mpsc::channel(depth.max(1));
            if let Some(lease) = self.ledger.leases.get_mut(&id) {
                lease.incoming = Some((sender, limits));
            }
            receiver
        });
        let incoming_failure = self
            .ledger
            .leases
            .get(&id)
            .map(|lease| lease.incoming_failure.clone())
            .unwrap_or_default();
        if cold {
            self.outbox.updates.extend(self.ledger.prepare_adds(subs));
            let Some((_, initial)) = self.outbox.updates.pop_front() else {
                return Flow::Shutdown;
            };
            match self.open_preemptibly(initial).await {
                OpenOutcome::Opened(wire) => {
                    self.conn = Some(wire);
                    self.open_wire_span();
                    self.wire_opens += 1;
                }
                OpenOutcome::Failed(error) => {
                    self.ledger.deref(id);
                    self.ledger.reset_wire();
                    self.outbox.clear();
                    let _ = reply.send(Err(TransportError::Open(error)));
                    return Flow::Continue;
                }
                OpenOutcome::Suspended(ack) => {
                    self.ledger.reset_wire();
                    self.outbox.clear();
                    self.suspend_preempted(ack);
                }
                OpenOutcome::Shutdown => return Flow::Shutdown,
            }
        } else if self.conn.is_some() {
            self.outbox.updates.extend(self.ledger.join(id, subs));
            let dropped = self.ledger.recheck();
            let removes = self.drop_leases(dropped);
            self.retire(removes);
        }
        let Some(cmds) = self.lease_cmds.upgrade() else {
            return Flow::Continue;
        };
        let _ = reply.send(Ok(TopicLease {
            id,
            topics,
            events,
            incoming,
            incoming_pending: Vec::new().into_iter(),
            incoming_failure,
            cmds,
        }));
        Flow::Continue
    }

    fn deref(&mut self, id: LeaseId) -> Flow {
        let removes = self.drop_leases(vec![id]);
        self.retire(removes);
        self.settle_idle_waiters();
        self.settle_caught_up_waiters();
        Flow::Continue
    }

    fn drop_leases(&mut self, dropped: Vec<LeaseId>) -> Vec<Topic> {
        let mut removes = Vec::new();
        for lease in dropped {
            removes.extend(self.ledger.deref(lease));
        }
        // Cancel an unsent add only when none of its topics has a holder.
        let unused: HashSet<_> = self
            .ledger
            .pending_updates
            .iter()
            .filter_map(|(id, update)| {
                (!update.adds.is_empty()
                    && update
                        .adds
                        .iter()
                        .all(|(topic, _)| !self.ledger.by_topic.contains_key(topic)))
                .then_some(*id)
            })
            .collect();
        for id in self.outbox.purge(&unused) {
            if let Some(update) = self.ledger.pending_updates.remove(&id) {
                for (topic, _) in update.adds {
                    self.ledger.dirty_topics.insert(topic.clone());
                    // Keep a pending removal until Applied. A replacement must
                    // not add this topic before that old boundary is consumed.
                    if !self
                        .ledger
                        .registrations
                        .get(&topic)
                        .is_some_and(|registration| {
                            matches!(registration.state, RegistrationState::Removing)
                        })
                    {
                        self.ledger.registrations.remove(&topic);
                    }
                }
            }
        }
        removes
    }

    fn suspend(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            had_wire = self.conn.is_some(),
            "bidi transport: suspending — going off the network"
        );
        self.suspended = true;
        self.outbox.clear();
        self.ledger.reset_wire();
        self.close_wire_span("suspend");
        if let Some(wire) = self.conn.take() {
            close_gracefully(wire);
        }
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
        let _ = reply.send(());
        Flow::Continue
    }

    /// Resume from suspension immediately. During an outage, join the pending
    /// catch-up without replacing the scheduled reconnect backoff.
    async fn resume(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            "bidi transport: resuming — catch up, then done"
        );
        let was_suspended = self.suspended;
        self.suspended = false;
        self.resume_notify.push(reply);
        if self.conn.is_some() {
            self.settle_caught_up_waiters();
            return Flow::Continue;
        }
        if self.ledger.leases.is_empty() {
            self.settle_idle_waiters();
            return Flow::Continue;
        }
        if was_suspended {
            self.reconnect_delay = RECONNECT_INITIAL_DELAY;
            self.reconnect().await
        } else {
            Flow::Continue
        }
    }

    fn wire_event(&mut self, event: Event<B::GroupMessage, B::WelcomeMessage>) -> Flow {
        let has_incoming = self
            .ledger
            .leases
            .values()
            .any(|lease| lease.incoming.is_some());
        let incoming_dropped = if has_incoming {
            match &event {
                Event::Applied { id, targets } if !self.ledger.valid_targets(*id, targets) => {
                    return self.fail_incoming("Applied targets");
                }
                Event::GroupMessages { messages } => {
                    self.ledger.demux_incoming(messages, B::group_envelope)
                }
                Event::WelcomeMessages { messages } => {
                    self.ledger.demux_incoming(messages, B::welcome_envelope)
                }
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        };
        let incoming_dropped = match incoming_dropped {
            Ok(dropped) => dropped,
            Err(reason) => return self.fail_incoming(reason),
        };
        let mut dropped = match event {
            Event::Started { .. } => Vec::new(),
            Event::Applied { id, targets } => {
                let readds = self.ledger.applied(id, targets);
                self.outbox.updates.extend(self.ledger.prepare_adds(readds));
                Vec::new()
            }
            Event::GroupMessages { messages } => self.ledger.demux(
                messages,
                DeliveryKind::Group,
                B::group_topic,
                B::group_cursor,
                LeaseEvent::GroupMessages,
            ),
            Event::WelcomeMessages { messages } => self.ledger.demux(
                messages,
                DeliveryKind::Welcome,
                B::welcome_topic,
                B::welcome_cursor,
                LeaseEvent::WelcomeMessages,
            ),
        };
        dropped.extend(incoming_dropped);
        dropped.extend(self.ledger.recheck());
        let removes = self.drop_leases(dropped);
        self.retire(removes);
        self.settle_idle_waiters();
        self.settle_caught_up_waiters();
        Flow::Continue
    }

    fn fail_incoming(&mut self, reason: &'static str) -> Flow {
        let dropped = self
            .ledger
            .leases
            .iter()
            .filter_map(|(id, lease)| {
                lease.incoming.as_ref()?;
                *lease.incoming_failure.lock() = Some(TransportError::Protocol(reason));
                Some(*id)
            })
            .collect();
        let removes = self.drop_leases(dropped);
        self.retire(removes);
        self.settle_idle_waiters();
        Flow::Continue
    }

    fn arm_reconnect(&mut self) -> std::time::Duration {
        let delay = self.reconnect_delay + xmtp_common::time::rand_offset(self.reconnect_delay);
        self.reconnect_at = tokio::time::Instant::now() + delay;
        delay
    }

    fn wire_died(&mut self) -> Flow {
        self.outbox.clear();
        let failure = self.conn.as_ref().and_then(Connection::failure);
        drop(self.conn.take());
        let dropped = self
            .ledger
            .leases
            .iter()
            .filter_map(|(id, lease)| {
                let (sender, _) = lease.incoming.as_ref()?;
                let event = failure
                    .as_ref()
                    .map_or(Ok(vec![IncomingEvent::Disconnected]), |error| {
                        Err(TransportError::Wire(error.clone()))
                    });
                let full = match sender.try_send(event) {
                    Ok(()) => false,
                    Err(error) => {
                        *lease.incoming_failure.lock() = Some(match error.into_inner() {
                            Err(error) => error,
                            Ok(_) => TransportError::Backpressure,
                        });
                        true
                    }
                };
                (full || failure.as_ref().is_some_and(|error| !error.is_retryable())).then_some(*id)
            })
            .collect();
        self.drop_leases(dropped);
        self.ledger.reset_wire();
        self.close_wire_span("wire_end");
        if failure.is_some_and(|error| !error.is_retryable()) {
            return Flow::Shutdown;
        }
        let stable = self
            .wire_opened_at
            .take()
            .is_some_and(|opened| opened.elapsed() >= MIN_STABLE_UPTIME);
        self.reconnect_delay = if stable {
            RECONNECT_INITIAL_DELAY
        } else {
            (self.reconnect_delay * 2).min(RECONNECT_MAX_DELAY)
        };
        let retry_in = self.arm_reconnect();
        if !self.ledger.leases.is_empty() {
            tracing::warn!(
                leases = self.ledger.leases.len(),
                pending_updates = self.ledger.pending_updates.len(),
                retry_in_ms = retry_in.as_millis() as u64,
                "bidi transport: wire died; reopening from lease floors"
            );
        }
        self.settle_idle_waiters();
        Flow::Continue
    }

    #[tracing::instrument(skip_all, fields(operation = "bidi.reconnect"))]
    async fn reconnect(&mut self) -> Flow {
        match self.reopen().await {
            AfterReopen::Proceed => Flow::Continue,
            AfterReopen::Suspended(ack) => {
                self.suspend_preempted(ack);
                Flow::Continue
            }
            AfterReopen::Shutdown => Flow::Shutdown,
        }
    }

    fn suspend_preempted(&mut self, ack: oneshot::Sender<()>) {
        self.suspended = true;
        self.park_deferred_resumes();
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
        let _ = ack.send(());
    }

    async fn reopen(&mut self) -> AfterReopen {
        self.ledger.reset_wire();
        let adds = self.ledger.resume_adds();
        if adds.is_empty() {
            self.settle_idle_waiters();
            return AfterReopen::Proceed;
        }
        self.outbox.updates.extend(self.ledger.prepare_adds(adds));
        let Some((_, initial)) = self.outbox.updates.pop_front() else {
            return AfterReopen::Proceed;
        };
        match self.open_preemptibly(initial).await {
            OpenOutcome::Opened(wire) => {
                self.conn = Some(wire);
                self.open_wire_span();
                self.wire_opens += 1;
                AfterReopen::Proceed
            }
            OpenOutcome::Failed(error) => {
                self.outbox.clear();
                self.ledger.reset_wire();
                // implements: AUTH-025
                if error.is_locked_out() {
                    // The cool-down clears on its own, so the wire must wait for
                    // it. A shutdown here would lose every subscription for the
                    // life of the process because nothing restarts this task.
                    tracing::warn!("bidi reopen waits for the auth cool-down: {error}");
                    self.reconnect_delay = AUTH_LOCKOUT_COOLDOWN;
                    self.arm_reconnect();
                    self.park_deferred_resumes();
                    return AfterReopen::Proceed;
                }
                if !error.is_retryable() {
                    tracing::error!("bidi reconnect failed permanently: {error}");
                    let error = std::sync::Arc::new(super::bidi::ConnectionFailure::Wire(
                        xmtp_proto::api::NetworkError::new(error),
                    ));
                    for lease in self.ledger.leases.values() {
                        if lease.incoming.is_some() {
                            *lease.incoming_failure.lock() =
                                Some(TransportError::Wire(error.clone()));
                        }
                    }
                    return AfterReopen::Shutdown;
                }
                self.reconnect_delay = (self.reconnect_delay * 2).min(RECONNECT_MAX_DELAY);
                self.arm_reconnect();
                self.park_deferred_resumes();
                AfterReopen::Proceed
            }
            OpenOutcome::Suspended(ack) => {
                self.outbox.clear();
                self.ledger.reset_wire();
                AfterReopen::Suspended(ack)
            }
            OpenOutcome::Shutdown => AfterReopen::Shutdown,
        }
    }

    /// Keep receiving commands during a dial. Suspend cancels the dial at once.
    /// Other commands keep their order and run after the dial ends.
    async fn open_preemptibly(&mut self, mutate: B::Mutate) -> OpenOutcome<B> {
        let open = self.opener.open(mutate);
        tokio::pin!(open);
        loop {
            tokio::select! {
                result = &mut open => {
                    return match result {
                        Ok(wire) => OpenOutcome::Opened(wire),
                        Err(e) => OpenOutcome::Failed(e),
                    };
                }
                cmd = self.cmds.recv() => match cmd {
                    Some(Cmd::Suspend { reply }) => return OpenOutcome::Suspended(reply),
                    Some(cmd) => self.deferred.push_back(cmd),
                    None => return OpenOutcome::Shutdown,
                },
            }
        }
    }

    /// Merge concurrent resume calls into the same attempt and backoff.
    fn park_deferred_resumes(&mut self) {
        for cmd in std::mem::take(&mut self.deferred) {
            match cmd {
                Cmd::Resume { reply } => self.resume_notify.push(reply),
                other => self.deferred.push_back(other),
            }
        }
    }

    fn settle_idle_waiters(&mut self) {
        if !self.ledger.leases.is_empty() {
            return;
        }
        self.ledger.reset_wire();
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
    }

    fn settle_caught_up_waiters(&mut self) {
        if self.conn.is_none() || !self.ledger.caught_up() || !self.outbox.is_empty() {
            return;
        }
        for waiter in self.resume_notify.drain(..) {
            let _ = waiter.send(());
        }
    }

    fn retire(&mut self, removes: Vec<Topic>) {
        if self.ledger.leases.is_empty() {
            self.outbox.clear();
            self.ledger.reset_wire();
            self.close_wire_span("idle");
            if let Some(wire) = self.conn.take() {
                close_gracefully(wire);
            }
            return;
        }
        if self.conn.is_none() {
            return;
        }
        let mut to_remove = Vec::new();
        for topic in removes {
            if let Some(registration) = self.ledger.registrations.get_mut(&topic)
                && !matches!(registration.state, RegistrationState::Removing)
            {
                registration.state = RegistrationState::Removing;
                to_remove.push(topic);
            }
        }
        self.outbox
            .updates
            .extend(self.ledger.prepare_removes(to_remove));
    }

    /// Combine a same-kind prefix without changing queued frames or ack state.
    /// Duplicate topics and add/remove boundaries stay in separate updates.
    fn coalesced_prefix(&self) -> Option<(usize, PendingUpdate<B::Cursor>)> {
        if self.outbox.updates.len() < 2 {
            return None;
        }
        let (first_id, _) = self.outbox.updates.front()?;
        let first = self.ledger.pending_updates.get(first_id)?;
        let adds_only = !first.adds.is_empty() && first.removes.is_empty();
        let removes_only = first.adds.is_empty() && !first.removes.is_empty();
        if !adds_only && !removes_only {
            return None;
        }
        // The deployment caps adds and removes separately, so the
        // merged frame is bounded by the cap for the kind it carries. Merging a
        // removes-only prefix up to `add_cap` would build a frame the backend
        // rejects with INVALID_ARGUMENT wherever a deployment publishes a
        // smaller `max_update_removes`.
        let topic_cap = if adds_only {
            self.ledger.mutate.add_cap
        } else {
            self.ledger.mutate.remove_cap
        };
        let mut topics: HashSet<_> = first
            .adds
            .iter()
            .map(|(topic, _)| topic)
            .chain(first.removes.iter())
            .collect();
        let mut bytes: usize = topics.iter().map(|topic| topic_wire_cost(topic)).sum();
        let mut count = 1;
        for (id, _) in self.outbox.updates.iter().skip(1) {
            let Some(next) = self.ledger.pending_updates.get(id) else {
                break;
            };
            if (adds_only && (next.adds.is_empty() || !next.removes.is_empty()))
                || (removes_only && (next.removes.is_empty() || !next.adds.is_empty()))
            {
                break;
            }
            let next_topics: Vec<_> = next
                .adds
                .iter()
                .map(|(topic, _)| topic)
                .chain(next.removes.iter())
                .collect();
            let next_bytes: usize = next_topics.iter().map(|topic| topic_wire_cost(topic)).sum();
            if topics.len() + next_topics.len() > topic_cap
                || bytes + next_bytes > self.ledger.mutate.byte_cap
                || next_topics.iter().any(|topic| topics.contains(topic))
            {
                break;
            }
            topics.extend(next_topics);
            bytes += next_bytes;
            count += 1;
        }
        if count == 1 {
            return None;
        }
        let mut merged = PendingUpdate {
            adds: Vec::new(),
            removes: Vec::new(),
        };
        for (id, _) in self.outbox.updates.iter().take(count) {
            let update = self.ledger.pending_updates.get(id)?;
            merged.adds.extend(update.adds.iter().cloned());
            merged.removes.extend(update.removes.iter().cloned());
        }
        Some((count, merged))
    }

    /// Commit a coalesced acknowledgement ID only after the wire accepts it.
    /// Backpressure leaves the original queue and pending IDs intact.
    fn flush_outbox(&mut self) {
        let Some(wire) = self.conn.as_ref() else {
            return;
        };
        while let Some((id, _)) = self.outbox.updates.front() {
            if !self.update_budget.take() {
                return;
            }
            let id = *id;
            if let Some((count, merged)) = self.coalesced_prefix() {
                let update = B::build_mutate(merged.adds.clone(), merged.removes.clone(), id);
                if wire.try_mutate(update).is_err() {
                    self.update_budget.refund();
                    return;
                }
                for (old_id, _) in self.outbox.updates.drain(..count) {
                    self.ledger.pending_updates.remove(&old_id);
                }
                self.ledger.pending_updates.insert(id, merged);
                continue;
            }
            let Some((id, update)) = self.outbox.updates.pop_front() else {
                break;
            };
            match wire.try_mutate(update) {
                Ok(()) => {}
                Err(TryMutateError::Full(update)) | Err(TryMutateError::Closed(update)) => {
                    self.update_budget.refund();
                    self.outbox.updates.push_front((id, update));
                    return;
                }
            }
        }
    }
}

enum OpenOutcome<B: BidiBinding> {
    Opened(Connection<B>),
    Failed(OpenError),
    Suspended(oneshot::Sender<()>),
    Shutdown,
}

enum AfterReopen {
    Proceed,
    Suspended(oneshot::Sender<()>),
    Shutdown,
}

struct Outbox<M> {
    updates: std::collections::VecDeque<(u64, M)>,
}

impl<M> Default for Outbox<M> {
    fn default() -> Self {
        Self {
            updates: std::collections::VecDeque::new(),
        }
    }
}

impl<M> Outbox<M> {
    fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }
    fn clear(&mut self) {
        self.updates.clear();
    }

    fn purge(&mut self, ids: &HashSet<u64>) -> Vec<u64> {
        let mut purged = Vec::new();
        self.updates.retain(|(id, _)| {
            if ids.contains(id) {
                purged.push(*id);
                false
            } else {
                true
            }
        });
        purged
    }
}

/// Release the connection without blocking the ledger on command capacity.
/// Discard remaining events and abort the connection when the drain budget ends.
fn close_gracefully<B: TransportBinding>(wire: Connection<B>)
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    xmtp_common::spawn(None, async move {
        let mut wire = wire;
        let drained = xmtp_common::time::timeout(GRACEFUL_CLOSE_BUDGET, async {
            if wire.finish().await.is_err() {
                return; // actor already gone — nothing to drain
            }
            while wire.next().await.is_some() {}
        })
        .await;
        if drained.is_err() {
            tracing::debug!(
                budget_ms = GRACEFUL_CLOSE_BUDGET.as_millis() as u64,
                "bidi transport: graceful-close drain budget expired; dropping the wire"
            );
        }
    });
}

#[cfg(test)]
mod tests;
