//! XIP-83 client-level stream router.
//!
//! One [`StreamRouter`] per client. Each consumer stream leases its topics
//! from the process-level [`BidiTransport`], decodes raw wire deliveries
//! through the shared pipeline seams ([`process_one`] /
//! [`ProcessWelcomeFuture`]), and delivers decoded results on a bounded
//! channel. Everything here speaks *stream* vocabulary — streams, messages,
//! conversations, durable cursors — the transport's wire/topic/envelope world
//! stays below the lease boundary.
//!
//! ## Shape: one task per stream, nothing shared
//!
//! Every stream is its own task owning its lease, its dedup state, and its
//! delivery channel. Streams share no mutable state, so a stall in one — say
//! a multi-second recovery sync inside the pipeline — backs up only that
//! stream's own lease channel (where the transport's drop policy applies to
//! it alone) and never delays a sibling. The router task itself only seeds,
//! leases, and spawns; it holds no delivery path.
//!
//! Within a stream, message decode stays inline — but welcome processing
//! does not: joining a group can run a full network sync, and a welcome
//! inline on the lease-draining loop would wedge the whole stream behind it
//! (past the lease's channel bound, the transport drops the lease and the
//! stream ends). Welcomes and local-group events fan out to a capped set of
//! spawned tasks instead (see [`WelcomeIntake`]).
//!
//! ## Cursors and dedup
//!
//! Streams are seeded from the client's **durable** application cursors
//! (`refresh_state`), never wire cursors: the cursored lease is the one
//! catch-up mechanism (a re-add below the server's floor replays history).
//! Steady-state dedup is a per-topic [`GlobalCursor`] position — the
//! transport fans a shared topic to every lease, and each stream
//! independently skips what it has already delivered, while decrypt/store
//! still happens once per unique message (the pipeline's DB fast-path serves
//! every later copy from storage).
//!
//! During a stream's **catch-up window** (subscribe until its
//! `CatchUpComplete`) wire order is not cursor-monotonic, so the position is
//! not consulted; the window dedups against the frozen subscribe-time seed
//! (drop everything at-or-below it — including a sibling's replay of older
//! history) plus an exact-identity seen-set (see [`StreamDedup`]).
//!
//! Welcome dedup is a per-stream known-welcome set: consulted at intake on
//! the stream's own task, snapshotted into each processing task, and
//! recorded back at completion — the consumers' known/positions guards make
//! completions idempotent, so concurrent same-cursor flights collapse to one
//! delivery. Exact-identity sets are immune to the ordering caveat, so
//! welcome streams need no window.
//!
//! ## Backpressure (same policy as every layer below)
//!
//! A stream that stops draining its bounded channel is dropped: its channel
//! closes, its lease derefs (the transport removes last-interest topics from
//! the wire), and the consumer recovers by re-subscribing — a cursored re-add
//! from durable state. A dead wire ends every affected stream the same way;
//! transparent reconnect arrives with a later phase.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, oneshot};

use xmtp_api_d14n::v3::{V3ProtoGroupMessage, V3ProtoWelcomeMessage};
use xmtp_api_d14n::{
    BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent, TopicLease, TransportError, V3Binding,
};
use xmtp_common::task::JoinSet;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::mls_v1;
use xmtp_proto::types::{Cursor, GlobalCursor, GroupId, InstallationId, SequenceId, Topic};

use xmtp_db::group::GroupQueryArgs;

use super::process_message::{ProcessMessageFuture, process_one};
use super::process_welcome::{ProcessWelcomeFuture, WelcomeOutcome};
use super::{LocalEvents, Result, SubscribeError, SyncWorkerEvent, WelcomeOrGroup};
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::groups::welcome_sync::WelcomeService;

/// Default per-stream channel depth (chooseable per stream).
pub const DEFAULT_STREAM_DEPTH: usize = 16;

/// Cap on in-flight welcome/local-group processing per stream. Enough to
/// ride out a slow join (each task may run a network sync) without fanning a
/// replay burst out into unbounded tasks; arrivals past the cap park in the
/// intake's backlog.
const MAX_WELCOME_TASKS: usize = 16;

/// Cap on the intake backlog of arrivals parked past [`MAX_WELCOME_TASKS`].
/// Welcome volume is join-rate-bounded, so this is generous for any
/// realistic join burst. Overflow means the stream is unrecoverably behind —
/// end it: the documented re-subscribe recovery replays welcomes from the
/// durable cursor over the wire lease, so nothing is lost.
const MAX_WELCOME_BACKLOG: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// Leasing topics from the transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Seeding cursors failed while subscribing.
    #[error(transparent)]
    Subscribe(#[from] SubscribeError),
    /// The router task is gone (client shutdown).
    #[error("the stream router is closed")]
    Closed,
}

/// Handle to a client's router. Cheap to clone. Live streams are independent
/// tasks: they survive the last handle dropping, and the router task lingers
/// only to reap them before exiting.
pub struct StreamRouter<Context> {
    context: Context,
    cmds: mpsc::UnboundedSender<Cmd<Context>>,
}

impl<Context: Clone> Clone for StreamRouter<Context> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            cmds: self.cmds.clone(),
        }
    }
}

/// A decoded stream handed to a consumer. Dropping it unsubscribes: its task
/// exits and its topic lease derefs.
pub struct RouterStream<T> {
    id: u64,
    items: mpsc::Receiver<Result<T>>,
    ends: mpsc::UnboundedSender<u64>,
}

impl<T> RouterStream<T> {
    /// Next decoded item. `None` means the stream was closed — the wire died
    /// or this consumer fell behind and was dropped; recover by
    /// re-subscribing (which re-seeds from durable cursors).
    pub async fn next(&mut self) -> Option<Result<T>> {
        self.items.recv().await
    }
}

impl<T> Drop for RouterStream<T> {
    fn drop(&mut self) {
        let _ = self.ends.send(self.id);
    }
}

enum Cmd<Context> {
    Messages {
        group_ids: Vec<GroupId>,
        depth: usize,
        reply: oneshot::Sender<std::result::Result<RouterStream<StoredGroupMessage>, RouterError>>,
    },
    AllMessages {
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
        reply: oneshot::Sender<std::result::Result<RouterStream<StoredGroupMessage>, RouterError>>,
    },
    Conversations {
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
        reply: oneshot::Sender<std::result::Result<RouterStream<MlsGroup<Context>>, RouterError>>,
    },
}

