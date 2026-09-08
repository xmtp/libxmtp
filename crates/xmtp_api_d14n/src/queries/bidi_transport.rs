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
//! After the add acknowledgement, all holders receive the new registration.
//! The delivery positions discard overlap without storing message buffers.
//!
//! A connection failure keeps leases alive. Reconnect uses the current topic
//! set and the meet of observed positions and lease floors. Suspend releases
//! the connection and keeps this state. Resume waits for all acknowledgements
//! and targets. A slow lease is closed so its consumer can recover from storage.
//!
//! The consumer owns durable progress. This module does not decode MLS data
//! or promise exactly-once callbacks across a process crash.

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};
use xmtp_common::{BoxDynFuture, MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::types::Topic;

use super::bidi::{BidiBinding, Connection, Event, TryMutateError};

/// Default event capacity for one lease. Slow consumers must recover from storage.
pub const DEFAULT_LEASE_DEPTH: usize = 64;
/// Retry a queued update when the connection is quiet and capacity may be free.
const OUTBOX_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// Maximum adds per backend update. The wire also has a separate topic cap.
pub(crate) const MAX_MUTATE_TOPICS: usize = xmtp_configuration::BACKEND_DEFAULT_MAX_UPDATE_ADDS;
/// Leave space for the request wrapper within the backend byte limit.
pub(crate) const MAX_MUTATE_BYTES: usize =
    xmtp_configuration::BACKEND_DEFAULT_MAX_REQUEST_BYTES - PER_ENTRY_OVERHEAD;
/// Conservative protobuf overhead per topic, including cursor and length fields.
const PER_ENTRY_OVERHEAD: usize = 64;

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
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl OpenError {
    pub fn new<E>(e: E) -> Self
    where
        E: std::error::Error + xmtp_common::RetryableError + Send + Sync + 'static,
    {
        Self {
            retryable: e.is_retryable(),
            source: Box::new(e),
        }
    }

    #[cfg(test)]
    pub fn retryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: true,
            source: e.into(),
        }
    }

    #[cfg(test)]
    pub fn unretryable(e: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> Self {
        Self {
            retryable: false,
            source: e.into(),
        }
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
    type Cursor: Copy + Send + std::fmt::Debug + From<u64> + 'static;

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
}

impl xmtp_common::RetryableError for TransportError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Open(e) => e.is_retryable(),
            Self::Closed | Self::Empty | Self::TooManyTopics => false,
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
        Self::spawn(
            opener,
            initially_suspended,
            MAX_MUTATE_TOPICS,
            MAX_MUTATE_BYTES,
        )
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
        Self::spawn(opener, initially_suspended, chunk_cap, chunk_bytes)
    }

    fn spawn<O, Fut>(
        opener: O,
        initially_suspended: bool,
        chunk_cap: usize,
        chunk_bytes: usize,
    ) -> Self
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
                chunk_cap,
                chunk_bytes,
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
    state: RegistrationState<C>,
}

enum RegistrationState<C> {
    Adding,
    Active,
    /// Old holders receive queued frames until the remove acknowledgement.
    Removing {
        pending_readd: Option<(C, HashSet<LeaseId>)>,
    },
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
    next_lease: u64,
    next_update: u64,
    chunk_cap: usize,
    chunk_bytes: usize,
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
            next_lease: 0,
            next_update: 0,
            chunk_cap: MAX_MUTATE_TOPICS,
            chunk_bytes: MAX_MUTATE_BYTES,
        }
    }
}

