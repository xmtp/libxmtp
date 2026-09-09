//! Bounded catch-up on a dedicated backend subscription.
//!
//! Each Update must receive Applied. Process envelopes through the fixed
//! targets, including groups discovered by welcomes within those targets.
//! Later traffic does not extend the run. Processing failures are counted
//! and keep completed false. Durable cursors remain available for recovery.

use std::collections::{HashMap, HashSet, VecDeque};
use xmtp_common::time::{Duration, Instant};

use tracing::Instrument;
use xmtp_api_backend::{
    BackendBinding, BidiConnection, BidiEvent, OpenError, TransportBinding, TryMutateError,
    chunk_mutate_adds,
};
use xmtp_common::{ErrorCode, RetryableError, retryable};
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::backend_v1::{ServerEnvelope, subscribe_request::Update};
use xmtp_proto::types::{Cursor, GroupId, SequenceId, Topic};

use super::process_message::{ProcessMessageFuture, process_one};
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
    /// Envelopes or processing steps that failed during this call.
    pub failed: u64,
    /// Whether the run reached the live edge before its optional deadline.
    /// `false` means a timeout or processing failure prevented completion.
    /// The counts contain the partial total. A later call resumes from
    /// durable state.
    pub completed: bool,
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum CatchUpError {
    /// Seeding, processing, or the legacy fallback failed.
    #[error(transparent)]
    #[error_code(inherit)]
    Subscribe(#[from] SubscribeError),
    /// The requested set exceeds the backend wire limit. Not retryable.
    #[error("catch-up exceeds the backend topic limit")]
    TooManyTopics,
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
            // A permanent open error cannot be retried.
            Self::TooManyTopics | Self::DeadEnd(_) => false,
            // Wire deaths are transient; a fresh call resumes from durable
            // state.
            Self::Exhausted { .. } => true,
        }
    }
}

/// How one bounded run over the wire ended.
enum AttemptEnd {
    /// Every update is acknowledged, every target is reached, and intake is idle.
    Complete,
    /// The wire died (or the welcome backlog overflowed) mid-run; retry
    /// from durable state.
    WireDied,
}

/// Build updates with increasing IDs and enforce the per-wire topic cap.
fn catch_up_update(
    subs: Vec<(Topic, SequenceId)>,
    first_id: u64,
) -> Result<Vec<Update>, CatchUpError> {
    if subs.len() > xmtp_configuration::BACKEND_DEFAULT_MAX_STREAM_TOPICS {
        return Err(CatchUpError::TooManyTopics);
    }
    Ok(chunk_catch_up_updates(subs, first_id))
}

/// Split adds and assign IDs after the caller checks the connection topic cap.
fn chunk_catch_up_updates(subs: Vec<(Topic, SequenceId)>, first_id: u64) -> Vec<Update> {
    chunk_mutate_adds(subs)
        .into_iter()
        .enumerate()
        .map(|(offset, adds)| BackendBinding::build_mutate(adds, [], first_id + offset as u64))
        .collect()
}

/// Restrict processing to each registration's fixed target.
fn within_targets(envelope: &ServerEnvelope, targets: &HashMap<Topic, u64>) -> bool {
    // Keep malformed metadata so the decoder records a processing failure.
    let Some(meta) = envelope.meta.as_ref() else {
        return true;
    };
    let Some(topic) = meta
        .topic
        .as_ref()
        .and_then(|topic| Topic::parse(&topic.topic).ok())
    else {
        return true;
    };
    let Some(cursor) = meta.cursor.as_ref() else {
        return true;
    };
    targets
        .get(&topic)
        .is_some_and(|target| cursor.sequence_id <= *target)
}