impl<Context> StreamRouter<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Create a router over `transport`. The transport decides where the wire
    /// goes; this router only assumes its own client's topics live there.
    pub fn new(context: Context, transport: BidiTransport<V3Binding>) -> Self {
        let (cmds, cmds_rx) = mpsc::unbounded_channel();
        let (ends_tx, ends_rx) = mpsc::unbounded_channel();
        let task = RouterTask {
            context: context.clone(),
            transport,
            consumers: HashMap::new(),
            next_id: 0,
            ends: ends_tx,
        };
        xmtp_common::spawn(None, task.run(cmds_rx, ends_rx));
        Self { context, cmds }
    }

    /// Stream decoded messages for `group_ids`, resuming each group from its
    /// durable cursor (catch-up, then live, over the shared wire).
    pub async fn stream_messages(
        &self,
        group_ids: Vec<GroupId>,
        depth: usize,
    ) -> std::result::Result<RouterStream<StoredGroupMessage>, RouterError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Messages {
                group_ids,
                depth,
                reply,
            })
            .map_err(|_| RouterError::Closed)?;
        response.await.map_err(|_| RouterError::Closed)?
    }

    /// Stream every matching conversation's messages, growing with new
    /// conversations: the subscribe-time set seeds the stream, and the
    /// welcome auto-subscribe reflex leases each later-joined group's topic
    /// as its welcome arrives — no re-subscribe needed.
    pub async fn stream_all_messages(
        &self,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
    ) -> std::result::Result<RouterStream<StoredGroupMessage>, RouterError> {
        // Close the welcome gap first — same seeding as the legacy stream —
        // so the subscribe-time set is current when the welcome lease takes
        // over the live edge. On the caller's task, not the router task: the
        // router serializes subscribes, and one subscriber's network
        // round-trip must not stall every sibling subscribe (and reap)
        // behind it.
        WelcomeService::new(&self.context)
            .sync_welcomes()
            .await
            .map_err(SubscribeError::from)?;
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::AllMessages {
                conversation_type,
                consent_states,
                depth,
                reply,
            })
            .map_err(|_| RouterError::Closed)?;
        response.await.map_err(|_| RouterError::Closed)?
    }

    /// Stream new conversations from this client's welcome topic, resuming
    /// from the durable welcome cursor.
    pub async fn stream_conversations(
        &self,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
    ) -> std::result::Result<RouterStream<MlsGroup<Context>>, RouterError> {
        let (reply, response) = oneshot::channel();
        self.cmds
            .send(Cmd::Conversations {
                conversation_type,
                include_duplicate_dms,
                consent_states,
                depth,
                reply,
            })
            .map_err(|_| RouterError::Closed)?;
        response.await.map_err(|_| RouterError::Closed)?
    }
}

/// Dedup for one message stream.
///
/// Steady state is the per-topic position: wire order is cursor-monotonic,
/// so a high-water mark suffices. During a topic's catch-up window it is
/// not — a live delivery already in flight for a shared topic can arrive
/// *before* the replay this stream's cursored add requested (see the
/// transport's delivery-order contract) — so the window checks the **frozen
/// subscribe-time seed** (anything at-or-below the durable resume point is
/// never for this stream, including a sibling lease's replay of older
/// history) plus an exact-identity seen-set.
///
/// Windows are **per topic**: each lease's `CatchUpComplete` closes exactly
/// the topics that lease added (a stream that grows mid-flight has one lease
/// per addition, each with its own window), and the live positions — which
/// kept advancing — take over per topic. The seen-set is shared across open
/// windows (exact identity is safe across groups), records only for topics
/// whose own window is open (a closed window's deliveries are already
/// position-deduped), and drops when the last window closes.
struct StreamDedup {
    /// Topics still inside their catch-up window → their frozen seed.
    syncing: HashMap<Topic, GlobalCursor>,
    /// Exact identities delivered while any window is open.
    seen: HashSet<Cursor>,
}

impl StreamDedup {
    /// `seen` starts with the identities already stored locally (delivered by
    /// an earlier stream or a sync) — the durable cursor alone is not a
    /// delivery floor, because the streaming pipeline stores without
    /// advancing it.
    fn syncing(seeds: HashMap<Topic, GlobalCursor>, seen: HashSet<Cursor>) -> Self {
        Self {
            syncing: seeds,
            seen,
        }
    }

    /// Open windows for late-added topics (a growing stream's new lease),
    /// folding their locally-stored identities into the shared seen-set.
    fn open_window(
        &mut self,
        seeds: HashMap<Topic, GlobalCursor>,
        seen: impl IntoIterator<Item = Cursor>,
    ) {
        self.syncing.extend(seeds);
        self.seen.extend(seen);
    }

    fn has_seen(&self, position: &GlobalCursor, topic: &Topic, cursor: &Cursor) -> bool {
        match self.syncing.get(topic) {
            Some(seed) => seed.has_seen(cursor) || self.seen.contains(cursor),
            None => position.has_seen(cursor),
        }
    }

    /// Record a delivered identity — only while `topic`'s own window is
    /// open. Once the window closes the position governs that topic, so
    /// recording would only grow the seen-set for as long as any sibling
    /// window stays open.
    fn record(&mut self, topic: &Topic, cursor: Cursor) {
        if self.syncing.contains_key(topic) {
            self.seen.insert(cursor);
        }
    }

    /// Close the window for `topics` (their lease's `CatchUpComplete`).
    fn complete(&mut self, topics: &[Topic]) {
        for topic in topics {
            self.syncing.remove(topic);
        }
        if self.syncing.is_empty() {
            self.seen = HashSet::new();
        }
    }
}

/// The reaping handle the router keeps per live stream. Dropping the kill
/// sender (router shutdown with no live `RouterStream`s, or an explicit end)
/// stops the stream's task, which drops its lease.
struct ConsumerHandle {
    _kill: oneshot::Sender<()>,
}

struct RouterTask<Context> {
    context: Context,
    transport: BidiTransport<V3Binding>,
    consumers: HashMap<u64, ConsumerHandle>,
    next_id: u64,
    ends: mpsc::UnboundedSender<u64>,
}

