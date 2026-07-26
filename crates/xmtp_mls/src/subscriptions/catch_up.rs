//! One-shot bounded catch-up over the XIP-83 bidi wire.
//!
//! [`Client::catch_up_to_live`] brings the local store current with the
//! server — every pending welcome joined, every leased-topic message replayed
//! and processed — and then stops, leaving nothing running. XIP-83 bounded
//! sync: one `Subscribe` stream carrying a `history_only` Mutate for every
//! topic this client owns (each from its durable cursor) — chunked to the
//! shared per-`Mutate` frame limit, so a large account opens with a bounded
//! first frame and the rest follow as their own waves — a half-close once
//! everything owed has arrived, and a server-side close in reply. The shape
//! for "sync me, then let the process sleep" callers — an agent priming
//! before it serves, a mobile background fetch — where a live stream would be
//! wasted.
//!
//! ## Its own wire, not the shared transport
//!
//! The process-shared [`BidiTransport`](xmtp_api_d14n::BidiTransport) is a
//! live-delivery machine: its ledger assumes every wave crosses to live
//! delivery, keeps wire positions for reconnects, and holds the wire open for
//! its leases. A `history_only` wave never registers for live delivery, so it
//! has no place in that bookkeeping — and a bounded run has no reconnect
//! state worth keeping. Each call opens its own [`BidiConnection`], which
//! also means a concurrently-running live stream is untouched: both paths
//! store through the same pipeline, whose DB fast-path makes double delivery
//! a cheap lookup, and each keeps its own delivery dedup. For the same
//! reason this call deliberately ignores the stream-suspend intent (the
//! app-lifecycle suspend/resume pair): a backgrounded app calling it IS the
//! background fetch, and the bounded wire closes when the run does.
//!
//! ## The discovery loop
//!
//! Welcomes can join groups whose own history is then owed too. Processing a
//! welcome that becomes a group issues a follow-up `history_only` Mutate for
//! that group's topic on the same stream (joins run their own group sync, so
//! the follow-up wave usually replays nothing — it exists to close the gap
//! between the join's sync and this run's live edge). The half-close waits
//! until no wave is outstanding and no welcome is still processing, so
//! chained discoveries extend the run instead of escaping it.
//!
//! Two things deliberately do NOT extend the run: groups created locally
//! mid-run (their creator already holds their state; nothing is owed from
//! the server) and messages published after a topic's wave was frozen (the
//! next call — or a live stream — picks them up; "live" here means the
//! server's freeze point per wave).
//!
//! ## Durable cursors
//!
//! Group messages are processed exactly as the streaming pipeline does —
//! stored without advancing the durable message cursor — so a failed message
//! is never skipped past and a later query-path sync retries it. The cost is
//! symmetric too: a repeat call re-requests the tail since the last durable
//! advance and drops the already-stored copies at the seed's seen-set.
//! Welcomes are the same on this path: they are processed with the cursor
//! increment OFF (`process_new_welcome(.., false, ..)`), so the streamed and
//! catch-up arms do NOT advance the durable welcome cursor either — a
//! re-processed welcome is dropped by group-existence dedup (the seed's
//! `known_welcomes_above`), not by the cursor. Only the legacy full-sync
//! fallback advances the welcome cursor.
//!
//! ## Fallback
//!
//! Dispatch mirrors the stream entry points: the bidi path when
//! [`bidi_streams_active`], the legacy full sync otherwise. A backend that
//! refuses the bidi surface latches its destination onto legacy (same
//! per-destination latch the streams use) and this call completes on the
//! legacy path — the caller never sees the refusal.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use tracing::Instrument;
use xmtp_api_d14n::v3::V3ProtoGroupMessage;
use xmtp_api_d14n::{BidiConnection, BidiEvent, OpenError, TryMutateError, chunk_mutate_adds};
use xmtp_common::{ErrorCode, RetryableError, retryable};
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::subscribe_request::v1::{Mutate, mutate::Subscription};
use xmtp_proto::types::{Cursor, GroupId, SequenceId, Topic};

use super::process_message::{ProcessMessageFuture, process_one};
use super::router_callbacks::{
    bidi_streams_active, latch_bidi_unsupported, open_is_backend_refusal,
};
use super::stream_router::{WelcomeIntake, known_welcomes_above, seed_groups, welcome_seed};
use super::{SubscribeError, SyncWorkerEvent};
use crate::Client;
use crate::context::XmtpSharedContext;
use crate::groups::welcome_sync::WelcomeService;