impl<B: TransportBinding> Ledger<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
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
    fn resume_cursor(&self, topic: &Topic) -> Option<B::Cursor> {
        let holders = self.by_topic.get(topic)?;
        holders
            .iter()
            .filter_map(|id| self.leases.get(id)?.floors.get(topic).copied())
            .chain(self.last_seen.get(topic).copied())
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
        let chunks = chunk_by_budget(adds, self.chunk_cap, self.chunk_bytes, |(topic, _)| {
            topic_wire_cost(topic)
        });
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
        chunk_by_budget(removes, self.chunk_cap, self.chunk_bytes, topic_wire_cost)
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
        for (topic, floor) in subs {
            self.dirty_topics.insert(topic.clone());
            let cursor = self.resume_cursor(&topic).unwrap_or(floor);
            let Some(registration) = self.registrations.get_mut(&topic) else {
                adds.push((topic, floor));
                continue;
            };
            match &mut registration.state {
                RegistrationState::Removing { pending_readd } => {
                    let (requested, waiting) =
                        pending_readd.get_or_insert_with(|| (cursor, HashSet::new()));
                    *requested = B::meet(*requested, cursor);
                    waiting.insert(id);
                }
                _ if !B::covers(&floor, &registration.delivered) => {
                    registration.state = RegistrationState::Removing {
                        pending_readd: Some((cursor, HashSet::from([id]))),
                    };
                    removes.push(topic);
                }
                _ => {
                    registration.holders.insert(id);
                }
            }
        }
        let mut updates = self.prepare_removes(removes);
        updates.extend(self.prepare_adds(adds));
        updates
    }

    /// Apply ordered acknowledgement boundaries and return topics to re-add.
    fn applied(&mut self, id: u64, targets: Vec<(Topic, u64)>) -> Vec<(Topic, B::Cursor)> {
        let Some(update) = self.pending_updates.remove(&id) else {
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
            }
        }
        readds
    }

    /// Check changed topics only. Send completion after their message batches.
    fn recheck(&mut self) -> Vec<LeaseId> {
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
                        || B::covers(&registration.delivered, &target_cursor)
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
                    if lease.events.try_send(LeaseEvent::CatchUpComplete).is_err() {
                        return Some(id);
                    }
                    lease.notified = true;
                }
                None
            })
            .collect()
    }

    /// Remove local interest immediately. The caller queues wire removals.
    fn deref(&mut self, id: LeaseId) -> Vec<Topic> {
        let Some(lease) = self.leases.remove(&id) else {
            return vec![];
        };
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
                if let RegistrationState::Removing {
                    pending_readd: Some((_, waiting)),
                } = &mut registration.state
                {
                    waiting.remove(&id);
                }
            }
        }
        removes
    }

    /// Route one frame by topic. A lease's position only moves forward.
    fn demux<M: Clone>(
        &mut self,
        messages: Vec<M>,
        kind: DeliveryKind,
        topic_of: impl Fn(&M) -> Option<Topic>,
        cursor_of: impl Fn(&M) -> Option<B::Cursor>,
        event: impl Fn(Vec<M>) -> LeaseEvent<B>,
    ) -> Vec<LeaseId> {
        let mut batches: HashMap<LeaseId, Vec<M>> = HashMap::new();
        for message in messages {
            let Some(topic) = topic_of(&message) else {
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
            let cursor = cursor_of(&message);
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
                if let Some(cursor) = cursor {
                    let Some(position) = lease.delivered.get_mut(&topic) else {
                        continue;
                    };
                    if B::covers(position, &cursor) {
                        continue;
                    }
                    B::advance(position, cursor);
                }
                batches.entry(*id).or_default().push(message.clone());
            }
        }
        batches
            .into_iter()
            .filter_map(|(id, batch)| {
                let lease = self.leases.get(&id)?;
                if lease.events.try_send(event(batch)).is_err() {
                    tracing::warn!(
                        lease = id.0,
                        ?kind,
                        "closing a lease whose delivery channel is full"
                    );
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    fn caught_up(&self) -> bool {
        self.pending_updates.is_empty() && self.leases.values().all(|lease| lease.unmet == 0)
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
    chunk_cap: usize,
    chunk_bytes: usize,
) where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    LedgerTask {
        opener,
        cmds,
        lease_cmds,
        ledger: Ledger {
            chunk_cap,
            chunk_bytes,
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
    deferred: std::collections::VecDeque<Cmd<B>>,
}

impl<B: TransportBinding> LedgerTask<B>
where
    B::GroupMessage: Clone,
    B::WelcomeMessage: Clone,
{
    async fn run(mut self) {
        loop {
            self.flush_outbox();
            let flow = match self.next_step().await {
                Step::Retry => Flow::Continue,
                Step::Cmd(None) => self.shutdown(),
                Step::Cmd(Some(Cmd::Lease { subs, depth, reply })) => {
                    self.lease(subs, depth, reply).await
                }
                Step::Cmd(Some(Cmd::Deref(id))) => self.deref(id),
                Step::Cmd(Some(Cmd::Suspend { reply })) => self.suspend(reply),
                Step::Cmd(Some(Cmd::Resume { reply })) => self.resume(reply).await,
                Step::Wire(Some(event)) => self.wire_event(event),
                Step::Wire(None) => self.wire_died(),
                Step::Reconnect => self.reconnect().await,
            };
            if let Flow::Shutdown = flow {
                return;
            }
        }
    }

    async fn next_step(&mut self) -> Step<B> {
        if let Some(cmd) = self.deferred.pop_front() {
            return Step::Cmd(Some(cmd));
        }
        match self.conn.as_mut() {
            Some(wire) => tokio::select! {
                cmd = self.cmds.recv() => Step::Cmd(cmd),
                event = wire.next() => Step::Wire(event),
                _ = tokio::time::sleep(OUTBOX_RETRY_INTERVAL), if !self.outbox.is_empty() => Step::Retry,
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
                    self.dirty_topics.insert(topic.clone());
                    self.ledger.registrations.remove(&topic);
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

    async fn resume(&mut self, reply: oneshot::Sender<()>) -> Flow {
        tracing::info!(
            leases = self.ledger.leases.len(),
            "bidi transport: resuming — catch up, then done"
        );
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
        self.reconnect_delay = RECONNECT_INITIAL_DELAY;
        self.reconnect().await
    }

    fn wire_event(&mut self, event: Event<B::GroupMessage, B::WelcomeMessage>) -> Flow {
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
        dropped.extend(self.ledger.recheck());
        let removes = self.drop_leases(dropped);
        self.retire(removes);
        self.settle_idle_waiters();
        self.settle_caught_up_waiters();
        Flow::Continue
    }

    fn arm_reconnect(&mut self) -> std::time::Duration {
        let delay = self.reconnect_delay + xmtp_common::time::rand_offset(self.reconnect_delay);
        self.reconnect_at = tokio::time::Instant::now() + delay;
        delay
    }

    fn wire_died(&mut self) -> Flow {
        self.outbox.clear();
        drop(self.conn.take());
        self.ledger.reset_wire();
        self.close_wire_span("wire_end");
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
                "bidi transport: wire died; reconnecting from last-seen positions"
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
                if !error.is_retryable() {
                    tracing::error!("bidi reconnect failed permanently: {error}");
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
                && !matches!(registration.state, RegistrationState::Removing { .. })
            {
                registration.state = RegistrationState::Removing {
                    pending_readd: None,
                };
                to_remove.push(topic);
            }
        }
        self.outbox
            .updates
            .extend(self.ledger.prepare_removes(to_remove));
    }

    fn flush_outbox(&mut self) {
        let Some(wire) = self.conn.as_ref() else {
            return;
        };
        while let Some((id, update)) = self.outbox.updates.pop_front() {
            match wire.try_mutate(update) {
                Ok(()) => {}
                Err(TryMutateError::Full(update)) | Err(TryMutateError::Closed(update)) => {
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
        let drained = tokio::time::timeout(GRACEFUL_CLOSE_BUDGET, async {
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
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::{BackendBinding, BidiConnection};
    use super::*;

    use futures::StreamExt;
    use futures::stream::BoxStream;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use xmtp_proto::api::ApiClientError;
    use xmtp_proto::api_client::XmtpMlsBidiStreams;
    use xmtp_proto::backend_v1::subscribe_request::Update as Mutate;
    use xmtp_proto::backend_v1::{
        self, ServerEnvelope, SubscribeRequest, SubscribeResponse, subscribe_request,
        subscribe_response,
    };
    type GroupMessage = ServerEnvelope;
    type WelcomeMessage = ServerEnvelope;
    use xmtp_proto::types::TopicKind;

    const WAIT: Duration = Duration::from_secs(5);

    struct MockApi {
        inbound: Mutex<
            Option<tokio::sync::mpsc::UnboundedReceiver<Result<SubscribeResponse, ApiClientError>>>,
        >,
        captured: tokio::sync::mpsc::UnboundedSender<SubscribeRequest>,
    }

    struct MockServer {
        to_client: tokio::sync::mpsc::UnboundedSender<Result<SubscribeResponse, ApiClientError>>,
        from_client: tokio::sync::mpsc::UnboundedReceiver<SubscribeRequest>,
        updates: HashMap<u64, Mutate>,
        active: Mutex<HashSet<Vec<u8>>>,
        last_ack: Mutex<u64>,
    }

    fn mock_pair() -> (MockApi, MockServer) {
        let (to_client, inbound) = tokio::sync::mpsc::unbounded_channel();
        let (captured, from_client) = tokio::sync::mpsc::unbounded_channel();
        to_client
            .send(Ok(SubscribeResponse {
                response: Some(started(30_000)),
            }))
            .unwrap();
        (
            MockApi {
                inbound: Mutex::new(Some(inbound)),
                captured,
            },
            MockServer {
                to_client,
                from_client,
                updates: HashMap::new(),
                active: Mutex::default(),
                last_ack: Mutex::new(0),
            },
        )
    }

    #[xmtp_common::async_trait]
    impl XmtpMlsBidiStreams for MockApi {
        type SubscribeStream = BoxStream<'static, Result<SubscribeResponse, ApiClientError>>;
        type Error = ApiClientError;

        fn host(&self) -> &str {
            "mock://bidi"
        }

        async fn subscribe_bidi(
            &self,
            requests: BoxStream<'static, SubscribeRequest>,
        ) -> Result<Self::SubscribeStream, Self::Error> {
            let captured = self.captured.clone();
            xmtp_common::spawn(None, async move {
                let mut requests = requests;
                while let Some(frame) = requests.next().await {
                    let _ = captured.send(frame);
                }
            });
            let mut inbound = self
                .inbound
                .lock()
                .unwrap()
                .take()
                .expect("subscribe_bidi called twice on one mock session");
            Ok(Box::pin(futures::stream::poll_fn(move |cx| {
                inbound.poll_recv(cx)
            })))
        }
    }

    impl MockServer {
        fn send(&self, response: subscribe_response::Response) {
            self.to_client
                .send(Ok(SubscribeResponse {
                    response: Some(response),
                }))
                .unwrap();
        }

        fn ack(&self, id: u64, targets: Vec<(Topic, u64)>) {
            let update = self.updates.get(&id).expect("update was received");
            let mut last_ack = self.last_ack.lock().unwrap();
            assert!(id > *last_ack, "acknowledgements must follow update order");
            *last_ack = id;
            let mut active = self.active.lock().unwrap();
            for topic in &update.removes {
                active.remove(&topic.topic);
            }
            let targets: HashMap<_, _> = targets
                .into_iter()
                .map(|(topic, target)| (topic.cloned_vec(), target))
                .collect();
            let added_targets = update
                .adds
                .iter()
                .filter_map(|add| {
                    let topic = add.topic.as_ref().unwrap();
                    active
                        .insert(topic.topic.clone())
                        .then(|| backend_v1::CatchupTarget {
                            topic: Some(topic.clone()),
                            through_sequence_id: targets.get(&topic.topic).copied().unwrap_or(0),
                        })
                })
                .collect();
            self.send(subscribe_response::Response::Applied(
                subscribe_response::Applied { id, added_targets },
            ));
        }

        fn ack_empty(&self, id: u64) {
            self.ack(id, vec![]);
        }

        async fn next_mutate(&mut self) -> Mutate {
            let frame = tokio::time::timeout(WAIT, self.from_client.recv())
                .await
                .expect("timed out waiting for a client frame")
                .expect("client closed the request stream");
            match frame.request.expect("client sent empty request") {
                subscribe_request::Request::Update(mutate) => {
                    self.updates.insert(mutate.id, mutate.clone());
                    mutate
                }
                other => panic!("expected a Mutate, got {other:?}"),
            }
        }

        async fn next_ping(&mut self) -> u64 {
            let frame = tokio::time::timeout(WAIT, self.from_client.recv())
                .await
                .expect("timed out waiting for a client frame")
                .expect("client closed the request stream");
            match frame.request.expect("client sent empty request") {
                subscribe_request::Request::Ping(ping) => ping.nonce,
                other => panic!("expected a Ping, got {other:?}"),
            }
        }

        async fn request_stream_ended(&mut self) {
            loop {
                match tokio::time::timeout(WAIT, self.from_client.recv())
                    .await
                    .expect("timed out waiting for the request half-close")
                {
                    Some(_) => continue, // drain trailing frames (e.g. a remove wave)
                    None => return,
                }
            }
        }
    }

    type Servers = Arc<Mutex<Vec<MockServer>>>;

    fn transport() -> (BidiTransport<BackendBinding>, Servers) {
        transport_born(false)
    }

    fn transport_born(suspended: bool) -> (BidiTransport<BackendBinding>, Servers) {
        let servers: Servers = Arc::default();
        let sink = servers.clone();
        let transport = BidiTransport::new(
            move |initial| {
                let (api, server) = mock_pair();
                sink.lock().unwrap().push(server);
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            suspended,
        );
        (transport, servers)
    }

    fn transport_capped(
        max_topics: usize,
        max_bytes: usize,
    ) -> (BidiTransport<BackendBinding>, Servers) {
        let servers: Servers = Arc::default();
        let sink = servers.clone();
        let transport = BidiTransport::new_with_chunk_limits(
            move |initial| {
                let (api, server) = mock_pair();
                sink.lock().unwrap().push(server);
                async move {
                    BidiConnection::open(&api, initial)
                        .await
                        .map_err(OpenError::new)
                }
            },
            false,
            max_topics,
            max_bytes,
        );
        (transport, servers)
    }

    fn take_server(servers: &Servers) -> MockServer {
        servers.lock().unwrap().remove(0)
    }

    #[derive(Clone, Default)]
    struct WireCloseLog {
        reasons: Arc<Mutex<Vec<String>>>,
    }

    struct ReasonVisitor<'a>(&'a Mutex<Vec<String>>);

    impl tracing::field::Visit for ReasonVisitor<'_> {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "reason" {
                self.0.lock().unwrap().push(value.to_owned());
            }
        }

        fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WireCloseLog {
        fn on_record(
            &self,
            _: &tracing::Id,
            values: &tracing::span::Record<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            values.record(&mut ReasonVisitor(&self.reasons));
        }
    }

    async fn close_reasons(log: &WireCloseLog, n: usize) -> Vec<String> {
        let poll = async {
            loop {
                {
                    let reasons = log.reasons.lock().unwrap();
                    if reasons.len() >= n {
                        return reasons.clone();
                    }
                }
                xmtp_common::time::sleep(Duration::from_millis(5)).await;
            }
        };
        match tokio::time::timeout(WAIT, poll).await {
            Ok(reasons) => reasons,
            Err(_) => panic!(
                "expected {n} closed wire spans, saw {:?}",
                log.reasons.lock().unwrap()
            ),
        }
    }

    fn group_topic(id: &[u8]) -> Topic {
        Topic::new_group_message(id)
    }

    fn welcome_topic(installation_key: &[u8]) -> Topic {
        TopicKind::WelcomeMessagesV1.create(installation_key)
    }

    fn group_msg(id: u64, group_id: &[u8]) -> GroupMessage {
        envelope(
            id,
            group_topic(group_id),
            backend_v1::client_envelope::Payload::GroupMessage(backend_v1::GroupMessage {
                data: vec![0xda; 4],
                ..Default::default()
            }),
        )
    }

    fn welcome_msg(id: u64, installation_key: &[u8]) -> WelcomeMessage {
        envelope(
            id,
            welcome_topic(installation_key),
            backend_v1::client_envelope::Payload::WelcomeMessage(backend_v1::WelcomeMessage {
                version: Some(backend_v1::welcome_message::Version::V1(
                    backend_v1::welcome_message::V1 {
                        installation_key: installation_key.to_vec(),
                        data: vec![0xef; 4],
                        ..Default::default()
                    },
                )),
            }),
        )
    }

    fn envelope(
        sequence_id: u64,
        topic: Topic,
        payload: backend_v1::client_envelope::Payload,
    ) -> ServerEnvelope {
        ServerEnvelope {
            meta: Some(backend_v1::EnvelopeMeta {
                topic: Some(backend_v1::Topic {
                    topic: topic.cloned_vec(),
                }),
                cursor: Some(backend_v1::Cursor { sequence_id }),
                ..Default::default()
            }),
            envelope: Some(backend_v1::ClientEnvelope {
                payload: Some(payload),
            }),
        }
    }

    fn messages(
        group: Vec<GroupMessage>,
        welcome: Vec<WelcomeMessage>,
    ) -> subscribe_response::Response {
        subscribe_response::Response::Messages(subscribe_response::Messages {
            envelopes: group.into_iter().chain(welcome).collect(),
        })
    }

    fn started(keepalive: u32) -> subscribe_response::Response {
        subscribe_response::Response::Started(subscribe_response::Started {
            keepalive_interval_ms: keepalive,
        })
    }

    async fn recv(lease: &mut TopicLease<BackendBinding>) -> Option<LeaseEvent<BackendBinding>> {
        tokio::time::timeout(WAIT, lease.next())
            .await
            .expect("timed out waiting for a lease event")
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn first_lease_opens_the_wire_with_its_cursored_adds() {
        let (transport, servers) = transport();
        assert!(servers.lock().unwrap().is_empty(), "wire must open lazily");

        let group = group_topic(b"g1");
        let welcome = welcome_topic(b"i1");
        let _lease = transport
            .lease(
                vec![(group.clone(), 5), (welcome.clone(), 0)],
                DEFAULT_LEASE_DEPTH,
            )
            .await?;

        let mut server = take_server(&servers);
        let mutate = server.next_mutate().await;
        assert_eq!(mutate.adds.len(), 2);
        assert_eq!(
            mutate.adds[0].topic.as_ref().unwrap().topic,
            group.cloned_vec()
        );
        assert_eq!(mutate.adds[0].cursor.as_ref().unwrap().sequence_id, 5);
        assert_eq!(
            mutate.adds[1].topic.as_ref().unwrap().topic,
            welcome.cloned_vec()
        );
        assert_eq!(mutate.adds[1].cursor.as_ref().unwrap().sequence_id, 0);
        assert!(mutate.removes.is_empty());
        assert_ne!(mutate.id, 0, "update IDs must be nonzero");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_lease_over_the_cap_splits_into_bounded_frames() {
        const CHUNK_CAP: usize = 2;
        let (transport, servers) = transport_capped(CHUNK_CAP, MAX_MUTATE_BYTES);
        let topics: Vec<(Topic, u64)> = (0..(CHUNK_CAP as u32 + 1))
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        assert_eq!(first.adds.len(), CHUNK_CAP, "first frame is capped");
        assert_eq!(second.adds.len(), 1, "the overflow rides a second frame");
        assert_ne!(first.id, second.id, "each chunk has its own update ID");

        server.ack_empty(first.id);
        let quiet = tokio::time::timeout(Duration::from_millis(200), lease.next()).await;
        assert!(
            quiet.is_err(),
            "a chunked lease must not report caught-up until its last chunk completes"
        );

        server.ack_empty(second.id);
        assert!(
            matches!(recv(&mut lease).await, Some(LeaseEvent::CatchUpComplete)),
            "the last chunk's completion catches the lease up"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_reconnect_resume_over_the_cap_splits_into_bounded_frames() {
        const CHUNK_CAP: usize = 2;
        let (transport, servers) = transport_capped(CHUNK_CAP, MAX_MUTATE_BYTES);
        let topics: Vec<(Topic, u64)> = (0..(CHUNK_CAP as u32 + 1))
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        server.ack_empty(first.id);
        server.ack_empty(second.id);
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server);
        let mut reconnect = wait_for_server(&servers).await;
        let resume_a = reconnect.next_mutate().await;
        let resume_b = reconnect.next_mutate().await;
        assert_eq!(resume_a.adds.len(), CHUNK_CAP, "resume frame is capped");
        assert_eq!(
            resume_b.adds.len(),
            1,
            "the overflow rides a second resume frame"
        );
        assert_ne!(
            resume_a.id, resume_b.id,
            "each resume chunk has its own update ID"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn an_over_cap_lease_still_catching_up_survives_a_wire_death() {
        let (transport, servers) = transport_capped(2, usize::MAX);
        let topics: Vec<(Topic, u64)> = (0..3u32)
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        assert_eq!(first.adds.len() + second.adds.len(), 3);
        server.ack_empty(first.id);
        drop(server);

        let mut reconnect = wait_for_server(&servers).await;
        let resume_a = reconnect.next_mutate().await;
        let resume_b = reconnect.next_mutate().await;
        assert_eq!(
            resume_a.adds.len() + resume_b.adds.len(),
            3,
            "the whole lease is re-issued after the death, not just the unfinished chunk"
        );
        for id in [resume_a.id, resume_b.id] {
            assert!(
                id != first.id && id != second.id,
                "reopened chunks carry fresh update IDs"
            );
        }

        reconnect.ack_empty(resume_a.id);
        let quiet = tokio::time::timeout(Duration::from_millis(200), lease.next()).await;
        assert!(
            quiet.is_err(),
            "a re-chunked lease stays catching-up until its LAST reopened chunk completes"
        );
        reconnect.ack_empty(resume_b.id);
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn an_over_cap_lease_survives_suspend_and_resume() {
        let (transport, servers) = transport_capped(2, usize::MAX);
        let topics: Vec<(Topic, u64)> = (0..3u32)
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let mut lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        let second = server.next_mutate().await;
        server.ack_empty(first.id);
        server.ack_empty(second.id);
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        drop(server);
        let mut resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });

        let mut reconnect = wait_for_server(&servers).await;
        let resume_a = reconnect.next_mutate().await;
        let resume_b = reconnect.next_mutate().await;
        assert_eq!(
            resume_a.adds.len() + resume_b.adds.len(),
            3,
            "the whole lease is re-subscribed on resume, chunked into bounded frames"
        );
        reconnect.ack_empty(resume_a.id);
        let pending = tokio::time::timeout(Duration::from_millis(200), &mut resume).await;
        assert!(
            pending.is_err(),
            "resume() must not resolve until every resumed chunk is caught up"
        );
        reconnect.ack_empty(resume_b.id);
        tokio::time::timeout(WAIT, resume).await?.unwrap()?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_lease_over_the_byte_budget_splits_into_bounded_frames() {
        let per_topic = topic_wire_cost(&group_topic(&0u32.to_le_bytes()));
        let (transport, servers) = transport_capped(usize::MAX, 2 * per_topic);
        let topics: Vec<(Topic, u64)> = (0..5u32)
            .map(|i| (group_topic(&i.to_le_bytes()), 0))
            .collect();
        let _lease = transport.lease(topics, DEFAULT_LEASE_DEPTH).await?;

        let mut server = take_server(&servers);
        let mut subscribed = 0;
        while subscribed < 5 {
            let mutate = server.next_mutate().await;
            assert!(
                !mutate.adds.is_empty() && mutate.adds.len() <= 2,
                "a byte-budgeted frame holds 1–2 adds, got {}",
                mutate.adds.len()
            );
            subscribed += mutate.adds.len();
        }
        assert_eq!(subscribed, 5, "every topic is subscribed across the frames");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_mass_unsubscribe_chunks_the_removes_update() {
        let (transport, servers) = transport_capped(2, usize::MAX);
        let keeper = transport
            .lease(vec![(group_topic(b"keep"), 0)], DEFAULT_LEASE_DEPTH)
            .await?;
        let mut server = take_server(&servers);
        let k = server.next_mutate().await;
        server.ack_empty(k.id);

        let big = transport
            .lease(
                (0..3u32)
                    .map(|i| (group_topic(&i.to_le_bytes()), 0))
                    .collect(),
                DEFAULT_LEASE_DEPTH,
            )
            .await?;
        let a = server.next_mutate().await;
        let b = server.next_mutate().await;
        server.ack_empty(a.id);
        server.ack_empty(b.id);

        drop(big);
        let mut removed = 0;
        while removed < 3 {
            let mutate = server.next_mutate().await;
            if mutate.removes.is_empty() {
                continue; // ignore a trailing add/echo frame
            }
            assert!(
                mutate.removes.len() <= 2,
                "each removes frame respects the cap, got {}",
                mutate.removes.len()
            );
            removed += mutate.removes.len();
        }
        assert_eq!(removed, 3, "every dropped topic is unsubscribed");
        drop(keeper);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn second_lease_is_a_cursored_re_add_on_the_open_wire() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let _first = transport.lease(vec![(shared.clone(), 40)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        let fresh = group_topic(b"g2");
        let _second = transport
            .lease(vec![(shared.clone(), 7), (fresh.clone(), 0)], 8)
            .await?;
        assert!(
            servers.lock().unwrap().is_empty(),
            "second lease must use the open wire"
        );
        let remove = server.next_mutate().await;
        assert!(remove.adds.is_empty());
        assert_eq!(
            remove.removes,
            vec![backend_v1::Topic {
                topic: shared.cloned_vec()
            }]
        );
        let fresh_add = server.next_mutate().await;
        assert_eq!(fresh_add.adds.len(), 1);
        assert_eq!(
            fresh_add.adds[0].topic.as_ref().unwrap().topic,
            fresh.cloned_vec()
        );
        assert_eq!(fresh_add.adds[0].cursor.as_ref().unwrap().sequence_id, 0);
        server.ack_empty(remove.id);
        server.ack_empty(fresh_add.id);
        let second = server.next_mutate().await;
        assert_eq!(second.adds.len(), 1);
        assert_eq!(
            second.adds[0].topic.as_ref().unwrap().topic,
            shared.cloned_vec()
        );
        assert_eq!(second.adds[0].cursor.as_ref().unwrap().sequence_id, 7);
        assert!(first.id < remove.id && remove.id < fresh_add.id && fresh_add.id < second.id);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn deliveries_demux_by_topic() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut inst = transport.lease(vec![(welcome_topic(b"i1"), 0)], 8).await?;
        let mut server = take_server(&servers);

        for _ in 0..3 {
            let wave = server.next_mutate().await;
            server.ack_empty(wave.id);
        }
        for lease in [&mut alpha, &mut beta, &mut inst] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
        }

        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g2"),
            group_msg(3, b"g1"),
        );
        let unroutable = GroupMessage::default();
        let w = welcome_msg(9, b"i1");
        server.send(messages(
            vec![m1.clone(), unroutable, m2.clone(), m3.clone()],
            vec![w.clone()],
        ));

        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m1, m3]),
            _ => panic!("alpha expected its two group messages"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m2]),
            _ => panic!("beta expected its one group message"),
        }
        match recv(&mut inst).await {
            Some(LeaseEvent::WelcomeMessages(got)) => assert_eq!(got, vec![w]),
            _ => panic!("inst expected its welcome"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn rotation_ordered_replay_delivers_every_topic() {
        let (transport, servers) = transport();
        let mut alpha = transport
            .lease(vec![(group_topic(b"g1"), 0), (group_topic(b"g2"), 0)], 8)
            .await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack(
            first.id,
            vec![(group_topic(b"g1"), 6), (group_topic(b"g2"), 4)],
        );

        server.send(messages(
            vec![group_msg(5, b"g1"), group_msg(6, b"g1")],
            vec![],
        ));
        server.send(messages(
            vec![group_msg(3, b"g2"), group_msg(4, b"g2")],
            vec![],
        ));

        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(5, b"g1"), group_msg(6, b"g1")])
            }
            _ => panic!("expected the first rotation turn"),
        }
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(3, b"g2"), group_msg(4, b"g2")],
                "a later turn opening below an earlier turn's ids must still deliver"
            ),
            _ => panic!("expected the second rotation turn"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn covered_live_frame_is_dropped() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack(first.id, vec![(group_topic(b"g1"), 2)]);
        server.send(messages(
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got.len(), 2),
            _ => panic!("alpha expected its replay"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        server.send(messages(
            vec![group_msg(2, b"g1"), group_msg(3, b"g1")],
            vec![],
        ));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(
                got,
                vec![group_msg(3, b"g1")],
                "a covered live frame must be dropped, not re-delivered"
            ),
            _ => panic!("alpha expected only the fresh live message"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn shared_topic_fans_out_to_every_lease() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let mut alpha = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);

        for _ in 0..1 {
            let wave = server.next_mutate().await;
            server.ack_empty(wave.id);
        }
        let m = group_msg(1, b"g1");
        server.send(messages(vec![m.clone()], vec![]));

        for lease in [&mut alpha, &mut beta] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
            match recv(lease).await {
                Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![m.clone()]),
                _ => panic!("both leases must receive the shared delivery"),
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn markers_route_to_their_owners() {
        let (transport, servers) = transport();
        let (ga, gb) = (group_topic(b"g1"), group_topic(b"g2"));
        let mut alpha = transport.lease(vec![(ga.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(gb.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let update_a = server.next_mutate().await;
        let update_b = server.next_mutate().await;
        server.ack(update_a.id, vec![(ga, 1)]);
        server.ack(update_b.id, vec![(gb, 2)]);
        server.send(messages(vec![group_msg(2, b"g2")], vec![]));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(2, b"g2")]),
            _ => panic!("beta expected only its own topic"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), alpha.next())
                .await
                .is_err(),
            "beta's completion must not complete alpha"
        );
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(1, b"g1")]),
            _ => panic!("alpha expected only its own topic"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), beta.next())
                .await
                .is_err(),
            "alpha's completion must not complete beta again"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_is_refcounted_and_last_lease_closes_the_wire() {
        let (transport, servers) = transport();
        let (shared, exclusive) = (group_topic(b"g1"), group_topic(b"g2"));
        let alpha = transport
            .lease(vec![(shared.clone(), 0), (exclusive.clone(), 0)], 8)
            .await?;
        let beta = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let initial = server.next_mutate().await;
        server.ack_empty(initial.id);

        drop(alpha);
        let removal = server.next_mutate().await;
        assert!(removal.adds.is_empty());
        assert_eq!(
            removal.removes,
            vec![backend_v1::Topic {
                topic: exclusive.cloned_vec()
            }],
            "the shared topic still has a holder and must stay"
        );

        drop(beta);
        server.request_stream_ended().await;

        tokio::time::sleep(Duration::from_millis(50)).await; // let a would-be abort land
        server.send(started(30_000));

        let _again = transport.lease(vec![(shared, 3)], 8).await?;
        let mut second = take_server(&servers);
        assert_eq!(second.next_mutate().await.adds.len(), 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn slow_lease_is_dropped_without_blocking_siblings() {
        let (transport, servers) = transport();
        let shared = group_topic(b"g1");
        let mut slow = transport.lease(vec![(shared.clone(), 0)], 1).await?;
        let mut fast = transport.lease(vec![(shared.clone(), 0)], 8).await?;
        let mut server = take_server(&servers);
        let initial = server.next_mutate().await;
        server.ack_empty(initial.id);
        for lease in [&mut slow, &mut fast] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
        }

        let (m1, m2, m3) = (
            group_msg(1, b"g1"),
            group_msg(2, b"g1"),
            group_msg(3, b"g1"),
        );
        server.send(messages(vec![m1.clone()], vec![]));
        server.send(messages(vec![m2.clone()], vec![]));
        server.send(messages(vec![m3.clone()], vec![]));

        for expected in [&m1, &m2, &m3] {
            match recv(&mut fast).await {
                Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![expected.clone()]),
                _ => panic!("fast lease must receive every delivery"),
            }
        }
        assert!(matches!(
            recv(&mut slow).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(
            recv(&mut slow).await.is_none(),
            "wedged lease must be closed"
        );
    }

    async fn wait_for_server(servers: &Servers) -> MockServer {
        tokio::time::timeout(WAIT, async {
            loop {
                let next = {
                    let mut parked = servers.lock().unwrap();
                    (!parked.is_empty()).then(|| parked.remove(0))
                };
                if let Some(server) = next {
                    return server;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out waiting for a reconnect open")
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn wire_death_reconnects_from_last_seen_positions() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack(first.id, vec![(group_topic(b"g1"), 2)]);
        server.send(messages(
            vec![group_msg(1, b"g1"), group_msg(2, b"g1")],
            vec![],
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies mid-stream

        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1, "one resume update");
        assert_eq!(
            resume.adds[0].topic.as_ref().unwrap().topic,
            group_topic(b"g1").cloned_vec()
        );
        assert_eq!(
            resume.adds[0].cursor.as_ref().unwrap().sequence_id,
            2,
            "resume at the meet of last-seen and the lease floor"
        );

        second.ack(resume.id, vec![(group_topic(b"g1"), 3)]);
        second.send(messages(
            vec![group_msg(2, b"g1"), group_msg(3, b"g1")],
            vec![],
        ));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(3, b"g1")])
            }
            _ => panic!("the lease must survive the flap and see only new messages"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn command_traffic_does_not_postpone_the_reconnect() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies

        let churn = {
            let transport = transport.clone();
            tokio::spawn(async move {
                loop {
                    let lease = transport.lease(vec![(group_topic(b"g9"), 0)], 4).await;
                    drop(lease);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
            })
        };
        let _second = wait_for_server(&servers).await;
        churn.abort();
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn half_open_wire_is_reaped_and_reconnected() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack(first.id, vec![(group_topic(b"g1"), 1)]);
        server.send(started(150));
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        server.next_ping().await;

        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1);
        assert_eq!(resume.adds[0].cursor.as_ref().unwrap().sequence_id, 0);

        second.ack(resume.id, vec![(group_topic(b"g1"), 2)]);
        second.send(messages(vec![group_msg(2, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(2, b"g1")])
            }
            _ => panic!("the lease must survive a half-open wire invisibly"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn suspend_half_closes_and_resume_completes_at_catch_up() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack(first.id, vec![(group_topic(b"g1"), 1)]);
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        assert_eq!(resume.adds.len(), 1);
        assert_eq!(
            resume.adds[0].cursor.as_ref().unwrap().sequence_id,
            1,
            "resume at the meet of the kept position and floor"
        );
        second.ack(resume.id, vec![(group_topic(b"g1"), 2)]);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !resumed.is_finished(),
            "Applied alone does not meet the target"
        );

        second.send(messages(vec![group_msg(2, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(2, b"g1")])
            }
            _ => panic!("the lease must survive suspend/resume invisibly"),
        }
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
    }

    #[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
    async fn wire_session_span_closes_with_a_reason_on_every_release_path() {
        use tracing_subscriber::layer::SubscriberExt;

        let closes = WireCloseLog::default();
        let _guard =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(closes.clone()));

        let (suspended, servers_a) = transport();
        let alpha = suspended.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut first = take_server(&servers_a);
        first.next_mutate().await;
        suspended.suspend().await?;
        assert_eq!(close_reasons(&closes, 1).await[0], "suspend");
        drop((alpha, first, suspended));

        let (dying, servers_b) = transport();
        let beta = dying.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut second = take_server(&servers_b);
        second.next_mutate().await;
        drop(second);
        assert_eq!(close_reasons(&closes, 2).await[1], "wire_end");
        drop((beta, dying));

        let (retiring, servers_c) = transport();
        let gamma = retiring.lease(vec![(group_topic(b"g3"), 0)], 8).await?;
        let mut third = take_server(&servers_c);
        third.next_mutate().await;
        drop(gamma);
        assert_eq!(close_reasons(&closes, 3).await[2], "idle");
        drop((third, retiring));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_resumes_join_one_catch_up_update() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        let resume_a = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let resume_b = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });

        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        second.ack_empty(resume.id);
        tokio::time::timeout(WAIT, resume_a).await?.unwrap()?;
        tokio::time::timeout(WAIT, resume_b).await?.unwrap()?;
        assert!(
            servers.lock().unwrap().is_empty(),
            "the second resume must join the pending update"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn suspended_transport_stays_off_the_network() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        transport.suspend().await?;
        server.request_stream_ended().await;
        let mut beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            servers.lock().unwrap().is_empty(),
            "suspended must mean off the network"
        );
        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let resume = second.next_mutate().await;
        let got: HashSet<_> = resume
            .adds
            .iter()
            .map(|add| add.topic.as_ref().unwrap().topic.clone())
            .collect();
        assert_eq!(
            got,
            HashSet::from([
                group_topic(b"g1").cloned_vec(),
                group_topic(b"g2").cloned_vec()
            ]),
            "one update covers all leased topics"
        );
        assert_eq!(resume.adds.len(), 2);
        assert!(
            servers.lock().unwrap().is_empty(),
            "one wire serves both leases"
        );
        second.ack(resume.id, vec![(group_topic(b"g2"), 1)]);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !resumed.is_finished(),
            "resume must wait for the parked lease's target too"
        );
        second.send(messages(vec![group_msg(1, b"g2")], vec![]));
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(1, b"g2")]),
            _ => panic!("beta must receive its catch-up"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_born_suspended_transport_parks_the_first_lease() {
        let (transport, servers) = transport_born(true);

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(
            servers.lock().unwrap().is_empty(),
            "a born-suspended transport must keep its first lease parked"
        );

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let wave = server.next_mutate().await;
        assert_eq!(
            wave.adds
                .iter()
                .map(|add| &add.topic.as_ref().unwrap().topic)
                .collect::<Vec<_>>(),
            vec![&group_topic(b"g1").cloned_vec()],
            "the parked lease's adds open the wire"
        );
        server.ack_empty(wave.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;

        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn dropping_the_last_lease_settles_resume_waiters() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        transport.suspend().await?;
        server.request_stream_ended().await;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut second = wait_for_server(&servers).await;
        let _resume = second.next_mutate().await;
        drop(alpha);
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn suspend_preempts_a_stuck_dial() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<BackendBinding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(
                move |initial| {
                    let n = dials.fetch_add(1, Ordering::SeqCst);
                    let sink = sink.clone();
                    async move {
                        if n == 0 {
                            std::future::pending::<()>().await;
                            unreachable!("the hung dial never resolves");
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
        };

        let leased = tokio::spawn({
            let transport = transport.clone();
            async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
        });
        while dials.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        tokio::time::timeout(WAIT, transport.suspend()).await??;
        let mut alpha = tokio::time::timeout(WAIT, leased).await?.unwrap()?;

        let resumed = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.ack_empty(resume.id);
        tokio::time::timeout(WAIT, resumed).await?.unwrap()?;
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_preempting_suspend_outranks_a_deferred_resume() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<BackendBinding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(
                move |initial| {
                    let n = dials.fetch_add(1, Ordering::SeqCst);
                    let sink = sink.clone();
                    async move {
                        if n == 0 {
                            std::future::pending::<()>().await;
                            unreachable!("the hung dial never resolves");
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
        };

        let leased = tokio::spawn({
            let transport = transport.clone();
            async move { transport.lease(vec![(group_topic(b"g1"), 0)], 8).await }
        });
        while dials.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let first_resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::time::timeout(WAIT, transport.suspend()).await??;
        tokio::time::timeout(WAIT, first_resume).await?.unwrap()?;
        let mut alpha = tokio::time::timeout(WAIT, leased).await?.unwrap()?;

        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(
            dials.load(Ordering::SeqCst),
            1,
            "a resume deferred before the suspend must not redial"
        );

        let second_resume = tokio::spawn({
            let transport = transport.clone();
            async move { transport.resume().await }
        });
        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.ack_empty(resume.id);
        tokio::time::timeout(WAIT, second_resume).await?.unwrap()?;
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_resume_burst_during_an_outage_dials_once() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Notify::new());
        let network_down = Arc::new(AtomicBool::new(true));
        let transport: BidiTransport<BackendBinding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            let gate = gate.clone();
            let network_down = network_down.clone();
            BidiTransport::new(
                move |initial| {
                    let n = dials.fetch_add(1, Ordering::SeqCst);
                    let sink = sink.clone();
                    let gate = gate.clone();
                    let network_down = network_down.clone();
                    async move {
                        if n == 1 {
                            gate.notified().await;
                            return Err(OpenError::retryable(std::io::Error::other("down")));
                        }
                        if n >= 2 && network_down.load(Ordering::SeqCst) {
                            return Err(OpenError::retryable(std::io::Error::other("down")));
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
        };

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        tokio::time::timeout(WAIT, transport.suspend()).await??;

        let resumes: Vec<_> = (0..3)
            .map(|i| {
                let transport = transport.clone();
                tokio::spawn(async move {
                    if i > 0 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    transport.resume().await
                })
            })
            .collect();
        while dials.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        network_down.store(false, Ordering::SeqCst);
        gate.notify_one();

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            dials.load(Ordering::SeqCst),
            2,
            "resumes deferred by the failed dial must park, not re-dial"
        );

        let mut server = wait_for_server(&servers).await;
        let resume = server.next_mutate().await;
        server.ack_empty(resume.id);
        for handle in resumes {
            tokio::time::timeout(WAIT, handle).await?.unwrap()?;
        }
        assert_eq!(dials.load(Ordering::SeqCst), 3, "exactly one retry dial");

        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn resume_with_nothing_to_do_resolves_immediately() {
        let (transport, servers) = transport();
        transport.suspend().await?;
        tokio::time::timeout(WAIT, transport.resume()).await??;

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        tokio::time::timeout(WAIT, transport.resume()).await??;
        server.send(messages(vec![group_msg(1, b"g1")], vec![]));
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::GroupMessages(_))
        ));
        assert!(
            servers.lock().unwrap().is_empty(),
            "resume on a live wire must not open a second one"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered() {
        let (transport, servers) = transport();
        let topic = group_topic(b"g1");
        let mut gamma = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut gamma).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        drop(server); // the wire dies...

        let _alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (
                    add.topic.as_ref().unwrap().topic.clone(),
                    add.cursor.as_ref().unwrap().sequence_id
                ))
                .collect::<Vec<_>>(),
            vec![(topic.cloned_vec(), 5)],
            "the re-add anchors at the caught-up holder's floor"
        );

        second.ack(reissue.id, vec![(topic.clone(), 30)]);
        second.send(messages(vec![group_msg(30, b"g1")], vec![]));
        match recv(&mut gamma).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(30, b"g1")])
            }
            _ => panic!("gamma expected its outage gap"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn lease_during_a_dead_wire_rides_the_resume_open() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let server = take_server(&servers);
        drop(server);

        let mut beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let mut second = wait_for_server(&servers).await;

        let mut adds: Vec<(Vec<u8>, u64)> = Vec::new();
        let mut waves = Vec::new();
        while adds.len() < 2 {
            let mutate = second.next_mutate().await;
            waves.push(mutate.id);
            adds.extend(mutate.adds.iter().map(|add| {
                (
                    add.topic.as_ref().unwrap().topic.clone(),
                    add.cursor.as_ref().unwrap().sequence_id,
                )
            }));
        }
        adds.sort();
        let mut expected = vec![
            (group_topic(b"g1").cloned_vec(), 3),
            (group_topic(b"g2").cloned_vec(), 7),
        ];
        expected.sort();
        assert_eq!(adds, expected);
        assert!(
            servers.lock().unwrap().is_empty(),
            "one wire serves everyone"
        );

        for wave in waves {
            second.ack_empty(wave);
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_during_a_dead_wire_keeps_the_topic_off_the_reconnect() {
        let (transport, servers) = transport();
        let _alpha = transport.lease(vec![(group_topic(b"g1"), 3)], 8).await?;
        let beta = transport.lease(vec![(group_topic(b"g2"), 7)], 8).await?;
        let server = take_server(&servers);
        drop(server); // the wire dies

        drop(beta); // deref lands while there is no wire to send a remove on

        let mut second = wait_for_server(&servers).await;
        let reissue = second.next_mutate().await;
        assert_eq!(
            reissue
                .adds
                .iter()
                .map(|add| (
                    add.topic.as_ref().unwrap().topic.clone(),
                    add.cursor.as_ref().unwrap().sequence_id
                ))
                .collect::<Vec<_>>(),
            vec![(group_topic(b"g1").cloned_vec(), 3)],
            "the dropped lease's topic must not ride the reconnect"
        );
        assert!(
            reissue.removes.is_empty(),
            "nothing to remove on a fresh wire"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn deref_purges_only_the_dropped_leases_unsent_updates() {
        let mut ledger = Ledger::<BackendBinding>::default();
        let (a, _events_a) = mpsc::channel(8);
        let (b, _events_b) = mpsc::channel(8);
        let (g1, g3, g4) = (group_topic(b"g1"), group_topic(b"g3"), group_topic(b"g4"));
        let alpha = ledger.register(&[(g1.clone(), 0), (g4.clone(), 0)], a);
        let beta = ledger.register(&[(g3.clone(), 0)], b);
        let mut outbox = Outbox::default();
        outbox
            .updates
            .extend(ledger.prepare_adds(vec![(g1.clone(), 0)]));
        outbox
            .updates
            .extend(ledger.prepare_removes(vec![group_topic(b"g2")]));
        outbox.updates.extend(ledger.prepare_adds(vec![(g3, 0)]));
        outbox
            .updates
            .extend(ledger.prepare_adds(vec![(g4.clone(), 0)]));
        let (cmds, receiver) = mpsc::unbounded_channel();
        let mut task = LedgerTask {
            opener: Box::new(
                |_| -> BoxDynFuture<'static, Result<Connection<BackendBinding>, OpenError>> {
                    Box::pin(std::future::pending())
                },
            ),
            cmds: receiver,
            lease_cmds: cmds.downgrade(),
            ledger,
            conn: None,
            reconnect_delay: RECONNECT_INITIAL_DELAY,
            reconnect_at: tokio::time::Instant::now(),
            wire_opened_at: None,
            wire_span: None,
            suspended: false,
            wire_opens: 0,
            resume_notify: vec![],
            outbox,
            deferred: std::collections::VecDeque::new(),
        };
        let removed: HashSet<_> = task.drop_leases(vec![alpha]).into_iter().collect();
        assert_eq!(removed, HashSet::from([g1, g4]));
        let remaining: Vec<_> = task.outbox.updates.iter().map(|(id, _)| *id).collect();
        assert_eq!(remaining, vec![2, 3]);
        assert_eq!(
            task.ledger
                .pending_updates
                .keys()
                .copied()
                .collect::<HashSet<_>>(),
            HashSet::from([2, 3])
        );
        assert!(task.ledger.leases.contains_key(&beta));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn empty_lease_is_refused_without_opening_the_wire() {
        let (transport, servers) = transport();
        let refused = transport.lease(vec![], 8).await;
        assert!(matches!(refused, Err(TransportError::Empty)));
        assert!(servers.lock().unwrap().is_empty());
    }

    #[derive(Debug, thiserror::Error)]
    #[error("no wire for you")]
    struct Refused;

    impl xmtp_common::RetryableError for Refused {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn open_failure_surfaces_and_registers_nothing() {
        let transport = BidiTransport::<BackendBinding>::new(
            |_initial| async { Err(OpenError::unretryable(Refused)) },
            false,
        );
        let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(denied, Err(TransportError::Open(_))));
        let again = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(again, Err(TransportError::Open(_))));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn unretryable_reconnect_closes_every_lease() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let servers: Servers = Arc::default();
        let dials = Arc::new(AtomicUsize::new(0));
        let transport: BidiTransport<BackendBinding> = {
            let sink = servers.clone();
            let dials = dials.clone();
            BidiTransport::new(
                move |initial| {
                    let n = dials.fetch_add(1, Ordering::SeqCst);
                    let sink = sink.clone();
                    async move {
                        if n > 0 {
                            return Err(OpenError::new(Refused));
                        }
                        let (api, server) = mock_pair();
                        sink.lock().unwrap().push(server);
                        BidiConnection::open(&api, initial)
                            .await
                            .map_err(OpenError::new)
                    }
                },
                false,
            )
        };

        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let mut server = take_server(&servers);
        let first = server.next_mutate().await;
        server.ack_empty(first.id);
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(server); // the wire dies; the reconnect dial is refused

        assert!(
            recv(&mut alpha).await.is_none(),
            "an unretryable reconnect must end every lease, not redial forever"
        );
        assert_eq!(dials.load(Ordering::SeqCst), 2, "no dial after the refusal");
        let denied = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await;
        assert!(matches!(denied, Err(TransportError::Closed)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_retire_remove_is_acked_without_closing_the_transport() {
        let (transport, servers) = transport();
        let mut alpha = transport.lease(vec![(group_topic(b"g1"), 0)], 8).await?;
        let beta = transport.lease(vec![(group_topic(b"g2"), 0)], 8).await?;
        let mut server = take_server(&servers);
        for _ in 0..2 {
            let wave = server.next_mutate().await;
            server.ack_empty(wave.id);
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        drop(beta);
        let removes = server.next_mutate().await;
        assert!(removes.adds.is_empty());
        assert_eq!(
            removes.removes,
            vec![backend_v1::Topic {
                topic: group_topic(b"g2").cloned_vec()
            }]
        );
        assert_ne!(removes.id, 0, "a remove update must have a nonzero ID");

        server.ack_empty(removes.id);
        server.send(messages(vec![group_msg(7, b"g1")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(7, b"g1")]),
            _ => panic!("alpha must keep receiving after a retire ack"),
        }
        let _gamma = transport
            .lease(vec![(group_topic(b"g3"), 0)], 8)
            .await
            .expect("the transport must survive a retire ack");
    }
    #[xmtp_common::test(unwrap_try = true)]
    async fn target_zero_completes_only_after_applied() {
        let (transport, servers) = transport();
        let topic = group_topic(b"empty");
        let mut lease = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let update = server.next_mutate().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), lease.next())
                .await
                .is_err()
        );
        server.ack(update.id, vec![(topic, 0)]);
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        server.send(messages(vec![group_msg(6, b"empty")], vec![]));
        match recv(&mut lease).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(6, b"empty")]),
            _ => panic!("the empty registration must continue receiving messages"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn target_equal_to_floor_needs_no_delivery() {
        let (transport, servers) = transport();
        let topic = group_topic(b"equal");
        let mut lease = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let update = server.next_mutate().await;
        server.ack(update.id, vec![(topic, 5)]);
        assert!(matches!(
            recv(&mut lease).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        server.send(messages(
            vec![group_msg(5, b"equal"), group_msg(6, b"equal")],
            vec![],
        ));
        match recv(&mut lease).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(6, b"equal")]),
            _ => panic!("the floor must remain exclusive after completion"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_topic_absent_from_targets_inherits_the_registration() {
        // Drive the same ledger through a no-op add acknowledgement. Normal
        // lease joins need no update when the registration can already serve them.
        let topic = group_topic(b"shared");
        let mut ledger = Ledger::<BackendBinding>::default();
        let (tx_a, mut events_a) = mpsc::channel(8);
        let alpha = ledger.register(&[(topic.clone(), 0)], tx_a);
        let (initial_id, initial) = ledger.prepare_adds(vec![(topic.clone(), 0)]).remove(0);
        let (api, mut server) = mock_pair();
        let mut connection = BidiConnection::open(&api, initial).await?;
        server.next_mutate().await;
        assert!(matches!(
            connection.next().await,
            Some(Event::Started { .. })
        ));
        server.ack(initial_id, vec![(topic.clone(), 10)]);
        let Some(Event::Applied { id, targets }) = connection.next().await else {
            panic!("expected Applied");
        };
        assert!(ledger.applied(id, targets).is_empty());
        assert!(ledger.recheck().is_empty());

        let (tx_b, mut events_b) = mpsc::channel(8);
        let beta = ledger.register(&[(topic.clone(), 5)], tx_b);
        assert!(ledger.join(beta, vec![(topic.clone(), 5)]).is_empty());
        let (noop_id, noop) = ledger.prepare_adds(vec![(topic.clone(), 5)]).remove(0);
        connection.mutate(noop).await?;
        server.next_mutate().await;
        server.ack_empty(noop_id);
        let Some(Event::Applied { id, targets }) = connection.next().await else {
            panic!("expected Applied");
        };
        assert_eq!(id, noop_id);
        assert!(
            targets.is_empty(),
            "an active topic is absent from added_targets"
        );
        assert!(ledger.applied(id, targets).is_empty());
        assert!(ledger.recheck().is_empty());
        assert_eq!(ledger.leases[&alpha].unmet, 1);
        assert_eq!(ledger.leases[&beta].unmet, 1);
        assert!(events_a.try_recv().is_err());
        assert!(
            events_b.try_recv().is_err(),
            "an empty target list is not completion"
        );

        server.send(messages(vec![group_msg(10, b"shared")], vec![]));
        let Some(Event::GroupMessages { messages }) = connection.next().await else {
            panic!("expected messages");
        };
        assert!(
            ledger
                .demux(
                    messages,
                    DeliveryKind::Group,
                    BackendBinding::group_topic,
                    BackendBinding::group_cursor,
                    LeaseEvent::GroupMessages
                )
                .is_empty()
        );
        assert!(ledger.recheck().is_empty());
        for events in [&mut events_a, &mut events_b] {
            match events.recv().await {
                Some(LeaseEvent::GroupMessages(got)) => {
                    assert_eq!(got, vec![group_msg(10, b"shared")])
                }
                _ => panic!("both holders must receive the target"),
            }
            assert!(matches!(
                events.recv().await,
                Some(LeaseEvent::CatchUpComplete)
            ));
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn two_holders_keep_independent_monotonic_floors() {
        let (transport, servers) = transport();
        let topic = group_topic(b"shared");
        let mut alpha = transport.lease(vec![(topic.clone(), 0)], 8).await?;
        let mut beta = transport.lease(vec![(topic.clone(), 5)], 8).await?;
        let mut server = take_server(&servers);
        let initial = server.next_mutate().await;
        server.ack(initial.id, vec![(topic, 10)]);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), server.from_client.recv())
                .await
                .is_err(),
            "a holder at or above the wire floor needs no update"
        );
        let all: Vec<_> = [1, 2, 5, 6, 10]
            .into_iter()
            .map(|id| group_msg(id, b"shared"))
            .collect();
        server.send(messages(all.clone(), vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, all),
            _ => panic!("alpha must receive its full suffix"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => {
                assert_eq!(got, vec![group_msg(6, b"shared"), group_msg(10, b"shared")])
            }
            _ => panic!("beta must receive only messages above its floor"),
        }
        for lease in [&mut alpha, &mut beta] {
            assert!(matches!(
                recv(lease).await,
                Some(LeaseEvent::CatchUpComplete)
            ));
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn lower_cursor_readd_routes_queued_messages_at_applied_boundaries() {
        let (transport, servers) = transport();
        let topic = group_topic(b"shared");
        let mut alpha = transport.lease(vec![(topic.clone(), 50)], 8).await?;
        let mut server = take_server(&servers);
        let initial = server.next_mutate().await;
        server.ack(initial.id, vec![(topic.clone(), 60)]);
        let first: Vec<_> = (51..=60).map(|id| group_msg(id, b"shared")).collect();
        server.send(messages(first.clone(), vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, first),
            _ => panic!("alpha must receive its initial history"),
        }
        assert!(matches!(
            recv(&mut alpha).await,
            Some(LeaseEvent::CatchUpComplete)
        ));

        let mut beta = transport.lease(vec![(topic.clone(), 40)], 8).await?;
        let remove = server.next_mutate().await;
        assert!(remove.adds.is_empty());
        assert_eq!(
            remove.removes,
            vec![backend_v1::Topic {
                topic: topic.cloned_vec()
            }]
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), server.from_client.recv())
                .await
                .is_err(),
            "the add must wait for the remove acknowledgement"
        );
        // This message was queued by the old registration before removal.
        server.send(messages(vec![group_msg(61, b"shared")], vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(61, b"shared")]),
            _ => panic!("the old holder must receive the queued message"),
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), beta.next())
                .await
                .is_err(),
            "the new holder must not receive the old registration"
        );

        server.ack_empty(remove.id);
        let add = server.next_mutate().await;
        assert!(add.id > remove.id);
        assert!(add.removes.is_empty());
        assert_eq!(add.adds.len(), 1);
        assert_eq!(
            add.adds[0].topic.as_ref().unwrap().topic,
            topic.cloned_vec()
        );
        assert_eq!(add.adds[0].cursor.as_ref().unwrap().sequence_id, 40);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), beta.next())
                .await
                .is_err()
        );
        server.ack(add.id, vec![(topic, 62)]);
        let replayed: Vec<_> = (41..=62).map(|id| group_msg(id, b"shared")).collect();
        server.send(messages(replayed.clone(), vec![]));
        match recv(&mut alpha).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, vec![group_msg(62, b"shared")]),
            _ => panic!("alpha must skip all overlap, including queued message 61"),
        }
        match recv(&mut beta).await {
            Some(LeaseEvent::GroupMessages(got)) => assert_eq!(got, replayed),
            _ => panic!("beta must receive every message from 41, including 61"),
        }
        assert!(matches!(
            recv(&mut beta).await,
            Some(LeaseEvent::CatchUpComplete)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), alpha.next())
                .await
                .is_err(),
            "an old holder must not receive another completion"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn a_lease_cannot_push_the_wire_past_the_topic_limit() {
        let (transport, servers) = transport();
        let limit = xmtp_configuration::BACKEND_DEFAULT_MAX_STREAM_TOPICS;
        let topics = (0..limit)
            .map(|id| {
                let mut group = [0u8; 16];
                group[..8].copy_from_slice(&(id as u64).to_le_bytes());
                (group_topic(&group), 0)
            })
            .collect();
        let _full = transport.lease(topics, 8).await?;
        let mut server = take_server(&servers);
        let initial = server.next_mutate().await;
        assert_eq!(initial.adds.len(), limit);
        let extra = group_topic(&[0xff; 16]);
        assert!(matches!(
            transport.lease(vec![(extra, 0)], 8).await,
            Err(TransportError::TooManyTopics)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), server.from_client.recv())
                .await
                .is_err(),
            "a refused lease must send no update"
        );
        assert!(
            servers.lock().unwrap().is_empty(),
            "a refused lease must open no extra wire"
        );
    }
}
