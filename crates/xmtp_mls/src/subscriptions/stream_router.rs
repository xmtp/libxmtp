//! XIP-83 client-level stream router.
//!
//! One [`StreamRouter`] per client. Each consumer stream leases its topics
//! from the process-level [`BidiTransport`], decodes raw wire deliveries
//! through the shared pipeline seams ([`process_one`] /
//! [`process_welcome_one`]), and delivers decoded results on a bounded
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
//! Welcome dedup is a per-stream known-welcome set, consulted and updated
//! sequentially inside that stream's task — exactly the serialization
//! [`process_welcome_one`]'s contract asks for. Exact-identity sets are
//! immune to the ordering caveat, so welcome streams need no window.
//!
//! ## Backpressure (same policy as every layer below)
//!
//! A stream that stops draining its bounded channel is dropped: its channel
//! closes, its lease derefs (the transport removes last-interest topics from
//! the wire), and the consumer recovers by re-subscribing — a cursored re-add
//! from durable state. A dead wire ends every affected stream the same way;
//! transparent reconnect arrives with a later phase.

use std::collections::{HashMap, HashSet};

use tokio::sync::{mpsc, oneshot};

use xmtp_api_d14n::v3::{V3ProtoGroupMessage, V3ProtoWelcomeMessage};
use xmtp_api_d14n::{
    BidiTransport, DEFAULT_LEASE_DEPTH, LeaseEvent, TopicLease, TransportError, V3Binding,
};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::mls_v1;
use xmtp_proto::types::{Cursor, GlobalCursor, GroupId, Topic};

use super::process_message::{ProcessMessageFuture, process_one};
use super::process_welcome::process_welcome_one;
use super::{Result, SubscribeError};
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;

/// Default per-stream channel depth (chooseable per stream).
pub const DEFAULT_STREAM_DEPTH: usize = 16;

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
    cmds: mpsc::UnboundedSender<Cmd<Context>>,
}