/// Wire deaths tolerated before giving up. Each retry reseeds from durable
/// state, so progress made before a death is never repeated over the wire
/// beyond the (deduped) tail since the last durable advance.
const MAX_ATTEMPTS: u32 = 3;

/// Base backoff between attempts (scaled by the attempt number).
const RETRY_BACKOFF: Duration = Duration::from_millis(500);

/// How long to wait for the server's close after the half-close. By this
/// point every wave has completed — the drain is courtesy, not correctness —
/// so a peer that never closes only costs this much patience.
const POST_FINISH_DRAIN: Duration = Duration::from_secs(5);

/// What one [`Client::catch_up_to_live`] call brought home — counts of what
/// it PERSISTED, not what the wire replayed: retries reseed from durable
/// state and replay, and already-stored history is deduped before counting,
/// so these are "new since the last call". This is the mobile host's "did
/// anything arrive" signal (notification content, background-refresh
/// `newData`, up-to-date UI).
///
/// Device-sync (virtual) groups are excluded from both counts: their
/// traffic is internal plumbing that never surfaces as a conversation or
/// message.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CatchUpSummary {
    /// Application messages newly persisted by this call.
    pub messages: u64,
    /// Conversations newly joined by this call.
    pub conversations: u64,
    /// Whether the run reached the live edge before its optional deadline.
    /// `false` means a `timeout` cut it short; the counts above are then the
    /// partial total persisted so far (bidi path) or zero (legacy fallback),
    /// and a later call resumes from durable state.
    pub completed: bool,
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum CatchUpError {
    /// Seeding, processing, or the legacy fallback failed.
    #[error(transparent)]
    #[error_code(inherit)]
    Subscribe(#[from] SubscribeError),
    /// Bidi unsupported.
    ///
    /// The backend refuses the bidi surface — the latch-worthy verdict
    /// ([`open_is_backend_refusal`]). The dispatch layer falls back to the
    /// legacy sync; this never escapes [`Client::catch_up_to_live`]. Not
    /// retryable.
    #[error("the backend does not support bidi streams: {0}")]
    Unsupported(OpenError),
    /// Catch-up stream could not open.
    ///
    /// A wire open no redial can fix, without a capability verdict. The
    /// dispatch layer serves the call on the legacy sync path. Not
    /// retryable.
    #[error("the catch-up stream could not open: {0}")]
    DeadEnd(OpenError),
    /// Catch-up did not complete.
    ///
    /// The wire kept dying before catch-up completed. Everything processed
    /// before each death is kept; calling again resumes from durable state.
    /// Retryable.
    #[error("catch-up did not complete within {attempts} attempts")]
    Exhausted { attempts: u32 },
}

impl RetryableError for CatchUpError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Subscribe(e) => retryable!(e),
            // The backend verdict is latched process-wide, and a dead-end
            // open is unretryable by definition — redialing changes neither.
            Self::Unsupported(_) | Self::DeadEnd(_) => false,
            // Wire deaths are transient; a fresh call resumes from durable
            // state.
            Self::Exhausted { .. } => true,
        }
    }
}

/// How one bounded run over the wire ended.
enum AttemptEnd {
    /// Every wave completed and every welcome resolved — the store is
    /// current to each topic's freeze point.
    Complete,
    /// The wire died (or the welcome backlog overflowed) mid-run; retry
    /// from durable state.
    WireDied,
}

/// The one `history_only` wave shape this module sends: cursored adds, no
/// removes, bounded catch-up only (XIP-83 bounded sync).
fn history_only_mutate(adds: Vec<(Topic, SequenceId)>, mutate_id: u64) -> Mutate {
    Mutate {
        adds: adds
            .into_iter()
            .map(|(topic, cursor)| Subscription {
                topic: topic.to_bytes().into_vec(),
                id_cursor: cursor,
            })
            .collect(),
        removes: vec![],
        history_only: true,
        mutate_id,
    }
}