impl<Context> RouterTask<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(
        mut self,
        mut cmds: mpsc::UnboundedReceiver<Cmd<Context>>,
        mut ends: mpsc::UnboundedReceiver<u64>,
    ) {
        let mut handles_gone = false;
        loop {
            // Streams outlive the handles: exit only once no handle can mint
            // a new stream AND every live stream has ended.
            if handles_gone && self.consumers.is_empty() {
                return;
            }
            tokio::select! {
                cmd = cmds.recv(), if !handles_gone => match cmd {
                    None => handles_gone = true,
                    Some(cmd) => self.subscribe(cmd).await,
                },
                Some(id) = ends.recv() => {
                    self.consumers.remove(&id);
                }
            }
        }
    }

    async fn subscribe(&mut self, cmd: Cmd<Context>) {
        // A failed reply (the caller cancelled mid-subscribe) drops the
        // returned `RouterStream` right here, and its `Drop` routes back
        // through `ends` to reap the just-spawned consumer — nothing leaks.
        match cmd {
            Cmd::Messages {
                group_ids,
                depth,
                reply,
            } => {
                let _ = reply.send(self.subscribe_messages(group_ids, depth).await);
            }
            Cmd::AllMessages {
                conversation_type,
                consent_states,
                depth,
                reply,
            } => {
                let _ = reply.send(
                    self.subscribe_all_messages(conversation_type, consent_states, depth)
                        .await,
                );
            }
            Cmd::Conversations {
                conversation_type,
                include_duplicate_dms,
                consent_states,
                depth,
                reply,
            } => {
                let _ = reply.send(
                    self.subscribe_conversations(
                        conversation_type,
                        include_duplicate_dms,
                        consent_states,
                        depth,
                    )
                    .await,
                );
            }
        }
    }

    async fn subscribe_messages(
        &mut self,
        group_ids: Vec<GroupId>,
        depth: usize,
    ) -> std::result::Result<RouterStream<StoredGroupMessage>, RouterError> {
        let db = self.context.db();
        let seeds = seed_groups(&db, &group_ids)?;

        let lease = self
            .transport
            .lease(seeds.subs(), DEFAULT_LEASE_DEPTH)
            .await?;
        let (tx, items) = mpsc::channel(depth.max(1));
        let consumer = MessageConsumer {
            factory: ProcessMessageFuture::new(self.context.clone()),
            leases: LeaseSet::new(lease),
            tx,
            dedup: StreamDedup::syncing(seeds.floors, seeds.seen),
            positions: seeds.positions,
            reflex: None,
        };
        let id = self.spawn(|kill| consumer.run(kill));
        Ok(RouterStream {
            id,
            items,
            ends: self.ends.clone(),
        })
    }

    async fn subscribe_all_messages(
        &mut self,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
    ) -> std::result::Result<RouterStream<StoredGroupMessage>, RouterError> {
        // The local-events receiver subscribes BEFORE the seed query: a group
        // created in between is either in the query result or buffered on the
        // receiver — never dropped.
        let local_events = self.context.local_events().subscribe();
        let db = self.context.db();
        let installation = self.context.installation_id();
        // Welcome floor and known set BEFORE the group query: a welcome
        // processed in between then shows up in the query result AND above
        // the floor — the positions guard absorbs that overlap. The reverse
        // order would leave it in neither: not yet a group when queried,
        // already below the floor when leased.
        let welcome_floor = welcome_seed(&db, installation)?;
        let known = known_welcomes_above(&db, welcome_floor)?;
        let groups = db
            .find_groups(GroupQueryArgs {
                conversation_type,
                consent_states: consent_states.clone(),
                include_duplicate_dms: true,
                // Sync groups are subscribed — their traffic nudges the
                // device-sync worker — but intercepted at delivery, exactly
                // like the legacy stream: internal payloads never surface.
                include_sync_groups: conversation_type
                    .map(|ct| matches!(ct, ConversationType::Sync))
                    .unwrap_or(true),
                ..Default::default()
            })
            .map_err(SubscribeError::from)?;
        let sync_groups: HashSet<GroupId> = groups
            .iter()
            .filter(|g| matches!(g.conversation_type, ConversationType::Sync))
            .map(|g| g.id)
            .collect();
        let group_ids: Vec<GroupId> = groups.into_iter().map(|g| g.id).collect();
        let seeds = seed_groups(&db, &group_ids)?;

        // One lease for the subscribe-time set. The welcome topic rides in
        // it, so even an account with no conversations holds a live lease —
        // the stream is simply one that has not grown yet.
        let mut subs = seeds.subs();
        subs.push((Topic::new_welcome_message(installation), welcome_floor));

        let lease = self.transport.lease(subs, DEFAULT_LEASE_DEPTH).await?;
        let (tx, items) = mpsc::channel(depth.max(1));
        let consumer = MessageConsumer {
            factory: ProcessMessageFuture::new(self.context.clone()),
            leases: LeaseSet::new(lease),
            tx,
            dedup: StreamDedup::syncing(seeds.floors, seeds.seen),
            positions: seeds.positions,
            reflex: Some(Reflex {
                transport: self.transport.clone(),
                local_events,
                sync_groups,
                intake: WelcomeIntake::new(
                    self.context.clone(),
                    welcome_floor,
                    known,
                    conversation_type,
                    true,
                    consent_states,
                ),
            }),
        };
        let id = self.spawn(|kill| consumer.run(kill));
        Ok(RouterStream {
            id,
            items,
            ends: self.ends.clone(),
        })
    }

    async fn subscribe_conversations(
        &mut self,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
        depth: usize,
    ) -> std::result::Result<RouterStream<MlsGroup<Context>>, RouterError> {
        // Before the cursor seed, so a group created mid-subscribe is either
        // past the seed (replayed) or buffered on the receiver.
        let local_events = self.context.local_events().subscribe();
        let db = self.context.db();
        let installation = self.context.installation_id();
        // Wire resume point: the durable welcome cursor — every welcome
        // *processed* (stored, ignored, or filtered) advances it, so nothing
        // already handled is replayed.
        let seed = welcome_seed(&db, installation)?;
        // Classification input for the pipeline: the welcome ids that became
        // groups. Per-stream — each stream's dedup is its own, so one
        // stream's delivery never suppresses another's (they may hold
        // different filters).
        let known = known_welcomes_above(&db, seed)?;

        let topic = Topic::new_welcome_message(installation);
        let lease = self
            .transport
            .lease(vec![(topic, seed)], DEFAULT_LEASE_DEPTH)
            .await?;
        let (tx, items) = mpsc::channel(depth.max(1));
        let consumer = WelcomeConsumer {
            lease,
            tx,
            local_events,
            intake: WelcomeIntake::new(
                self.context.clone(),
                seed,
                known,
                conversation_type,
                include_duplicate_dms,
                consent_states,
            ),
        };
        let id = self.spawn(|kill| consumer.run(kill));
        Ok(RouterStream {
            id,
            items,
            ends: self.ends.clone(),
        })
    }

    fn spawn<F, Fut>(&mut self, consumer: F) -> u64
    where
        F: FnOnce(oneshot::Receiver<()>) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;
        let (kill, kill_rx) = oneshot::channel();
        let ends = self.ends.clone();
        let task = consumer(kill_rx);
        xmtp_common::spawn(None, async move {
            task.await;
            // A stream that ends on its own (wire death, backpressure drop)
            // frees its consumer slot without waiting for the exhausted
            // `RouterStream` handle to be dropped — idempotent with that
            // handle's own drop notification.
            let _ = ends.send(id);
        });
        self.consumers.insert(id, ConsumerHandle { _kill: kill });
        id
    }
}

/// One seeding of a set of group topics from their durable cursors.
///
/// The streaming pipeline stores messages WITHOUT advancing the durable
/// cursor (`allow_cursor_increment=false`), so the cursor alone is not a
/// delivery floor: everything streamed since the last full sync is stored
/// but still above it. Those exact identities seed the window's seen-set (so
/// the server's replay of them is skipped) and fold into the live positions
/// (so they stay skipped after the window). The wire cursor and the window
/// floor stay at the durable cursor — a stored gap (6 and 8 stored, 7
/// missed) still gets 7 replayed and delivered.
struct GroupSeeds {
    /// Frozen per-topic window floors: the durable resume points.
    floors: HashMap<Topic, GlobalCursor>,
    /// Live per-topic positions: the floors folded with stored identities.
    positions: HashMap<Topic, GlobalCursor>,
    /// The window's exact-identity seen-set: the stored identities.
    seen: HashSet<Cursor>,
}