impl<Context> Clone for StreamRouter<Context> {
    fn clone(&self) -> Self {
        Self {
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
            context,
            transport,
            consumers: HashMap::new(),
            next_id: 0,
            ends: ends_tx,
        };
        xmtp_common::spawn(None, task.run(cmds_rx, ends_rx));
        Self { cmds }
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
/// Steady state (`Live`) is the per-topic position: wire order is
/// cursor-monotonic, so a high-water mark suffices. During the catch-up
/// window (`Syncing`) it is not — a live delivery already in flight for a
/// shared topic can arrive *before* the replay this stream's cursored add
/// requested (see the transport's delivery-order contract) — so the window
/// checks the **frozen subscribe-time seed** (anything at-or-below the
/// durable resume point is never for this stream, including a sibling
/// lease's replay of older history) plus an exact-identity seen-set. The set
/// is bounded by the window: it drops at `CatchUpComplete`, when the live
/// positions (which kept advancing) take over.
enum StreamDedup {
    Syncing {
        /// Per-topic durable positions frozen at subscribe.
        seeds: HashMap<Topic, GlobalCursor>,
        /// Exact identities delivered during the window.
        seen: HashSet<Cursor>,
    },
    Live,
}

impl StreamDedup {
    /// `seen` starts with the identities already stored locally (delivered by
    /// an earlier stream or a sync) — the durable cursor alone is not a
    /// delivery floor, because the streaming pipeline stores without
    /// advancing it.
    fn syncing(seeds: HashMap<Topic, GlobalCursor>, seen: HashSet<Cursor>) -> Self {
        Self::Syncing { seeds, seen }
    }

    fn has_seen(&self, position: &GlobalCursor, topic: &Topic, cursor: &Cursor) -> bool {
        match self {
            Self::Syncing { seeds, seen } => {
                seeds.get(topic).is_some_and(|seed| seed.has_seen(cursor)) || seen.contains(cursor)
            }
            Self::Live => position.has_seen(cursor),
        }
    }

    fn record(&mut self, cursor: Cursor) {
        if let Self::Syncing { seen, .. } = self {
            seen.insert(cursor);
        }
    }

    fn complete(&mut self) {
        *self = Self::Live;
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
        // Seed every group from its durable cursor; the cursored lease makes
        // the server replay anything past it (catch-up == subscribe).
        let seeds = self
            .context
            .db()
            .get_last_cursor_for_ids(
                &group_ids,
                &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
            )
            .map_err(SubscribeError::from)?;

        // The streaming pipeline stores messages WITHOUT advancing the durable
        // cursor (`allow_cursor_increment=false`), so the cursor alone is not a
        // delivery floor: everything streamed since the last full sync is
        // stored but still above it. Those exact identities seed the window's
        // seen-set (so the server's replay of them is skipped) and fold into
        // the live positions (so they stay skipped after the window). The wire
        // cursor and the window floor stay at the durable cursor — a stored
        // gap (6 and 8 stored, 7 missed) still gets 7 replayed and delivered.
        let stored = self
            .context
            .db()
            .messages_newer_than(&seeds)
            .map_err(SubscribeError::from)?;

        let mut subs = Vec::with_capacity(group_ids.len());
        let mut floors = HashMap::with_capacity(group_ids.len());
        for group_id in &group_ids {
            let position = seeds.get(group_id.as_slice()).cloned().unwrap_or_default();
            let topic = Topic::new_group_message(*group_id);
            subs.push((topic.clone(), position.max()));
            floors.insert(topic, position);
        }
        let (positions, seen) = fold_stored(&floors, stored);

        let lease = self.transport.lease(subs, DEFAULT_LEASE_DEPTH).await?;
        let (tx, items) = mpsc::channel(depth.max(1));
        let consumer = MessageConsumer {
            factory: ProcessMessageFuture::new(self.context.clone()),
            lease,
            tx,
            dedup: StreamDedup::syncing(floors, seen),
            positions,
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
        let db = self.context.db();
        let installation = self.context.installation_id();
        // Wire resume point: the durable welcome cursor — every welcome
        // *processed* (stored, ignored, or filtered) advances it, so nothing
        // already handled is replayed.
        let seed = db
            .get_last_cursor_for_ids(&[installation], &[EntityKind::Welcome])
            .map_err(SubscribeError::from)?
            .get(installation.as_slice())
            .cloned()
            .unwrap_or_default()
            .v3_welcome();
        // Classification input for the pipeline: the welcome ids that became
        // groups. Per-stream — each stream's dedup is its own, so one
        // stream's delivery never suppresses another's (they may hold
        // different filters).
        let known: HashSet<Cursor> =
            HashSet::from_iter(db.group_cursors().map_err(SubscribeError::from)?);

        let topic = Topic::new_welcome_message(installation);
        let lease = self
            .transport
            .lease(vec![(topic, seed)], DEFAULT_LEASE_DEPTH)
            .await?;
        let (tx, items) = mpsc::channel(depth.max(1));
        let consumer = WelcomeConsumer {
            context: self.context.clone(),
            lease,
            tx,
            known,
            conversation_type,
            include_duplicate_dms,
            consent_states,
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

/// One message stream: owns its lease, decodes sequentially, delivers on its
/// bounded channel. A stall here (e.g. recovery sync) backs up only this
/// stream's lease.
struct MessageConsumer<Context> {
    factory: ProcessMessageFuture<Context>,
    lease: TopicLease<V3Binding>,
    tx: mpsc::Sender<Result<StoredGroupMessage>>,
    /// Live per-topic positions — steady-state dedup; they keep advancing
    /// during the window so they hold the live edge when it closes.
    positions: HashMap<Topic, GlobalCursor>,
    dedup: StreamDedup,
}

impl<Context> MessageConsumer<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(mut self, mut kill: oneshot::Receiver<()>) {
        loop {
            let event = tokio::select! {
                event = self.lease.next() => match event {
                    Some(event) => event,
                    // Wire death or transport backpressure drop: the stream
                    // ends (tx drops); the consumer re-subscribes.
                    None => return,
                },
                _ = &mut kill => return,
            };
            match event {
                LeaseEvent::GroupMessages(batch) => {
                    if !self.deliver_batch(batch, &mut kill).await {
                        return;
                    }
                }
                LeaseEvent::CatchUpComplete => {
                    tracing::debug!("stream router: message stream caught up");
                    self.dedup.complete();
                }
                LeaseEvent::TopicsLive(topics) => {
                    tracing::debug!(?topics, "stream router: topics live");
                }
                LeaseEvent::WelcomeMessages(_) => {
                    tracing::warn!("stream router: welcome delivery on a message lease");
                }
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
                    self.dedup.record(cursor);
                    match processed.message {
                        Some(message) => {
                            // The pipeline may surface a different message
                            // than the envelope named (recovery sync stores
                            // ahead) — record the delivered identity too, or
                            // its own replay envelope would deliver it twice.
                            self.dedup.record(Cursor::new(
                                message.sequence_id as u64,
                                message.originator_id as u32,
                            ));
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
/// Locally-created conversations are not merged in yet: this consumer only
/// speaks the welcome topic, so a client does not see its own `create_group`
/// on this stream. That parity arrives with the auto-subscribe reflex work,
/// which fans local new-group events into both stream kinds.
struct WelcomeConsumer<Context> {
    context: Context,
    lease: TopicLease<V3Binding>,
    tx: mpsc::Sender<Result<MlsGroup<Context>>>,
    known: HashSet<Cursor>,
    conversation_type: Option<ConversationType>,
    include_duplicate_dms: bool,
    consent_states: Option<Vec<ConsentState>>,
}

impl<Context> WelcomeConsumer<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(mut self, mut kill: oneshot::Receiver<()>) {
        loop {
            let event = tokio::select! {
                event = self.lease.next() => match event {
                    Some(event) => event,
                    None => return,
                },
                _ = &mut kill => return,
            };
            match event {
                LeaseEvent::WelcomeMessages(batch) => {
                    if !self.deliver_batch(batch, &mut kill).await {
                        return;
                    }
                }
                LeaseEvent::CatchUpComplete => {
                    // Exact-identity dedup needs no window; nothing to flip.
                    tracing::debug!("stream router: conversation stream caught up");
                }
                LeaseEvent::TopicsLive(_) => {}
                LeaseEvent::GroupMessages(_) => {
                    tracing::warn!("stream router: group delivery on a welcome lease");
                }
            }
        }
    }

    async fn deliver_batch(
        &mut self,
        batch: Vec<mls_v1::WelcomeMessage>,
        kill: &mut oneshot::Receiver<()>,
    ) -> bool {
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
            if self.known.contains(&typed.cursor) {
                // Already a group before subscribe, or already delivered by
                // this stream. This must NOT fall through to
                // `process_welcome_one` — its known-id path re-surfaces the
                // group from store, which would re-emit the conversation every
                // time a sibling's cursored re-add replays this welcome.
                continue;
            }

            // Sequential against this stream's own set — exactly the
            // serialization `process_welcome_one`'s contract requires. Races
            // the kill so dropping the stream releases the lease promptly
            // even mid-processing.
            let processed = tokio::select! {
                processed = process_welcome_one(
                    self.context.clone(),
                    &self.known,
                    typed,
                    self.conversation_type,
                    self.include_duplicate_dms,
                    self.consent_states.clone(),
                ) => processed,
                _ = &mut *kill => return false,
            };
            let delivered = match processed {
                Ok(outcome) => {
                    if let Some(seen) = outcome.seen {
                        self.known.insert(seen);
                    }
                    match outcome.group {
                        Some(group) => send_or_kill(&self.tx, kill, Ok(group)).await,
                        None => continue,
                    }
                }
                Err(e) => send_or_kill(&self.tx, kill, Err(e)).await,
            };
            if delivered.is_err() {
                return false;
            }
        }
        true
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
        dedup.record(live);
        position.apply(&live);

        // The replay behind it still delivers during the window...
        let replayed = Cursor::new(51, 0u32);
        assert!(
            !dedup.has_seen(&position, &topic, &replayed),
            "the window must not swallow the replay behind an early live delivery"
        );
        dedup.record(replayed);
        // ...while exact duplicates are still skipped.
        assert!(dedup.has_seen(&position, &topic, &replayed));

        // Window closes: the position (already at the live edge) takes over.
        dedup.complete();
        assert!(dedup.has_seen(&position, &topic, &replayed));
        assert!(!dedup.has_seen(&position, &topic, &Cursor::new(101, 0u32)));
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
        dedup.record(Cursor::new(10, 0u32)); // the envelope
        dedup.record(Cursor::new(11, 0u32)); // the delivered message
        assert!(
            dedup.has_seen(&position, &topic, &Cursor::new(11, 0u32)),
            "the replay envelope for the already-delivered message must be skipped"
        );
    }
}