/// Receipt of a target envelope settles intake only after its batch is processed.
fn settle_targets(batch: &[ServerEnvelope], outstanding: &mut HashMap<Topic, u64>) {
    for envelope in batch {
        let Some(meta) = envelope.meta.as_ref() else {
            continue;
        };
        let Some(topic) = meta
            .topic
            .as_ref()
            .and_then(|topic| Topic::parse(&topic.topic).ok())
        else {
            continue;
        };
        let Some(cursor) = meta.cursor.as_ref() else {
            continue;
        };
        if outstanding
            .get(&topic)
            .is_some_and(|target| cursor.sequence_id >= *target)
        {
            outstanding.remove(&topic);
        }
    }
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
        let deadline = timeout.and_then(|d| Instant::now().checked_add(d));
        match self.catch_up_bidi(deadline).await {
            Ok(summary) => return Ok(summary),
            Err(CatchUpError::DeadEnd(error)) => {
                tracing::warn!("catch-up open failed without retry: {error}");
            }
            Err(error) => return Err(error),
        }
        // The legacy arm computes a terminal store diff, so it has no partial to
        // hand back mid-flight: on the deadline return an empty, `completed=false`
        // summary (its stores are still persisted; a later call resumes them).
        match deadline {
            Some(dl) => match xmtp_common::time::timeout(
                dl.saturating_duration_since(Instant::now()),
                self.catch_up_legacy(),
            )
            .await
            {
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
        deadline: Option<Instant>,
    ) -> Result<CatchUpSummary, CatchUpError> {
        let mut summary = CatchUpSummary::default();
        for attempt in 1..=MAX_ATTEMPTS {
            let end = match deadline {
                Some(dl) => {
                    match xmtp_common::time::timeout(
                        dl.saturating_duration_since(Instant::now()),
                        self.catch_up_attempt(&mut summary),
                    )
                    .await
                    {
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
                    summary.completed = summary.failed == 0;
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
                    let wake = Instant::now().checked_add(backoff);
                    if matches!(deadline, Some(dl) if wake.is_none_or(|w| w >= dl)) {
                        return Ok(summary);
                    }
                    xmtp_common::time::sleep(backoff).await;
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
        let pre_groups = db.find_groups(query()).map_err(SubscribeError::from)?;
        let pre_ids: HashSet<GroupId> = pre_groups.iter().map(|g| g.id).collect();
        let group_ids: Vec<GroupId> = pre_ids.iter().copied().collect();
        let mut cursors = db
            .get_last_cursor_for_ids(
                &group_ids,
                &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
            )
            .map_err(SubscribeError::from)?;
        let pre_stored: HashSet<Cursor> = db
            .messages_newer_than(&cursors)
            .map_err(SubscribeError::from)?
            .into_iter()
            .map(|(_, cursor)| cursor)
            .collect();

        WelcomeService::new(&self.context)
            .sync_all_welcomes_and_groups(None)
            .await
            .map_err(|e| CatchUpError::from(SubscribeError::from(Box::new(e))))?;

        let post_groups = db.find_groups(query()).map_err(SubscribeError::from)?;
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
            .map_err(SubscribeError::from)?
            .into_iter()
            .filter(|(group_id, cursor)| {
                !sync_ids.contains(group_id) && !pre_stored.contains(cursor)
            })
            .count() as u64;
        summary.completed = summary.failed == 0;
        Ok(summary)
    }

    /// Process through the fixed targets, then drop the dedicated connection.
    /// Welcomes can add group topics. Count only newly stored items.
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
        let welcome_floor = welcome_seed(&db, installation)?;
        let known = known_welcomes_above(&db, welcome_floor)?;
        let groups = db
            .find_groups(GroupQueryArgs {
                include_duplicate_dms: true,
                // Sync groups are caught up too — their replayed traffic
                // nudges the device-sync worker, like the streams do.
                include_sync_groups: true,
                ..Default::default()
            })
            .map_err(SubscribeError::from)?;
        let mut sync_groups: HashSet<GroupId> = groups
            .iter()
            .filter(|g| matches!(g.conversation_type, ConversationType::Sync))
            .map(|g| g.id)
            .collect();
        let group_ids: Vec<GroupId> = groups.into_iter().map(|g| g.id).collect();
        let seeds = seed_groups(&db, &group_ids)?;

        let mut tracked: HashSet<Topic> = seeds.floors.keys().cloned().collect();
        let mut subs = seeds.subs();
        subs.push((Topic::new_welcome_message(installation), welcome_floor));
        // Already-stored identities above the floors: replays of these are
        // skipped without touching the pipeline (stored by an earlier stream
        // or sync — the durable cursor alone is not a delivery floor).
        let mut seen = seeds.seen;

        let api = self.context.api().api_client.clone();
        // Keep each update within the backend limits.
        let mut floors: HashMap<Topic, u64> = subs.iter().cloned().collect();
        let updates = catch_up_update(subs, 1)?;
        let mut next_update_id = updates.len() as u64 + 1;
        let mut queued: VecDeque<Update> = updates.into();
        let open = queued
            .pop_front()
            .expect("welcome topic requires an update");
        let mut conn = match BidiConnection::open(&api, open).await {
            Ok(conn) => conn,
            Err(e) => {
                let open = OpenError::new(e);
                return if open.is_retryable() {
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
        let mut pending_updates = HashSet::from([1]);
        let mut outstanding = HashMap::new();
        let mut targets = HashMap::new();

        loop {
            while let Some(update) = queued.pop_front() {
                let id = update.id;
                match conn.try_mutate(update) {
                    Ok(()) => {
                        pending_updates.insert(id);
                    }
                    Err(TryMutateError::Full(update)) => {
                        // Retried next turn; replay frames keep arriving (or
                        // the watchdog ends the wire and the bounded attempt
                        // retry takes over), so the loop keeps turning —
                        // drainage is bounded by wire liveness, not by luck.
                        tracing::debug!(
                            queued_updates = queued.len() + 1,
                            "catch-up update deferred on a full command slot"
                        );
                        queued.push_front(update);
                        break;
                    }
                    Err(TryMutateError::Closed(_)) => return Ok(AttemptEnd::WireDied),
                }
            }
            if pending_updates.is_empty()
                && outstanding.is_empty()
                && queued.is_empty()
                && intake.is_idle()
            {
                break;
            }
            tokio::select! {
                event = conn.next() => match event {
                    None => return Ok(AttemptEnd::WireDied),
                    Some(BidiEvent::GroupMessages { messages }) => {
                        let messages: Vec<_> = messages.into_iter().filter(|message| within_targets(message, &targets)).collect();
                        self.process_group_batch(&factory, messages.clone(), &mut seen, &sync_groups, summary).await;
                        settle_targets(&messages, &mut outstanding);
                    }
                    Some(BidiEvent::WelcomeMessages { messages }) => {
                        let messages: Vec<_> = messages.into_iter().filter(|message| within_targets(message, &targets)).collect();
                        settle_targets(&messages, &mut outstanding);
                        let accepted = intake.absorb_batch(messages);
                        summary.failed += std::mem::take(&mut intake.decode_failures);
                        if !accepted {
                            summary.failed += 1;
                            return Ok(AttemptEnd::WireDied);
                        }
                    }
                    Some(BidiEvent::Applied { id, targets: added }) => {
                        pending_updates.remove(&id);
                        for (topic, target) in added {
                            if target > floors.get(&topic).copied().unwrap_or_default() {
                                outstanding.insert(topic.clone(), target);
                            }
                            targets.insert(topic, target);
                        }
                    }
                    Some(BidiEvent::Started { .. }) => {}

                },
                _ = xmtp_common::time::sleep(Duration::from_millis(25)), if !queued.is_empty() => {},
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
                                let gseeds = seed_groups(&db, &[group.group_id])?;
                                let adds = gseeds.subs();
                                seen.extend(gseeds.seen);
                                if tracked.len() + 2 > xmtp_configuration::BACKEND_DEFAULT_MAX_STREAM_TOPICS {
                                    return Err(CatchUpError::TooManyTopics);
                                }
                                tracked.insert(topic);
                                floors.extend(adds.iter().cloned());
                                let updates = catch_up_update(adds, next_update_id)?;
                                next_update_id += updates.len() as u64;
                                queued.extend(updates);
                            }
                        }
                        if let Some(cursor) = outcome.seen {
                            intake.known.insert(cursor);
                        }
                    }
                    // The welcome stays unrecorded, so it replays on the
                    // next call (or any welcome stream/sync) — same recovery
                    // as a live stream surfacing the error.
                    Err(e) => {
                        summary.failed += 1;
                        tracing::warn!("catch-up welcome processing failed: {e}");
                    },
                },
            }
        }

        // Drop cancels the dedicated stream after all processing settles.
        drop(conn);
        Ok(AttemptEnd::Complete)
    }

    /// Decode and process one replay batch through the streaming pipeline.
    /// Failures are logged and skipped — the durable cursor never advances
    /// past them, so a later sync retries (module docs).
    async fn process_group_batch(
        &self,
        factory: &ProcessMessageFuture<Context>,
        batch: Vec<ServerEnvelope>,
        seen: &mut HashSet<Cursor>,
        sync_groups: &HashSet<GroupId>,
        summary: &mut CatchUpSummary,
    ) {
        let batch_started = Instant::now();
        let batch_size = batch.len();
        for proto in batch {
            let typed = match xmtp_api_backend::envelope::decode_group_message(proto) {
                Ok(typed) => typed,
                Err(e) => {
                    summary.failed += 1;
                    tracing::warn!("catch-up skipping undecodable group message: {e}");
                    continue;
                }
            };
            if !seen.insert(typed.cursor) {
                continue;
            }
            let (topic, cursor) = (Topic::new_group_message(typed.group_id), typed.cursor);
            let started = Instant::now();
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
                    summary.failed += processed.failed;
                    if let Some(message) = processed.message {
                        // The pipeline may store ahead of the envelope it was
                        // handed (recovery sync) — record the surfaced
                        // identity too, so its own replay frame is skipped.
                        seen.insert(Cursor(message.sequence_id as u64));
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
                Err(e) => {
                    summary.failed += 1;
                    tracing::warn!(%topic, ?cursor, "catch-up message processing failed: {e}");
                }
            }
        }
        tracing::debug!(
            batch = batch_size,
            elapsed_ms = batch_started.elapsed().as_millis() as u64,
            "catch-up: message batch processed"
        );
    }
}

/// Live integration tests against the backend.
#[cfg(test)]
mod tests {
    use super::CatchUpSummary;
    use crate::tester;
    use crate::utils::MlsGroupExt;
    use xmtp_db::group_message::MsgQueryArgs;

    /// A stored envelope with invalid ciphertext must prevent completion.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_reports_message_processing_failures() {
        use crate::context::XmtpSharedContext;
        use xmtp_proto::backend_v1::client_envelope::Payload;
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = bo.create_group(None, None)?;
        group.invite(&alix).await?;
        group.send_msg(b"valid message").await;
        let mut envelope = bo
            .context
            .api()
            .query_group_messages(group.group_id)
            .await?;
        let cursor = envelope.pop().unwrap().cursor;
        let stored = bo.context.api().get_envelope(cursor.0).await?;
        let mut payload = stored.envelope.unwrap();
        let Some(Payload::GroupMessage(message)) = payload.payload.as_mut() else {
            panic!("expected a group message");
        };
        *message.data.last_mut().unwrap() ^= 1;
        bo.context
            .api()
            .send_group_messages(vec![xmtp_api::PublishUnit::single(payload)?])
            .await?;
        let summary = alix
            .catch_up_bidi(Some(
                xmtp_common::time::Instant::now() + xmtp_common::time::Duration::from_secs(10),
            ))
            .await?;
        assert!(summary.failed > 0, "invalid ciphertext must be reported");
        assert!(!summary.completed, "processing failures prevent completion");
    }

    /// A client that has never streamed or synced catches up: the pending
    /// welcome joins the group (discovery adds an update on the same
    /// stream) and the group's history lands in the store.
    #[xmtp_common::test(unwrap_try = true)]
    async fn catch_up_joins_pending_groups_and_stores_history() {
        tester!(alix);
        tester!(bo);

        let group = bo.create_group(None, None)?;
        group.invite(&alix).await?;
        group.send_msg(b"while you were out").await;
        group.send_msg(b"still out").await;

        let summary = alix.catch_up_bidi(None).await?;

        let alix_group = alix.group(&group.group_id)?;
        let bodies: Vec<Vec<u8>> = alix_group
            .find_messages(&MsgQueryArgs::default())?
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

        let group = alix.create_group(None, None)?;
        group.invite(&bo).await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&group.group_id)?;
        bo_group.sync().await?; // durable cursor at "now"

        group.send_msg(b"missed one").await;
        group.send_msg(b"missed two").await;

        let first = bo.catch_up_bidi(None).await?;
        let count_after_first = bo_group.find_messages(&MsgQueryArgs::default())?.len();
        let bodies: Vec<Vec<u8>> = bo_group
            .find_messages(&MsgQueryArgs::default())?
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
        let count_after_second = bo_group.find_messages(&MsgQueryArgs::default())?.len();
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
    /// update acknowledgement and the empty target set.
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

        let group = alix.create_group(None, None)?;
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

#[cfg(test)]
mod plan_tests {
    use super::{
        BackendBinding, CatchUpError, TransportBinding, catch_up_update, chunk_catch_up_updates,
    };
    use xmtp_configuration::{
        BACKEND_DEFAULT_MAX_STREAM_TOPICS, BACKEND_DEFAULT_MAX_UPDATE_ADDS as MAX_MUTATE_TOPICS,
    };
    use xmtp_proto::types::Topic;

    #[xmtp_common::test(unwrap_try = true)]
    fn plan_splits_a_large_subscription_set_into_bounded_updates() {
        // The connection cap equals the add cap. Test the shared chunk builder
        // directly so the input exceeds one update without bypassing the public guard.
        let input_count = MAX_MUTATE_TOPICS + 1;
        let first_id = 7;
        let subs: Vec<_> = (0..input_count)
            .map(|i| {
                let mut id = [0u8; 16];
                id[..8].copy_from_slice(&(i as u64).to_le_bytes());
                (Topic::new_group_message(id), i as u64)
            })
            .collect();
        let expected_adds = BackendBinding::build_mutate(subs.clone(), [], first_id).adds;
        let updates = chunk_catch_up_updates(subs, first_id);
        assert!(updates.len() > 1);
        assert_eq!(
            updates
                .iter()
                .map(|update| update.adds.len())
                .sum::<usize>(),
            input_count
        );
        assert_eq!(
            updates
                .iter()
                .flat_map(|update| update.adds.iter().cloned())
                .collect::<Vec<_>>(),
            expected_adds
        );
        for (index, update) in updates.iter().enumerate() {
            assert_eq!(update.id, first_id + index as u64);
            assert!(update.removes.is_empty());
            assert!(!update.adds.is_empty());
            assert!(update.adds.len() <= MAX_MUTATE_TOPICS);
        }
        let too_many =
            vec![(Topic::new_group_message([0; 16]), 0); BACKEND_DEFAULT_MAX_STREAM_TOPICS + 1];
        assert!(matches!(
            catch_up_update(too_many, 1),
            Err(CatchUpError::TooManyTopics)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn plan_keeps_a_small_set_in_one_update() {
        let subs = (0u8..3)
            .map(|i| (Topic::new_group_message([i; 16]), i as u64))
            .collect();
        let updates = catch_up_update(subs, 1)?;
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].id, 1);
        assert_eq!(updates[0].adds.len(), 3);
    }
}