impl GroupSeeds {
    /// The cursored adds for this seeding's lease wave: each topic from its
    /// frozen floor — the cursored lease makes the server replay anything
    /// past it (catch-up == subscribe).
    fn subs(&self) -> Vec<(Topic, SequenceId)> {
        self.floors
            .iter()
            .map(|(topic, floor)| (topic.clone(), floor.max()))
            .collect()
    }
}

/// Seed `group_ids` from their durable cursors (see [`GroupSeeds`]).
fn seed_groups(
    db: &(impl QueryRefreshState + QueryGroupMessage),
    group_ids: &[GroupId],
) -> Result<GroupSeeds> {
    let seeds = db.get_last_cursor_for_ids(
        group_ids,
        &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
    )?;
    let stored = db.messages_newer_than(&seeds)?;
    let mut floors = HashMap::with_capacity(group_ids.len());
    for group_id in group_ids {
        let floor = seeds.get(group_id.as_slice()).cloned().unwrap_or_default();
        floors.insert(Topic::new_group_message(*group_id), floor);
    }
    let (positions, seen) = fold_stored(&floors, stored);
    Ok(GroupSeeds {
        floors,
        positions,
        seen,
    })
}

/// The durable welcome cursor: this installation's wire resume point for its
/// welcome topic.
fn welcome_seed(db: &impl QueryRefreshState, installation: InstallationId) -> Result<SequenceId> {
    Ok(db
        .get_last_cursor_for_ids(&[installation], &[EntityKind::Welcome])?
        .get(installation.as_slice())
        .cloned()
        .unwrap_or_default()
        .v3_welcome())
}

/// The welcome cursors that already became groups, above the stream's
/// durable floor. Below the floor the intake drops every replay outright —
/// at-or-below the durable cursor means already processed, group or not —
/// so the known set only needs the above-floor overlap (welcomes processed
/// since the last cursor advance).
fn known_welcomes_above(db: &impl QueryGroup, floor: SequenceId) -> Result<HashSet<Cursor>> {
    Ok(db
        .group_cursors()?
        .into_iter()
        .filter(|cursor| cursor.sequence_id > floor)
        .collect())
}

/// Fold locally-stored identities above the durable floors into the live
/// positions — each strictly into its own group's topic (sequence ids are not
/// scoped per group, so a cross-group apply would swallow other groups'
/// deliveries) — and collect them all as the window's exact-identity seen-set
/// (exact identity is safe across groups).
fn fold_stored(
    floors: &HashMap<Topic, GlobalCursor>,
    stored: Vec<(GroupId, Cursor)>,
) -> (HashMap<Topic, GlobalCursor>, HashSet<Cursor>) {
    let mut positions = floors.clone();
    let mut seen = HashSet::with_capacity(stored.len());
    for (group_id, cursor) in stored {
        if let Some(position) = positions.get_mut(&Topic::new_group_message(group_id)) {
            position.apply(&cursor);
        }
        seen.insert(cursor);
    }
    (positions, seen)
}

/// Send on the stream's bounded channel, aborting the park if the router
/// reaps the stream first. Without the guard, a send to a held-but-undrained
/// `RouterStream` would pin the consumer task (and its lease) on a channel
/// only the holder can unblock. `Err` means the stream must end.
async fn send_or_kill<T>(
    tx: &mpsc::Sender<T>,
    kill: &mut oneshot::Receiver<()>,
    item: T,
) -> std::result::Result<(), ()> {
    tokio::select! {
        sent = tx.send(item) => sent.map_err(|_| ()),
        _ = kill => Err(()),
    }
}

/// A stream's leases, polled as one. A static stream holds exactly its
/// subscribe-time lease; a growing stream pushes one lease per late
/// addition — the transport multiplexes them onto the one wire, and a new
/// lease IS the cursored-add wave for its topics. Every feed item carries
/// its lease's topics, so each lease's `CatchUpComplete` closes exactly its
/// own topics' dedup windows.
struct LeaseSet {
    feeds: futures::stream::SelectAll<LeaseFeed>,
}

type LeaseFeed = std::pin::Pin<
    Box<dyn futures::Stream<Item = (Arc<[Topic]>, Option<LeaseEvent<V3Binding>>)> + Send + 'static>,
>;

impl LeaseSet {
    fn new(lease: TopicLease<V3Binding>) -> Self {
        let mut set = Self {
            feeds: futures::stream::SelectAll::new(),
        };
        set.push(lease);
        set
    }

    fn push(&mut self, lease: TopicLease<V3Binding>) {
        // Captured once at push: the topics this lease's `CatchUpComplete`
        // closes. The lease lives inside its feed; a `None` event marks its
        // death.
        let topics: Arc<[Topic]> = lease.topics().into();
        self.feeds.push(Box::pin(futures::stream::unfold(
            Some(lease),
            move |lease| {
                let topics = topics.clone();
                async move {
                    let mut lease = lease?;
                    match lease.next().await {
                        Some(event) => Some(((topics, Some(event)), Some(lease))),
                        None => Some(((topics, None), None)),
                    }
                }
            },
        )));
    }

    /// Next event across every lease, tagged with its lease's topics. `None`
    /// means a lease closed — the wire died or the transport dropped this
    /// consumer — and the stream ends (recovery is a re-subscribe from
    /// durable cursors, same as before).
    async fn next(&mut self) -> Option<(Arc<[Topic]>, LeaseEvent<V3Binding>)> {
        use futures::StreamExt;
        match self.feeds.next().await? {
            (topics, Some(event)) => Some((topics, event)),
            (_, None) => None,
        }
    }
}

/// What the `LocalEvents` arm woke up for.
enum LocalWake {
    Event(LocalEvents),
    /// The broadcast lapped this receiver: events were dropped unseen.
    Lagged,
}

/// The next local event, parking forever on close (the lease side ends the
/// stream). Lag is surfaced, not absorbed: a dropped `NewGroup` is a group
/// nothing will re-announce — the message consumer reconciles, the
/// conversation stream warns (legacy parity: recovery is a re-subscribe).
async fn next_local_wake(events: &mut broadcast::Receiver<LocalEvents>) -> LocalWake {
    match events.recv().await {
        Ok(event) => LocalWake::Event(event),
        Err(broadcast::error::RecvError::Lagged(n)) => {
            tracing::warn!("stream router: missed {n} local events");
            LocalWake::Lagged
        }
        Err(broadcast::error::RecvError::Closed) => std::future::pending().await,
    }
}