/// Plan the opening waves for a bounded run: the whole subscription set,
/// chunked to the shared `Mutate` frame limit so a client with very many
/// conversations never opens with one oversized `history_only` frame. The
/// first chunk seeds the open (wave `1`); each remaining chunk is a follow-up
/// wave (ids `2..`) queued below and completed like a discovery add. Wave ids
/// are contiguous from `1`, so the caller derives the outstanding set and the
/// next free id from the overflow count alone.
fn plan_catch_up_waves(subs: Vec<(Topic, SequenceId)>) -> (Mutate, Vec<Mutate>) {
    // `subs` always carries at least the welcome topic, so there is always a
    // first chunk; `unwrap_or_default` only guards the impossible empty case.
    let mut chunks = chunk_mutate_adds(subs).into_iter();
    let open = history_only_mutate(chunks.next().unwrap_or_default(), 1);
    let overflow = chunks
        .enumerate()
        .map(|(i, chunk)| history_only_mutate(chunk, i as u64 + 2))
        .collect();
    (open, overflow)
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
    Context::ApiClient: XmtpMlsBidiStreams + Clone + Send + Sync + 'static,
    <Context::ApiClient as XmtpMlsBidiStreams>::SubscribeStream: 'static,
{
    /// Bring the local store current with the server, then stop: pending
    /// welcomes joined, every conversation's messages replayed from its
    /// durable cursor and processed, nothing left running afterwards. See
    /// the module docs for the wire shape and its bounds.
    ///
    /// `timeout` bounds the whole call (`None` = run to completion, bounded only
    /// by the wire-death retry cap). On the deadline the returned summary is the
    /// partial persisted so far with `completed == false` (bidi path; the legacy
    /// fallback reports zero), and a later call resumes from durable state.
    ///
    /// Individual message- and welcome-processing failures are logged, not
    /// fatal — like a live stream, the durable cursors are not advanced
    /// past them (a failed welcome stays unrecorded), so the next call, a
    /// welcome stream, or a sync retries exactly the failed items. The
    /// pipelines already retry transient errors internally; what fails here
    /// is data-dependent, and failing the whole call over it would turn
    /// "everything else synced, one item pending replay" into a hard error
    /// on every wake. `Ok` therefore means "everything owed was received
    /// and attempted", not "zero processing errors".
    #[xmtp_common::span(prefix = "stream")]
    pub async fn catch_up_to_live(
        &self,
        timeout: Option<Duration>,
    ) -> Result<CatchUpSummary, CatchUpError> {
        // `checked_add` so an absurd `timeout` (it arrives as a u64 millisecond
        // count over the FFI) degrades to "no deadline" instead of panicking on
        // the Instant overflow — a caller asking to wait ~forever gets exactly
        // that.
        let deadline = timeout.and_then(|d| tokio::time::Instant::now().checked_add(d));
        let host = self.context.api().api_client.host().to_owned();
        if bidi_streams_active(&host) {
            match self.catch_up_bidi(deadline).await {
                Ok(summary) => return Ok(summary),
                Err(CatchUpError::Unsupported(e)) => {
                    tracing::error!(
                        "bidi catch-up refused by the backend, \
                         latching {host} onto the legacy streams: {e}"
                    );
                    latch_bidi_unsupported(&host);
                }
                Err(CatchUpError::DeadEnd(e)) => {
                    tracing::warn!(
                        "bidi catch-up open failed unretryably, \
                         serving this call on the legacy path: {e}"
                    );
                }
                Err(e) => return Err(e),
            }
        }
        // The legacy arm computes a terminal store diff, so it has no partial to
        // hand back mid-flight: on the deadline return an empty, `completed=false`
        // summary (its stores are still persisted; a later call resumes them).
        match deadline {
            Some(dl) => match tokio::time::timeout_at(dl, self.catch_up_legacy()).await {
                Ok(res) => res,
                Err(_) => Ok(CatchUpSummary::default()),
            },
            None => self.catch_up_legacy().await,
        }
    }

    /// The bidi arm: bounded runs with a bounded retry on wire death. The
    /// summary accumulates across retries — an attempt's stores survive its
    /// wire death, and the next attempt reseeds from durable state, so its
    /// replays of them are deduped and never recounted. On the `deadline` the
    /// run stops and returns the summary accumulated so far with
    /// `completed = false` — its `&mut summary` is owned here, so the counts
    /// earned before the cut survive the cancelled attempt.
    pub(crate) async fn catch_up_bidi(
        &self,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CatchUpSummary, CatchUpError> {
        let mut summary = CatchUpSummary::default();
        for attempt in 1..=MAX_ATTEMPTS {
            let end = match deadline {
                Some(dl) => {
                    match tokio::time::timeout_at(dl, self.catch_up_attempt(&mut summary)).await {
                        Ok(res) => res?,
                        // Deadline hit mid-attempt: `summary` holds everything
                        // processed so far (`completed` stays false).
                        Err(_) => return Ok(summary),
                    }
                }
                None => self.catch_up_attempt(&mut summary).await?,
            };
            match end {
                AttemptEnd::Complete => {
                    summary.completed = true;
                    return Ok(summary);
                }
                AttemptEnd::WireDied => {
                    tracing::warn!(attempt, "catch-up wire died; retrying from durable state");
                    let backoff = RETRY_BACKOFF * attempt;
                    // Don't sleep past the deadline; return the partial instead.
                    // `checked_add` mirrors the deadline computation above: the
                    // bounded backoff can't actually overflow, but if it ever
                    // did we'd treat it as "past the deadline" and stop rather
                    // than sleep for an unrepresentable duration.
                    let wake = tokio::time::Instant::now().checked_add(backoff);
                    if matches!(deadline, Some(dl) if wake.is_none_or(|w| w >= dl)) {
                        return Ok(summary);
                    }
                    tokio::time::sleep(backoff).await;
                }
            }
        }
        Err(CatchUpError::Exhausted {
            attempts: MAX_ATTEMPTS,
        })
    }

    /// The legacy arm: full welcome + group sync. The legacy sync reports
    /// groups synced, not items persisted, so the summary is a before/after
    /// diff of the store: identities above the PRE-call durable cursors,
    /// minus those already stored when the call began. (The query path
    /// advances cursors as it stores, so post-call cursors cannot serve as
    /// the diff floor.)
    async fn catch_up_legacy(&self) -> Result<CatchUpSummary, CatchUpError> {
        let db = self.context.db();
        let query = || GroupQueryArgs {
            include_duplicate_dms: true,
            include_sync_groups: true,
            ..Default::default()
        };
        let pre_groups = db
            .find_groups(&query())
            .await
            .map_err(SubscribeError::from)?;
        let pre_ids: HashSet<GroupId> = pre_groups.iter().map(|g| g.id).collect();
        let group_ids: Vec<GroupId> = pre_ids.iter().copied().collect();
        let mut cursors = db
            .get_last_cursor_for_ids(
                &group_ids,
                &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
            )
            .await
            .map_err(SubscribeError::from)?;
        let pre_stored: HashSet<Cursor> = db
            .messages_newer_than(&cursors)
            .await
            .map_err(SubscribeError::from)?
            .into_iter()
            .map(|(_, cursor)| cursor)
            .collect();

        WelcomeService::new(&self.context)
            .sync_all_welcomes_and_groups(None)
            .await
            .map_err(|e| CatchUpError::from(SubscribeError::from(Box::new(e))))?;

        let post_groups = db
            .find_groups(&query())
            .await
            .map_err(SubscribeError::from)?;
        let mut summary = CatchUpSummary::default();
        let mut sync_ids: HashSet<GroupId> = HashSet::new();
        for group in &post_groups {
            if matches!(group.conversation_type, ConversationType::Sync) {
                sync_ids.insert(group.id);
            }
            if !pre_ids.contains(&group.id) {
                // A group joined during the call: everything stored for it
                // is new.
                cursors.insert(group.id.as_slice().to_vec(), Default::default());
                if !matches!(group.conversation_type, ConversationType::Sync) {
                    summary.conversations += 1;
                }
            }
        }
        summary.messages = db
            .messages_newer_than(&cursors)
            .await
            .map_err(SubscribeError::from)?
            .into_iter()
            .filter(|(group_id, cursor)| {
                !sync_ids.contains(group_id) && !pre_stored.contains(cursor)
            })
            .count() as u64;
        summary.completed = true;
        Ok(summary)
    }

    /// One bounded run: subscribe everything `history_only`, process what
    /// arrives (welcomes may add follow-up waves), half-close once nothing
    /// is owed, and let the server close. Newly persisted items count into
    /// `summary`; replays of already-stored history do not (the seeded
    /// seen-set and the intake's known/floor guards drop them first).
    async fn catch_up_attempt(
        &self,
        summary: &mut CatchUpSummary,
    ) -> Result<AttemptEnd, CatchUpError> {
        let db = self.context.db();
        let installation = self.context.installation_id();
        // Welcome floor and known set BEFORE the group query, exactly as the
        // all-messages subscribe orders them: a welcome processed in between
        // shows up in the query result AND above the floor, and the tracked
        // set absorbs the overlap; the reverse order would leave it in
        // neither.
        let welcome_floor = welcome_seed(&db, installation).await?;
        let known = known_welcomes_above(&db, welcome_floor).await?;
        let groups = db
            .find_groups(&GroupQueryArgs {
                include_duplicate_dms: true,
                // Sync groups are caught up too — their replayed traffic
                // nudges the device-sync worker, like the streams do.
                include_sync_groups: true,
                ..Default::default()
            })
            .await
            .map_err(SubscribeError::from)?;
        let mut sync_groups: HashSet<GroupId> = groups
            .iter()
            .filter(|g| matches!(g.conversation_type, ConversationType::Sync))
            .map(|g| g.id)
            .collect();
        let group_ids: Vec<GroupId> = groups.into_iter().map(|g| g.id).collect();
        let seeds = seed_groups(&db, &group_ids).await?;

        let mut tracked: HashSet<Topic> = seeds.floors.keys().cloned().collect();
        let mut subs = seeds.subs();
        subs.push((Topic::new_welcome_message(installation), welcome_floor));
        // Already-stored identities above the floors: replays of these are
        // skipped without touching the pipeline (stored by an earlier stream
        // or sync — the durable cursor alone is not a delivery floor).
        let mut seen = seeds.seen;

        let api = self.context.api().api_client.clone();
        // Defensive: `subs` always carries the welcome topic (pushed above), so
        // it is never empty — but an empty set would seed an adds-nothing open
        // frame that never earns a `CatchUpComplete`, stalling the run until the
        // watchdog fires. Nothing owed means nothing to catch up.
        if subs.is_empty() {
            return Ok(AttemptEnd::Complete);
        }
        // Chunk the subscription set so a very large account never opens with
        // one oversized frame; the overflow chunks ride the follow-up queue
        // below as their own waves.
        let (open, overflow) = plan_catch_up_waves(subs);
        let overflow_waves = overflow.len() as u64;
        let mut conn = match BidiConnection::open(&api, open).await {
            Ok(conn) => conn,
            Err(e) => {
                let open = OpenError::new(e);
                return if open_is_backend_refusal(&open) {
                    Err(CatchUpError::Unsupported(open))
                } else if open.is_retryable() {
                    // A transient dial failure consumes an attempt.
                    Ok(AttemptEnd::WireDied)
                } else {
                    Err(CatchUpError::DeadEnd(open))
                };
            }
        };

        let factory = ProcessMessageFuture::new(self.context.clone());
        let mut intake =
            WelcomeIntake::new(self.context.clone(), welcome_floor, known, None, true, None);
        // Waves whose CatchUpComplete is still owed (including queued,
        // not-yet-sent follow-ups) — `1` is the initial wave, `2..=1+overflow`
        // its chunked remainder (all pre-queued below).
        let mut outstanding: HashSet<u64> = (1..=1 + overflow_waves).collect();
        let mut next_mutate_id: u64 = 2 + overflow_waves;
        // Follow-up waves awaiting a command slot. `try_mutate`, never
        // `mutate`: this task is the sole event drainer, and parking on the
        // command channel while events back up is the documented deadlock.
        // Seeded with the initial set's overflow chunks; discovery appends more.
        let mut queued: VecDeque<Mutate> = overflow.into_iter().collect();

        loop {
            while let Some(wave) = queued.pop_front() {
                match conn.try_mutate(wave) {
                    Ok(()) => {}
                    Err(TryMutateError::Full(wave)) => {
                        // Retried next turn; replay frames keep arriving (or
                        // the watchdog ends the wire and the bounded attempt
                        // retry takes over), so the loop keeps turning —
                        // drainage is bounded by wire liveness, not by luck.
                        tracing::debug!(
                            queued_waves = queued.len() + 1,
                            "catch-up follow-up wave deferred on a full command slot"
                        );
                        queued.push_front(wave);
                        break;
                    }
                    Err(TryMutateError::Closed(_)) => return Ok(AttemptEnd::WireDied),
                }
            }
            if outstanding.is_empty() && queued.is_empty() && intake.is_idle() {
                break;
            }
            tokio::select! {
                event = conn.next() => match event {
                    None => return Ok(AttemptEnd::WireDied),
                    Some(BidiEvent::GroupMessages { messages, .. }) => {
                        self.process_group_batch(&factory, messages, &mut seen, &sync_groups, summary)
                            .await;
                    }
                    Some(BidiEvent::WelcomeMessages { messages, .. }) => {
                        if !intake.absorb_batch(messages) {
                            tracing::warn!(
                                outstanding_waves = outstanding.len(),
                                queued_waves = queued.len(),
                                "catch-up welcome backlog overflowed; \
                                 ending the attempt to retry from durable state"
                            );
                            return Ok(AttemptEnd::WireDied);
                        }
                    }
                    Some(BidiEvent::CatchUpComplete { mutate_id }) => {
                        outstanding.remove(&mutate_id);
                    }
                    // No wave of ours registers for live delivery, so no
                    // topic ever crosses; tolerate the marker anyway.
                    Some(BidiEvent::Started { .. }) | Some(BidiEvent::TopicsLive { .. }) => {}
                },
                outcome = intake.next_outcome() => match outcome {
                    Ok(outcome) => {
                        if let Some(group) = outcome.group {
                            if matches!(group.conversation_type, ConversationType::Sync) {
                                sync_groups.insert(group.group_id);
                            }
                            let topic = Topic::new_group_message(group.group_id);
                            if !tracked.contains(&topic) {
                                if !matches!(group.conversation_type, ConversationType::Sync) {
                                    summary.conversations += 1;
                                }
                                let gseeds = seed_groups(&db, &[group.group_id]).await?;
                                let adds = gseeds.subs();
                                seen.extend(gseeds.seen);
                                tracked.insert(topic);
                                let mutate_id = next_mutate_id;
                                next_mutate_id += 1;
                                outstanding.insert(mutate_id);
                                queued.push_back(history_only_mutate(adds, mutate_id));
                            }
                        }
                        if let Some(cursor) = outcome.seen {
                            intake.known.insert(cursor);
                        }
                    }
                    // The welcome stays unrecorded, so it replays on the
                    // next call (or any welcome stream/sync) — same recovery
                    // as a live stream surfacing the error.
                    Err(e) => tracing::warn!("catch-up welcome processing failed: {e}"),
                },
            }
        }

        // Bounded-sync half-close (XIP-83): we are done sending; the server
        // finishes and closes its side. Everything owed has already arrived
        // and been attempted, so from here the run is complete no matter how
        // gracefully the wire goes down. Time-box the whole close — the
        // half-close send AND the drain: `finish()` blocks on the connection's
        // command channel, which a wedged actor (parked emitting into a full
        // event channel) could hold forever, so the courtesy close must never
        // outlast the run (mirrors the transport's `close_gracefully` budget).
        let close = async {
            if conn.finish().await.is_ok() {
                while conn.next().await.is_some() {}
            }
        };
        if tokio::time::timeout(POST_FINISH_DRAIN, close)
            .await
            .is_err()
        {
            tracing::debug!("catch-up half-close did not settle in time; dropping the wire");
        }
        Ok(AttemptEnd::Complete)
    }

    /// Decode and process one replay batch through the streaming pipeline.
    /// Failures are logged and skipped — the durable cursor never advances
    /// past them, so a later sync retries (module docs).
    async fn process_group_batch(
        &self,
        factory: &ProcessMessageFuture<Context>,
        batch: Vec<xmtp_proto::mls_v1::GroupMessage>,
        seen: &mut HashSet<Cursor>,
        sync_groups: &HashSet<GroupId>,
        summary: &mut CatchUpSummary,
    ) {
        let batch_started = std::time::Instant::now();
        let batch_size = batch.len();
        for proto in batch {
            let typed =
                match xmtp_proto::types::GroupMessage::try_from(V3ProtoGroupMessage::from(proto)) {
                    Ok(typed) => typed,
                    Err(e) => {
                        tracing::warn!("catch-up skipping undecodable group message: {e}");
                        continue;
                    }
                };
            if !seen.insert(typed.cursor) {
                continue;
            }
            let (topic, cursor) = (Topic::new_group_message(typed.group_id), typed.cursor);
            let started = std::time::Instant::now();
            let result = process_one(factory, typed)
                .instrument(tracing::debug_span!("process_envelope", %topic, ?cursor))
                .await;
            tracing::trace!(
                %topic,
                ?cursor,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "catch-up: envelope processed"
            );
            match result {
                Ok(processed) => {
                    if let Some(message) = processed.message {
                        // The pipeline may store ahead of the envelope it was
                        // handed (recovery sync) — record the surfaced
                        // identity too, so its own replay frame is skipped.
                        seen.insert(Cursor::new(
                            message.sequence_id as u64,
                            message.originator_id as u32,
                        ));
                        if sync_groups.contains(&message.group_id) {
                            let _ = self
                                .context
                                .worker_events()
                                .send(SyncWorkerEvent::NewSyncGroupMsg);
                        } else {
                            // Surfacing past the seeded seen-set means this
                            // identity was not stored when the call began —
                            // a newly persisted message.
                            summary.messages += 1;
                        }
                    }
                }
                // The identity is in the log so a message that fails every
                // catch-up (a poison message pinning its durable cursor) is
                // traceable across runs.
                Err(e) => tracing::warn!(
                    %topic,
                    ?cursor,
                    "catch-up message processing failed (durable cursor held; a later sync retries): {e}"
                ),
            }
        }
        tracing::debug!(
            batch = batch_size,
            elapsed_ms = batch_started.elapsed().as_millis() as u64,
            "catch-up: message batch processed"
        );
    }
}

/// Live integration tests over a real v3 backend (docker node), like
/// `bidi_tests` — native-only, v3-only.
#[cfg(all(test, not(feature = "d14n")))]
mod tests {
    use super::CatchUpSummary;
    use crate::tester;
    use crate::utils::MlsGroupExt;
    use xmtp_db::group_message::MsgQueryArgs;

    /// A client that has never streamed or synced catches up: the pending
    /// welcome joins the group (discovery adds a follow-up wave on the same
    /// stream) and the group's history lands in the store.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_joins_pending_groups_and_stores_history() {
        tester!(alix);
        tester!(bo);

        let group = bo.create_group(None, None).await?;
        group.invite(&alix).await?;
        group.send_msg(b"while you were out").await;
        group.send_msg(b"still out").await;

        let summary = alix.catch_up_bidi(None).await?;

        let alix_group = alix.group(&group.group_id).await?;
        let bodies: Vec<Vec<u8>> = alix_group
            .find_messages(&MsgQueryArgs::default())
            .await?
            .into_iter()
            .map(|m| m.decrypted_message_bytes)
            .collect();
        assert!(bodies.contains(&b"while you were out".to_vec()));
        assert!(bodies.contains(&b"still out".to_vec()));
        assert_eq!(summary.conversations, 1, "the joined group must be counted");
        assert!(
            summary.messages >= 2,
            "the stored history must be counted (got {})",
            summary.messages
        );
    }

    /// A member with a stale durable cursor gets the missed tail replayed
    /// and stored — and a second run finds nothing new to do.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_replays_the_missed_tail_idempotently() {
        tester!(alix);
        tester!(bo);

        let group = alix.create_group(None, None).await?;
        group.invite(&bo).await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&group.group_id).await?;
        bo_group.sync().await?; // durable cursor at "now"

        group.send_msg(b"missed one").await;
        group.send_msg(b"missed two").await;

        let first = bo.catch_up_bidi(None).await?;
        let count_after_first = bo_group
            .find_messages(&MsgQueryArgs::default())
            .await?
            .len();
        let bodies: Vec<Vec<u8>> = bo_group
            .find_messages(&MsgQueryArgs::default())
            .await?
            .into_iter()
            .map(|m| m.decrypted_message_bytes)
            .collect();
        assert!(bodies.contains(&b"missed one".to_vec()));
        assert!(bodies.contains(&b"missed two".to_vec()));
        assert!(
            first.messages >= 2,
            "the replayed tail must be counted (got {})",
            first.messages
        );
        assert_eq!(first.conversations, 0, "no new group was joined");

        // The honesty proof for the counter: the replay of already-stored
        // history persists nothing, so the summary must say so.
        let second = bo.catch_up_bidi(None).await?;
        let count_after_second = bo_group
            .find_messages(&MsgQueryArgs::default())
            .await?
            .len();
        assert_eq!(count_after_first, count_after_second);
        assert_eq!(
            second,
            CatchUpSummary {
                completed: true,
                ..Default::default()
            },
            "a second run still reaches live, but persists nothing, so its counts are zero"
        );
    }

    /// Nothing owed at all — a fresh client's run is just the welcome-topic
    /// wave, the half-close, and the server's close.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_with_nothing_owed_completes() {
        tester!(alix);
        let summary = alix.catch_up_bidi(None).await?;
        assert_eq!(
            summary,
            CatchUpSummary {
                completed: true,
                ..Default::default()
            },
            "nothing owed still completes, with zero counts"
        );
    }

    /// The legacy arm reports the same summary shape: a pending welcome and
    /// its history count on the first call (diffed against the pre-call
    /// store), and a repeat call — cursors now advanced by the query path —
    /// reports zero.
    #[xmtp_common::test(unwrap_try = true)]
    async fn legacy_catch_up_counts_the_same_way() {
        tester!(alix);
        tester!(bo);

        let group = alix.create_group(None, None).await?;
        group.invite(&bo).await?;
        group.send_msg(b"legacy one").await;
        group.send_msg(b"legacy two").await;

        let first = bo.catch_up_legacy().await?;
        assert_eq!(
            first.conversations, 1,
            "the welcome-joined group must be counted"
        );
        assert!(
            first.messages >= 2,
            "the synced history must be counted (got {})",
            first.messages
        );

        let second = bo.catch_up_legacy().await?;
        assert_eq!(
            second,
            CatchUpSummary {
                completed: true,
                ..Default::default()
            },
            "a repeat legacy run still completes, but persists nothing, so its counts are zero"
        );
    }
}

/// Pure planning tests — no wire, no backend, feature-independent.
#[cfg(test)]
mod plan_tests {
    use super::plan_catch_up_waves;
    use xmtp_proto::types::Topic;

    /// A subscription set far larger than one frame's topic cap splits into
    /// several bounded waves: ids contiguous from `1`, every wave an add-only
    /// `history_only` frame, and every topic carried exactly once. Guards the
    /// bounded-open invariant that keeps a large account from opening catch-up
    /// with one oversized frame.
    #[xmtp_common::test]
    fn plan_splits_a_large_subscription_set_into_bounded_waves() {
        let n: u32 = 5000;
        let subs: Vec<(Topic, u64)> = (0..n)
            .map(|i| (Topic::new_group_message(i.to_le_bytes()), i as u64))
            .collect();

        let (open, overflow) = plan_catch_up_waves(subs);

        // The cap is well under 5000, so the set must have split.
        assert!(!overflow.is_empty(), "a 5000-topic set must split");
        assert_eq!(open.mutate_id, 1, "the open is always wave 1");

        let waves: Vec<_> = std::iter::once(&open).chain(overflow.iter()).collect();
        let cap = open.adds.len();
        for (i, wave) in waves.iter().enumerate() {
            assert_eq!(
                wave.mutate_id,
                i as u64 + 1,
                "wave ids run contiguously from 1"
            );
            assert!(
                wave.history_only && wave.removes.is_empty(),
                "every catch-up wave is an add-only history frame"
            );
            assert!(!wave.adds.is_empty(), "no empty wave is ever emitted");
            assert!(
                wave.adds.len() <= cap,
                "no wave exceeds the first (full) frame"
            );
        }
        let total: usize = waves.iter().map(|w| w.adds.len()).sum();
        assert_eq!(total as u32, n, "every topic is carried exactly once");
    }

    /// The common case — a set that fits in one frame — opens with a single
    /// wave and queues no overflow.
    #[xmtp_common::test]
    fn plan_keeps_a_small_set_in_one_wave() {
        let subs: Vec<(Topic, u64)> = (0u32..3)
            .map(|i| (Topic::new_group_message(i.to_le_bytes()), i as u64))
            .collect();

        let (open, overflow) = plan_catch_up_waves(subs);

        assert!(overflow.is_empty(), "a 3-topic set fits one frame");
        assert_eq!(open.mutate_id, 1);
        assert_eq!(open.adds.len(), 3, "all three topics ride the open");
    }
}