/// Shared intake for the two growing consumers: welcomes off the wire and
/// locally-created groups off the `LocalEvents` broadcast, run through the
/// same pipeline ([`ProcessWelcomeFuture`]) and filters.
///
/// Processing is spawned, not inlined: a welcome can take seconds (joining
/// runs a network sync), and the intake runs on the lease-draining task —
/// inline processing would stop the drain and wedge the lease past its
/// channel bound, ending the whole stream (the transport's backpressure
/// drop). The legacy conversation stream spawns onto a `JoinSet` for the
/// same reason; this one is capped, with arrivals past the cap parked in
/// the backlog.
///
/// Each task snapshots the known set at spawn (the pipeline's contract), so
/// concurrent same-cursor flights are possible; they resolve at completion,
/// where the consumers' known/positions guards make them idempotent.
struct WelcomeIntake<Context> {
    context: Context,
    /// The stream's durable welcome cursor at subscribe. Every welcome
    /// at-or-below it was already processed — stored, ignored, filtered, or
    /// failed-and-advanced — so a replay of one (the transport can serve
    /// below this stream's own floor: a shared-topic reconnect re-adds at
    /// the lowest sibling position) is dropped at intake instead of
    /// re-running the pipeline, which would re-emit `Err` outcomes for
    /// welcomes that never became groups.
    floor: SequenceId,
    /// Welcome dedup above the floor: every cursor this stream has resolved
    /// (or knew at subscribe). Consulted at intake, snapshotted per task,
    /// recorded back by the consumer at completion.
    known: HashSet<Cursor>,
    conversation_type: Option<ConversationType>,
    include_duplicate_dms: bool,
    consent_states: Option<Vec<ConsentState>>,
    /// In-flight processing, capped at [`MAX_WELCOME_TASKS`]. Dropped with
    /// the consumer, which aborts every in-flight task.
    tasks: JoinSet<Result<WelcomeOutcome<Context>>>,
    /// Arrivals past the cap, in arrival order; capped at
    /// [`MAX_WELCOME_BACKLOG`].
    backlog: VecDeque<WelcomeOrGroup>,
}

impl<Context> WelcomeIntake<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn new(
        context: Context,
        floor: SequenceId,
        known: HashSet<Cursor>,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Self {
        Self {
            context,
            floor,
            known,
            conversation_type,
            include_duplicate_dms,
            consent_states,
            tasks: JoinSet::new(),
            backlog: VecDeque::new(),
        }
    }

    /// Wire welcomes: decode, drop the already-processed, queue the rest.
    /// Returns `false` when the backlog overflowed (the stream must end).
    fn absorb_batch(&mut self, batch: Vec<mls_v1::WelcomeMessage>) -> bool {
        for proto in batch {
            let typed = match xmtp_proto::types::WelcomeMessage::try_from(
                V3ProtoWelcomeMessage::from(proto),
            ) {
                Ok(typed) => typed,
                Err(e) => {
                    tracing::warn!("stream router: skipping undecodable welcome: {e}");
                    continue;
                }
            };
            if typed.cursor.sequence_id <= self.floor {
                // At-or-below the durable cursor: already processed, whether
                // or not it became a group — a below-floor replay must not
                // re-run the pipeline (see `floor`).
                continue;
            }
            if self.known.contains(&typed.cursor) {
                // Already a group before subscribe, or already resolved by
                // this stream. This must NOT fall through to the pipeline —
                // its known-id path re-surfaces the group from store, which
                // would re-emit the conversation every time a sibling's
                // cursored re-add replays this welcome.
                //
                // The subscribe-window race — a welcome resolving after the
                // known set was snapshotted but before the lease registered
                // — passes both guards above once (above the floor, not yet
                // known); the pipeline's known-id path and the consumers'
                // completion guards absorb that single flight.
                continue;
            }
            if !self.enqueue(WelcomeOrGroup::Welcome(typed)) {
                return false;
            }
        }
        true
    }

    /// A locally-created group: no welcome will arrive for it, so it takes
    /// the same pipeline and filters a welcome does (no cursor — dedup is
    /// the once-per-creation broadcast plus the completion guards).
    /// Returns `false` when the backlog overflowed (the stream must end).
    fn absorb_local(&mut self, group_id: GroupId) -> bool {
        self.enqueue(WelcomeOrGroup::Group(group_id))
    }

    /// Queues an item, refilling the task set. Returns `false` — without
    /// queueing — when the backlog is at [`MAX_WELCOME_BACKLOG`] (the stream
    /// must end).
    fn enqueue(&mut self, item: WelcomeOrGroup) -> bool {
        if self.backlog.len() >= MAX_WELCOME_BACKLOG {
            return false;
        }
        self.backlog.push_back(item);
        self.pump();
        true
    }

    /// Refill the task set from the backlog, up to the cap.
    fn pump(&mut self) {
        while self.tasks.len() < MAX_WELCOME_TASKS {
            let Some(item) = self.backlog.pop_front() else {
                return;
            };
            let task = ProcessWelcomeFuture::new(
                self.known.clone(),
                self.context.clone(),
                item,
                self.conversation_type,
                self.include_duplicate_dms,
                self.consent_states.clone(),
            );
            self.tasks
                .spawn(async move { Ok(task?.process().await?.into_outcome()) });
        }
    }

    /// The next finished task, parking forever while nothing is in flight
    /// (the backlog is non-empty only at the cap, so idle means empty).
    async fn next_outcome(&mut self) -> Result<WelcomeOutcome<Context>> {
        loop {
            match self.tasks.join_next().await {
                Some(Ok(outcome)) => {
                    self.pump();
                    return outcome;
                }
                // A panicked task: skip it, like the legacy conversation
                // stream's completion arm does.
                Some(Err(e)) => {
                    tracing::warn!("stream router: welcome processing task failed: {e}");
                    self.pump();
                }
                None => return std::future::pending().await,
            }
        }
    }
}

/// What the growth arms woke up for.
enum ReflexWake<Context> {
    Local(LocalWake),
    /// A spawned welcome/local-group task finished.
    Outcome(Result<WelcomeOutcome<Context>>),
}

/// The growth machinery of an all-messages stream. The welcome topic rides
/// in the lease set: every accepted welcome — and every locally-created
/// group off the `LocalEvents` broadcast, for which no welcome will ever
/// arrive — leases its group's topic on the same wire; the new lease IS the
/// cursored-add wave, so catch-up and dedup fall out of the per-topic
/// windows. Growth never adds a sync group: the welcome path filters virtual
/// groups inside the pipeline, and the local-group path — whose pipeline
/// filter deliberately admits stored virtual groups — is guarded at
/// completion. Sync-group traffic belongs to the device-sync worker, and
/// only the subscribe-time interception set carries it there.
struct Reflex<Context> {
    transport: BidiTransport<V3Binding>,
    /// Locally-created groups — no welcome will ever arrive for these, so
    /// they grow the stream through the same add-path as welcomes.
    local_events: broadcast::Receiver<LocalEvents>,
    /// Subscribe-time sync groups: their traffic nudges the device-sync
    /// worker instead of surfacing (legacy `StreamAllMessages` parity).
    sync_groups: HashSet<GroupId>,
    intake: WelcomeIntake<Context>,
}

impl<Context> Reflex<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Both growth arms as one future; a static stream (`None`) parks
    /// forever — the lease side ends the stream.
    async fn wake(reflex: &mut Option<Self>) -> ReflexWake<Context> {
        let Some(reflex) = reflex.as_mut() else {
            return std::future::pending().await;
        };
        tokio::select! {
            local = next_local_wake(&mut reflex.local_events) => ReflexWake::Local(local),
            outcome = reflex.intake.next_outcome() => ReflexWake::Outcome(outcome),
        }
    }
}

/// How [`MessageConsumer::add_group`] left the stream.
enum AddGroup {
    /// The group's topic is leased (now, or already was).
    Tracked,
    /// Seeding failed: the group is not leased, and its welcome must stay
    /// unrecorded so a wire replay can retry it.
    Skipped,
    /// The stream must end.
    End,
}

/// One message stream: owns its leases, decodes sequentially, delivers on its
/// bounded channel. A stall here (e.g. recovery sync) backs up only this
/// stream's leases.
struct MessageConsumer<Context> {
    factory: ProcessMessageFuture<Context>,
    leases: LeaseSet,
    tx: mpsc::Sender<Result<StoredGroupMessage>>,
    /// Live per-topic positions — steady-state dedup; they keep advancing
    /// during the window so they hold the live edge when it closes.
    positions: HashMap<Topic, GlobalCursor>,
    dedup: StreamDedup,
    /// `Some` for the all-messages stream: welcomes grow the topic set.
    reflex: Option<Reflex<Context>>,
}

impl<Context> MessageConsumer<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(mut self, mut kill: oneshot::Receiver<()>) {
        loop {
            enum Wake<Context> {
                Lease(Arc<[Topic]>, LeaseEvent<V3Binding>),
                Reflex(ReflexWake<Context>),
            }
            let wake = tokio::select! {
                event = self.leases.next() => match event {
                    Some((topics, event)) => Wake::Lease(topics, event),
                    // Wire death or transport backpressure drop on any lease:
                    // the stream ends (tx drops); the consumer re-subscribes.
                    None => return,
                },
                wake = Reflex::wake(&mut self.reflex) => Wake::Reflex(wake),
                _ = &mut kill => return,
            };
            match wake {
                Wake::Reflex(ReflexWake::Local(LocalWake::Event(LocalEvents::NewGroup(id)))) => {
                    if !self.absorb_local_group(id) {
                        return;
                    }
                }
                Wake::Reflex(ReflexWake::Local(LocalWake::Event(_))) => {}
                Wake::Reflex(ReflexWake::Local(LocalWake::Lagged)) => {
                    if !self.reconcile(&mut kill).await {
                        return;
                    }
                }
                Wake::Reflex(ReflexWake::Outcome(outcome)) => {
                    if !self.absorb_outcome(outcome, &mut kill).await {
                        return;
                    }
                }
                Wake::Lease(_, LeaseEvent::GroupMessages(batch)) => {
                    if !self.deliver_batch(batch, &mut kill).await {
                        return;
                    }
                }
                Wake::Lease(topics, LeaseEvent::CatchUpComplete) => {
                    tracing::debug!("stream router: message stream caught up");
                    self.dedup.complete(&topics);
                }
                Wake::Lease(_, LeaseEvent::TopicsLive(topics)) => {
                    tracing::debug!(?topics, "stream router: topics live");
                }
                Wake::Lease(_, LeaseEvent::WelcomeMessages(batch)) => match self.reflex.as_mut() {
                    Some(reflex) => {
                        if !reflex.intake.absorb_batch(batch) {
                            return;
                        }
                    }
                    None => tracing::warn!("stream router: welcome delivery on a message lease"),
                },
            }
        }
    }

    /// A locally-created group grows the stream through the same processing
    /// (and filters) a welcome takes — unless its topic is already leased
    /// (the subscribe-time set, or an earlier growth, got it). Returns
    /// `false` when the stream must end.
    fn absorb_local_group(&mut self, group_id: GroupId) -> bool {
        if self
            .positions
            .contains_key(&Topic::new_group_message(group_id))
        {
            return true;
        }
        match self.reflex.as_mut() {
            Some(reflex) => reflex.intake.absorb_local(group_id),
            None => true,
        }
    }

    /// The `LocalEvents` broadcast lapped this stream: any number of
    /// `NewGroup` announcements are gone for good, and nothing re-announces
    /// a local group. Recover by re-running the subscribe-time group query
    /// and adding whatever the stream does not already track — the positions
    /// guard skips everything it does. Returns `false` when the stream must
    /// end.
    async fn reconcile(&mut self, kill: &mut oneshot::Receiver<()>) -> bool {
        let groups = match self.reflex.as_ref() {
            Some(reflex) => reflex.intake.context.db().find_groups(GroupQueryArgs {
                conversation_type: reflex.intake.conversation_type,
                consent_states: reflex.intake.consent_states.clone(),
                include_duplicate_dms: true,
                // Growth never adds sync groups; the subscribe-time
                // interception set is untouched by a lag.
                ..Default::default()
            }),
            None => return true,
        };
        let groups = match groups {
            Ok(groups) => groups,
            Err(e) => {
                // The lapped announcements are unrecoverable in-stream; end
                // after surfacing so a re-subscribe re-seeds from a fresh
                // query instead of running with a silent hole.
                let _ = send_or_kill(&self.tx, kill, Err(e.into())).await;
                return false;
            }
        };
        for group in groups {
            match self.add_group(group.id, kill).await {
                AddGroup::Tracked => {}
                // A group this pass cannot add has no other way back in —
                // its announcement is already lost and a local group's
                // welcome never replays — so end rather than run with a
                // silent hole (`add_group` already surfaced the error).
                AddGroup::Skipped | AddGroup::End => return false,
            }
        }
        true
    }

    /// A finished welcome/local-group task: lease the accepted group's
    /// topic, then record the welcome as known. The order matters twice
    /// over: recording only after the group is tracked keeps a transiently
    /// failed add recoverable (the unrecorded welcome replays on the wire),
    /// and the positions guard inside [`Self::add_group`] collapses
    /// concurrent same-cursor completions to one lease. Returns `false`
    /// when the stream must end.
    async fn absorb_outcome(
        &mut self,
        outcome: Result<WelcomeOutcome<Context>>,
        kill: &mut oneshot::Receiver<()>,
    ) -> bool {
        let outcome = match outcome {
            Ok(outcome) => outcome,
            // Welcome-processing errors surface as stream items — the
            // legacy stream's conversations sub-stream does the same.
            Err(e) => return send_or_kill(&self.tx, kill, Err(e)).await.is_ok(),
        };
        // Growth never adds a sync group (only the local-group path can
        // carry one this far — see [`Reflex`]) — but its welcome still
        // becomes known, like any other filtered welcome.
        let group = outcome
            .group
            .filter(|group| !matches!(group.conversation_type, ConversationType::Sync));
        let tracked = match group {
            Some(group) => match self.add_group(group.group_id, kill).await {
                AddGroup::Tracked => true,
                AddGroup::Skipped => false,
                AddGroup::End => return false,
            },
            None => true,
        };
        if tracked
            && let Some(seen) = outcome.seen
            && let Some(reflex) = self.reflex.as_mut()
        {
            reflex.intake.known.insert(seen);
        }
        true
    }

    /// Lease a newly-joined group's topic and open its catch-up window,
    /// seeding exactly as the subscribe-time set was.
    async fn add_group(&mut self, group_id: GroupId, kill: &mut oneshot::Receiver<()>) -> AddGroup {
        let topic = Topic::new_group_message(group_id);
        if self.positions.contains_key(&topic) {
            return AddGroup::Tracked;
        }
        let Some(reflex) = self.reflex.as_ref() else {
            // Static streams never grow; nothing routes here without a reflex.
            return AddGroup::Tracked;
        };
        let seeds = match seed_groups(&reflex.intake.context.db(), &[group_id]) {
            Ok(seeds) => seeds,
            Err(e) => {
                // Seeding is local DB work; surface and skip this group (its
                // unrecorded welcome replay — or a re-subscribe — retries it).
                return match send_or_kill(&self.tx, kill, Err(e)).await {
                    Ok(()) => AddGroup::Skipped,
                    Err(()) => AddGroup::End,
                };
            }
        };
        let lease = tokio::select! {
            lease = reflex.transport.lease(seeds.subs(), DEFAULT_LEASE_DEPTH) => lease,
            _ = &mut *kill => return AddGroup::End,
        };
        match lease {
            Ok(lease) => {
                self.positions.extend(seeds.positions);
                self.dedup.open_window(seeds.floors, seeds.seen);
                self.leases.push(lease);
                AddGroup::Tracked
            }
            Err(e) => {
                // A live stream's wire survives flaps (leases ride the resume
                // wave), so a failed lease means the transport is going away:
                // surface it and end; the consumer re-subscribes.
                let error = SubscribeError::from(RouterError::from(e));
                let _ = send_or_kill(&self.tx, kill, Err(error)).await;
                AddGroup::End
            }
        }
    }

    /// Returns `false` when the stream must end (its `RouterStream` is gone).
    async fn deliver_batch(
        &mut self,
        batch: Vec<mls_v1::GroupMessage>,
        kill: &mut oneshot::Receiver<()>,
    ) -> bool {
        for proto in batch {
            let typed =
                match xmtp_proto::types::GroupMessage::try_from(V3ProtoGroupMessage::from(proto)) {
                    Ok(typed) => typed,
                    Err(e) => {
                        tracing::warn!("stream router: skipping undecodable group message: {e}");
                        continue;
                    }
                };
            let topic = Topic::new_group_message(typed.group_id);
            let cursor = typed.cursor;
            let Some(position) = self.positions.get(&topic) else {
                tracing::debug!(%topic, "stream router: delivery for an unleased topic");
                continue;
            };
            if self.dedup.has_seen(position, &topic, &cursor) {
                continue;
            }

            // Decode once per unique message; replay copies resolve from the
            // DB fast-path inside the pipeline. The decode can be long (a
            // recovery sync), so it races the kill: dropping the stream must
            // release the lease promptly, and cancelling here is the same
            // mid-processing drop a pull-based stream takes at an await point.
            let processed = tokio::select! {
                processed = process_one(&self.factory, typed) => processed,
                _ = &mut *kill => return false,
            };
            let delivered = match processed {
                Ok(processed) => {
                    if let Some(position) = self.positions.get_mut(&topic) {
                        position.apply(&processed.next_cursor);
                    }
                    self.dedup.record(&topic, cursor);
                    match processed.message {
                        Some(message) => {
                            // The pipeline may surface a different message
                            // than the envelope named (recovery sync stores
                            // ahead) — record the delivered identity too, or
                            // its own replay envelope would deliver it twice.
                            self.dedup.record(
                                &topic,
                                Cursor::new(
                                    message.sequence_id as u64,
                                    message.originator_id as u32,
                                ),
                            );
                            if let Some(reflex) = &self.reflex
                                && reflex.sync_groups.contains(&message.group_id)
                            {
                                // Sync-group traffic nudges the device-sync
                                // worker; internal payloads never surface
                                // (legacy `StreamAllMessages` parity).
                                let _ = reflex
                                    .intake
                                    .context
                                    .worker_events()
                                    .send(SyncWorkerEvent::NewSyncGroupMsg);
                                continue;
                            }
                            send_or_kill(&self.tx, kill, Ok(message)).await
                        }
                        // Surfaced nothing (e.g. a commit) — position
                        // advanced, nothing to deliver.
                        None => continue,
                    }
                }
                Err(e) => send_or_kill(&self.tx, kill, Err(e)).await,
            };
            if delivered.is_err() {
                // The `RouterStream` is gone (receiver dropped, or the router
                // reaped us mid-send) — exit; dropping the lease derefs the
                // topics.
                return false;
            }
        }
        true
    }
}

/// One conversation stream: owns its lease, its filters, and its own
/// known-welcome set (per-stream, so streams with different filters never
/// suppress each other's deliveries).
///
/// Locally-created conversations merge in from the `LocalEvents` broadcast —
/// no welcome ever arrives for a group this client created, and legacy
/// multiplexes the same events. Dedup for those is the once-per-creation
/// broadcast plus the known set: a group created off a welcome pointer
/// records its welcome cursor, so the wire replay of that welcome does not
/// surface the conversation a second time.
struct WelcomeConsumer<Context> {
    lease: TopicLease<V3Binding>,
    tx: mpsc::Sender<Result<MlsGroup<Context>>>,
    local_events: broadcast::Receiver<LocalEvents>,
    intake: WelcomeIntake<Context>,
}

impl<Context> WelcomeConsumer<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(mut self, mut kill: oneshot::Receiver<()>) {
        loop {
            enum Wake<Context> {
                Lease(LeaseEvent<V3Binding>),
                Local(LocalWake),
                Outcome(Result<WelcomeOutcome<Context>>),
            }
            let wake = tokio::select! {
                event = self.lease.next() => match event {
                    Some(event) => Wake::Lease(event),
                    None => return,
                },
                local = next_local_wake(&mut self.local_events) => Wake::Local(local),
                outcome = self.intake.next_outcome() => Wake::Outcome(outcome),
                _ = &mut kill => return,
            };
            match wake {
                Wake::Local(LocalWake::Event(LocalEvents::NewGroup(group_id))) => {
                    if !self.intake.absorb_local(group_id) {
                        return;
                    }
                }
                // A lagged broadcast (already warned) loses local groups
                // only; recovery is a re-subscribe, exactly as legacy.
                Wake::Local(_) => {}
                Wake::Outcome(outcome) => {
                    if !self.deliver_outcome(outcome, &mut kill).await {
                        return;
                    }
                }
                Wake::Lease(LeaseEvent::WelcomeMessages(batch)) => {
                    if !self.intake.absorb_batch(batch) {
                        return;
                    }
                }
                Wake::Lease(LeaseEvent::CatchUpComplete) => {
                    // Exact-identity dedup needs no window; nothing to flip.
                    tracing::debug!("stream router: conversation stream caught up");
                }
                Wake::Lease(LeaseEvent::TopicsLive(_)) => {}
                Wake::Lease(LeaseEvent::GroupMessages(_)) => {
                    tracing::warn!("stream router: group delivery on a welcome lease");
                }
            }
        }
    }

    /// A finished welcome/local-group task: record it known, surface the
    /// conversation. Recording first makes completions idempotent — a
    /// concurrent same-cursor flight (both spawned before either resolved)
    /// finds the cursor already known and surfaces nothing. Returns `false`
    /// when the stream must end.
    async fn deliver_outcome(
        &mut self,
        outcome: Result<WelcomeOutcome<Context>>,
        kill: &mut oneshot::Receiver<()>,
    ) -> bool {
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(e) => return send_or_kill(&self.tx, kill, Err(e)).await.is_ok(),
        };
        if let Some(seen) = outcome.seen
            && !self.intake.known.insert(seen)
        {
            return true;
        }
        match outcome.group {
            // A sync group never surfaces (only the local-group path can
            // carry one this far — the welcome path filters virtual groups
            // inside the pipeline): it is the device-sync worker's, not the
            // subscriber's.
            Some(group) if !matches!(group.conversation_type, ConversationType::Sync) => {
                send_or_kill(&self.tx, kill, Ok(group)).await.is_ok()
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// During the window an early live delivery must not swallow the replay
    /// behind it; anything at-or-below the frozen seed is skipped; after the
    /// window the position's high-water mark governs.
    #[xmtp_common::test(unwrap_try = true)]
    async fn window_dedups_by_identity_and_frozen_seed() {
        let topic = Topic::new_group_message([7u8; 16]);
        let mut seed = GlobalCursor::default();
        seed.apply(&Cursor::new(50, 0u32));
        let mut dedup = StreamDedup::syncing(
            HashMap::from([(topic.clone(), seed.clone())]),
            HashSet::new(),
        );
        let mut position = seed;

        // A sibling lease's replay of pre-seed history is never ours.
        assert!(dedup.has_seen(&position, &topic, &Cursor::new(50, 0u32)));
        assert!(dedup.has_seen(&position, &topic, &Cursor::new(3, 0u32)));

        // A live message leapfrogs the replay its own wave requested.
        let live = Cursor::new(100, 0u32);
        assert!(!dedup.has_seen(&position, &topic, &live));
        dedup.record(&topic, live);
        position.apply(&live);

        // The replay behind it still delivers during the window...
        let replayed = Cursor::new(51, 0u32);
        assert!(
            !dedup.has_seen(&position, &topic, &replayed),
            "the window must not swallow the replay behind an early live delivery"
        );
        dedup.record(&topic, replayed);
        // ...while exact duplicates are still skipped.
        assert!(dedup.has_seen(&position, &topic, &replayed));

        // Window closes: the position (already at the live edge) takes over.
        dedup.complete(std::slice::from_ref(&topic));
        assert!(dedup.has_seen(&position, &topic, &replayed));
        assert!(!dedup.has_seen(&position, &topic, &Cursor::new(101, 0u32)));
    }

    /// Windows are per topic: one lease's `CatchUpComplete` closes only its
    /// own topics' windows — a still-syncing sibling keeps its seed and the
    /// shared seen-set until the last window closes.
    #[xmtp_common::test(unwrap_try = true)]
    async fn windows_close_per_topic() {
        let early = Topic::new_group_message([1u8; 16]);
        let late = Topic::new_group_message([2u8; 16]);
        let mut seed = GlobalCursor::default();
        seed.apply(&Cursor::new(50, 0u32));
        let mut dedup = StreamDedup::syncing(
            HashMap::from([(early.clone(), GlobalCursor::default())]),
            HashSet::new(),
        );
        dedup.open_window(
            HashMap::from([(late.clone(), seed)]),
            [Cursor::new(60, 0u32)],
        );

        // Closing the early topic flips it to position dedup...
        dedup.complete(std::slice::from_ref(&early));
        let position = GlobalCursor::default();
        assert!(!dedup.has_seen(&position, &early, &Cursor::new(60, 0u32)));
        // ...while the late topic still dedups by its frozen seed + seen-set.
        assert!(dedup.has_seen(&position, &late, &Cursor::new(50, 0u32)));
        assert!(dedup.has_seen(&position, &late, &Cursor::new(60, 0u32)));
        assert!(!dedup.has_seen(&position, &late, &Cursor::new(61, 0u32)));

        // The last window closing drops the seen-set.
        dedup.complete(std::slice::from_ref(&late));
        assert!(!dedup.has_seen(&position, &late, &Cursor::new(60, 0u32)));
    }

    /// Sequence ids are not scoped per group: a stored identity in one group
    /// must fold only into that group's live position, or it swallows other
    /// groups' post-window deliveries at-or-below it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn stored_identities_fold_only_into_their_own_group() {
        let a = GroupId::from([1u8; 16]);
        let b = GroupId::from([2u8; 16]);
        let floors = HashMap::from([
            (Topic::new_group_message(a), GlobalCursor::default()),
            (Topic::new_group_message(b), GlobalCursor::default()),
        ]);

        let (positions, seen) = fold_stored(&floors, vec![(a, Cursor::new(100, 0u32))]);

        assert!(positions[&Topic::new_group_message(a)].has_seen(&Cursor::new(100, 0u32)));
        assert!(
            !positions[&Topic::new_group_message(b)].has_seen(&Cursor::new(90, 0u32)),
            "a stored identity in group A must not advance group B's position"
        );
        assert!(seen.contains(&Cursor::new(100, 0u32)));
    }

    /// The recovery-sync fallback can deliver a message whose cursor differs
    /// from the envelope's; recording both identities keeps the delivered
    /// message's own replay envelope from double-delivering it.
    #[xmtp_common::test(unwrap_try = true)]
    async fn window_records_the_delivered_identity_too() {
        let topic = Topic::new_group_message([7u8; 16]);
        let mut dedup = StreamDedup::syncing(
            HashMap::from([(topic.clone(), GlobalCursor::default())]),
            HashSet::new(),
        );
        let position = GlobalCursor::default();

        // Envelope 10 errors; recovery surfaces stored message 11.
        dedup.record(&topic, Cursor::new(10, 0u32)); // the envelope
        dedup.record(&topic, Cursor::new(11, 0u32)); // the delivered message
        assert!(
            dedup.has_seen(&position, &topic, &Cursor::new(11, 0u32)),
            "the replay envelope for the already-delivered message must be skipped"
        );
    }

    /// Recording is scoped to open windows: an identity delivered on a topic
    /// whose window is not open (closed, or never had one) is position-deduped
    /// and must not grow the seen-set kept for a sibling still syncing.
    #[xmtp_common::test(unwrap_try = true)]
    async fn record_only_tracks_topics_with_open_windows() {
        let syncing = Topic::new_group_message([1u8; 16]);
        let live = Topic::new_group_message([2u8; 16]);
        let mut dedup = StreamDedup::syncing(
            HashMap::from([(syncing.clone(), GlobalCursor::default())]),
            HashSet::new(),
        );
        let position = GlobalCursor::default();

        // A record for the window-less topic must not land in the shared
        // seen-set (observable through the still-open sibling window).
        dedup.record(&live, Cursor::new(10, 0u32));
        assert!(
            !dedup.has_seen(&position, &syncing, &Cursor::new(10, 0u32)),
            "an identity from a window-less topic must not be recorded"
        );

        // The same identity recorded under the open window is tracked.
        dedup.record(&syncing, Cursor::new(10, 0u32));
        assert!(dedup.has_seen(&position, &syncing, &Cursor::new(10, 0u32)));
    }
}
