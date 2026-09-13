//! Durable receipt and ordered processing positions for network envelopes.

use diesel::{
    prelude::*,
    sql_types::{BigInt, Binary, Integer},
};
use xmtp_proto::types::{Cursor, GroupId};

use super::{
    refresh_state::EntityKind,
    schema::{
        group_welcome_discovery as discovery, incoming_envelopes as incoming,
        refresh_state as progress,
    },
    stream_storage::{BudgetScope, StreamStorageError, stream_transaction},
};
use crate::{ConnectionExt, StorageError};

/// Separate network queues with independent pending-work budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkEntityKind {
    /// Independent Welcome work for an installation.
    Welcome,
    /// One ordered queue for both group commits and application messages.
    Group,
    /// Ordered identity updates needed by group and Welcome processing.
    Identity,
}

impl NetworkEntityKind {
    pub fn entity_kind(self) -> EntityKind {
        match self {
            Self::Welcome => EntityKind::Welcome,
            Self::Group => EntityKind::ApplicationMessage,
            Self::Identity => EntityKind::Identity,
        }
    }
}

/// The database key for one network log and its received/processed positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTopic {
    /// Group ID, installation ID, or inbox ID bytes, without the wire topic prefix.
    pub entity_id: Vec<u8>,
    /// Prevents progress or budgets from being shared across different log kinds.
    pub kind: NetworkEntityKind,
}

impl StreamTopic {
    pub fn group(group_id: GroupId) -> Self {
        Self {
            entity_id: group_id.as_ref().to_vec(),
            kind: NetworkEntityKind::Group,
        }
    }
}

/// The adapter validates wire metadata and the supplied hash shape before admission.
/// The backend owns the envelope hash; the client does not recompute it.
#[derive(Debug, Clone)]
pub struct NewIncomingEnvelope {
    /// Backend sequence ID. IDs must increase, but need not be consecutive integers.
    pub sequence_id: Cursor,
    /// Exact serialized envelope retained for later processing and crash recovery.
    pub envelope: Vec<u8>,
}

/// Admitted work that has not yet been applied or terminally rejected.
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = incoming)]
pub struct StoredIncomingEnvelope {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub sequence_id: i64,
    /// Durable input bytes. Receipt does not imply successful MLS processing.
    pub envelope: Vec<u8>,
    /// Earliest retry time in Unix nanoseconds.
    pub retry_at_ns: i64,
    /// A new coordinator generation must recheck this work; a timer must not retry it.
    pub blocked: bool,
    /// Stable diagnostic code only. Do not store input data or formatted errors here.
    pub error_code: Option<String>,
    /// First retry deadline; later attempts must not extend it.
    pub retry_expires_at_ns: Option<i64>,
}

/// Pending-work metadata for a fixed processing target. This does not contain ciphertext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEnvelopeState {
    pub sequence_id: Cursor,
    pub blocked: bool,
    pub error_code: Option<String>,
    pub retry_at_ns: i64,
}

/// Both limits must hold before a pending batch can commit.
#[derive(Debug, Clone, Copy)]
pub struct PendingBudget {
    /// Maximum retained envelope count.
    pub rows: u64,
    /// Maximum sum of serialized envelope sizes.
    pub bytes: u64,
}

/// Admission limits checked under the same writer as the envelope insertions.
#[derive(Debug, Clone, Copy)]
pub struct IncomingLimits {
    /// Bound one admission call, including supplied overlap rows.
    pub batch: PendingBudget,
    /// Bound retained work for this topic.
    pub topic: PendingBudget,
    /// Each kind has its own budget. Group work cannot use dependency capacity.
    pub kind: PendingBudget,
}

/// Durable network progress. Application acknowledgement is a separate position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TopicProgress {
    /// P: the prefix applied or terminally rejected in committed state transactions.
    pub processed: Cursor,
    /// F: the prefix whose envelope bytes were admitted atomically. P never exceeds F.
    pub received: Cursor,
}

/// The group-state proof required to install a validated Welcome's anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinAnchorMode {
    /// The anchor must advance an existing processed position.
    Advance,
    /// The caller proved that an inactive group has a newer re-add Welcome.
    /// Its anchor must equal the processed removal position in the same transaction.
    InactiveReadd,
}

/// Receipt progress visible only after the full admission transaction commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionResult {
    /// The committed F position, including any previously admitted overlap.
    pub received: Cursor,
    /// Newly stored rows. Duplicate overlap does not increase this count.
    pub inserted: usize,
}

/// One bounded rejection diagnostic per topic, not a payload history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRejection {
    pub sequence_id: Cursor,
    pub code: String,
}

/// Queue size metadata used to pause full topics without reading ciphertext.
#[derive(Debug, Clone, QueryableByName)]
pub struct PendingTopicUsage {
    #[diesel(sql_type = Binary)]
    pub entity_id: Vec<u8>,
    #[diesel(sql_type = BigInt)]
    pub rows: i64,
    #[diesel(sql_type = BigInt)]
    pub bytes: i64,
}

/// Retry state attached to the same still-pending envelope.
#[derive(Debug, Clone)]
pub struct IncomingRetry {
    /// Earliest retry time in Unix nanoseconds.
    pub retry_at_ns: i64,
    /// Requires a new coordinator generation instead of a timer retry.
    pub blocked: bool,
    /// Stable error code with no input payload or formatted error text.
    pub error_code: Option<String>,
    /// The first deadline wins. A retry must not extend pointer retention.
    pub retry_expires_at_ns: Option<i64>,
}

/// Atomic receipt, ordered completion, and bounded retry state for network logs.
pub trait QueryIncomingEnvelope: ConnectionExt + Sized {
    /// Record the first successful Welcome installation inside its state transaction.
    /// Rejoin does not change discovery. Local creation and history import do not call this.
    fn record_welcome_discovery(
        &self,
        group_id: GroupId,
        welcome_cursor: Cursor,
    ) -> Result<(), StorageError> {
        let sequence = i64::try_from(welcome_cursor.0)
            .ok()
            .filter(|sequence| *sequence > 0)
            .ok_or(StreamStorageError::InvalidBatch)?;
        stream_transaction(self, |conn| {
            diesel::insert_into(discovery::table)
                .values((
                    discovery::group_id.eq(group_id),
                    discovery::first_welcome_sequence_id.eq(sequence),
                ))
                .on_conflict(discovery::group_id)
                .do_nothing()
                .execute(conn)?;
            Ok(())
        })
    }

    /// Groups first discovered through the fixed own-installation Welcome target.
    /// Later joins and local/imported groups cannot expand this captured discovery set.
    fn group_ids_discovered_through(&self, target: Cursor) -> Result<Vec<GroupId>, StorageError> {
        let target = i64::try_from(target.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        self.raw_query(|conn| {
            discovery::table
                .filter(discovery::first_welcome_sequence_id.le(target))
                .order((
                    discovery::first_welcome_sequence_id.asc(),
                    discovery::group_id.asc(),
                ))
                .select(discovery::group_id)
                .load(conn)
        })
        .map_err(Into::into)
    }

    /// Keep one rejection diagnostic per topic, replacing it with a later rejection.
    /// Store a stable error code, never an input payload or a formatted error message.
    /// The caller records this inside the same state transaction that deletes the pending row.
    fn record_terminal_rejection(
        &self,
        topic: &StreamTopic,
        sequence: Cursor,
        code: &'static str,
    ) -> Result<(), StorageError> {
        let sequence_id = i64::try_from(sequence.0)
            .ok()
            .filter(|id| *id > 0)
            .ok_or(StreamStorageError::InvalidBatch)?;
        if code.is_empty() {
            return Err(StreamStorageError::InvalidBatch.into());
        }
        stream_transaction(self, |conn| {
            if sequence > load_progress(conn, topic)?.received {
                return Err(StreamStorageError::InvalidBatch.into());
            }
            let pending = incoming::table
                .find((&topic.entity_id, topic.kind.entity_kind(), sequence_id))
                .select(incoming::sequence_id)
                .first::<i64>(conn)
                .optional()?
                .is_some();
            if !pending {
                return Err(StreamStorageError::HeadChanged.into());
            }
            if topic.kind != NetworkEntityKind::Welcome
                && first_pending(conn, topic)?.is_none_or(|head| head.sequence_id != sequence_id)
            {
                return Err(StreamStorageError::HeadChanged.into());
            }
            diesel::update(progress::table.find((&topic.entity_id, topic.kind.entity_kind())))
                .set((
                    progress::last_rejection_sequence_id.eq(sequence_id),
                    progress::last_rejection_code.eq(code),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Read the topic's last committed rejection code without loading envelope data.
    fn read_last_rejection(
        &self,
        topic: &StreamTopic,
    ) -> Result<Option<TerminalRejection>, StorageError> {
        let row = self.raw_query(|conn| {
            progress::table
                .find((&topic.entity_id, topic.kind.entity_kind()))
                .select((
                    progress::last_rejection_sequence_id,
                    progress::last_rejection_code,
                ))
                .first::<(Option<i64>, Option<String>)>(conn)
                .optional()
        })?;
        match row {
            None | Some((None, None)) => Ok(None),
            Some((Some(sequence), Some(code))) => Ok(Some(TerminalRejection {
                sequence_id: Cursor(sequence as u64),
                code,
            })),
            _ => Err(StorageError::DbDeserialize),
        }
    }

    /// Commit the complete ordered batch and its received position together.
    /// An overlap is safe; a gap before the source cursor is rejected.
    /// Validation, capacity, or storage failure leaves both F and pending rows unchanged.
    fn admit_ordered_batch(
        &self,
        topic: &StreamTopic,
        after: Cursor,
        envelopes: &[NewIncomingEnvelope],
        limits: IncomingLimits,
    ) -> Result<AdmissionResult, StorageError> {
        let mut previous = after.0;
        let mut bytes = 0u64;
        for envelope in envelopes {
            if envelope.sequence_id.0 <= previous
                || envelope.envelope.is_empty()
                || envelope.sequence_id.0 > i64::MAX as u64
            {
                return Err(StreamStorageError::InvalidBatch.into());
            }
            previous = envelope.sequence_id.0;
            bytes = bytes
                .checked_add(envelope.envelope.len() as u64)
                .ok_or(StreamStorageError::InvalidBatch)?;
        }
        check_budget(
            envelopes.len() as u64,
            bytes,
            limits.batch,
            BudgetScope::Batch,
        )?;
        stream_transaction(self, |conn| {
            initialize_progress(conn, topic)?;
            let state = load_progress(conn, topic)?;
            if after.0 > state.received.0 {
                return Err(StreamStorageError::MissingPrefix {
                    after: after.0,
                    received: state.received.0,
                }
                .into());
            }
            let new: Vec<_> = envelopes
                .iter()
                .filter(|envelope| envelope.sequence_id.0 > state.received.0)
                .collect();
            if new.is_empty() {
                return Ok(AdmissionResult {
                    received: state.received,
                    inserted: 0,
                });
            }
            let new_bytes = new
                .iter()
                .map(|envelope| envelope.envelope.len() as u64)
                .sum::<u64>();
            let usages = pending_usage(conn, topic.kind)?;
            let own = usages
                .iter()
                .find(|usage| usage.entity_id == topic.entity_id);
            let topic_rows = own.map_or(0, |usage| usage.rows as u64);
            let topic_bytes = own.map_or(0, |usage| usage.bytes as u64);
            check_budget(
                topic_rows + new.len() as u64,
                topic_bytes + new_bytes,
                limits.topic,
                BudgetScope::Topic,
            )?;
            let kind_rows: u64 = usages.iter().map(|usage| usage.rows as u64).sum();
            let kind_bytes: u64 = usages.iter().map(|usage| usage.bytes as u64).sum();
            check_budget(
                kind_rows + new.len() as u64,
                kind_bytes + new_bytes,
                limits.kind,
                BudgetScope::Kind,
            )?;
            for envelope in &new {
                diesel::insert_into(incoming::table)
                    .values((
                        incoming::entity_id.eq(&topic.entity_id),
                        incoming::entity_kind.eq(topic.kind.entity_kind()),
                        incoming::sequence_id.eq(envelope.sequence_id.0 as i64),
                        incoming::envelope.eq(&envelope.envelope),
                    ))
                    .execute(conn)?;
            }
            let received = new
                .last()
                .ok_or(StreamStorageError::InvalidBatch)?
                .sequence_id;
            diesel::update(progress::table.find((&topic.entity_id, topic.kind.entity_kind())))
                .set(progress::received_sequence_id.eq(received.0 as i64))
                .execute(conn)?;
            Ok(AdmissionResult {
                received,
                inserted: new.len(),
            })
        })
    }

    /// Read P and F from the same row; an unseen topic starts at zero.
    fn topic_progress(&self, topic: &StreamTopic) -> Result<TopicProgress, StorageError> {
        self.raw_query(|conn| Ok(load_progress(conn, topic)))?
    }

    /// Read the next actual ID. Missing integer IDs are not queue entries.
    fn first_pending_envelope(
        &self,
        topic: &StreamTopic,
    ) -> Result<Option<StoredIncomingEnvelope>, StorageError> {
        self.raw_query(|conn| first_pending(conn, topic))
            .map_err(Into::into)
    }

    /// Recheck one independent welcome under the caller's state transaction.
    fn pending_envelope(
        &self,
        topic: &StreamTopic,
        sequence: Cursor,
    ) -> Result<Option<StoredIncomingEnvelope>, StorageError> {
        let sequence = i64::try_from(sequence.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        self.raw_query(|conn| {
            incoming::table
                .find((&topic.entity_id, topic.kind.entity_kind(), sequence))
                .first::<StoredIncomingEnvelope>(conn)
                .optional()
        })
        .map_err(Into::into)
    }

    /// Read actual pending IDs through the fixed target without loading ciphertext.
    /// Topic and kind admission limits bound the number of stored rows returned here.
    fn pending_states_through(
        &self,
        topic: &StreamTopic,
        target: Cursor,
    ) -> Result<Vec<PendingEnvelopeState>, StorageError> {
        let target = i64::try_from(target.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        let rows = self.raw_query(|conn| {
            incoming::table
                .filter(incoming::entity_id.eq(&topic.entity_id))
                .filter(incoming::entity_kind.eq(topic.kind.entity_kind()))
                .filter(incoming::sequence_id.le(target))
                .order(incoming::sequence_id.asc())
                .select((
                    incoming::sequence_id,
                    incoming::blocked,
                    incoming::error_code,
                    incoming::retry_at_ns,
                ))
                .load::<(i64, bool, Option<String>, i64)>(conn)
        })?;
        rows.into_iter()
            .map(|(sequence, blocked, error_code, retry_at_ns)| {
                Ok(PendingEnvelopeState {
                    sequence_id: Cursor(
                        u64::try_from(sequence).map_err(|_| StorageError::DbDeserialize)?,
                    ),
                    blocked,
                    error_code,
                    retry_at_ns,
                })
            })
            .collect()
    }

    /// Complete only the current group or identity head. Welcomes are independent.
    /// Call this inside the state transaction that applies or rejects the envelope.
    /// Deletion and P advance commit together; an unresolved Welcome keeps its prefix open.
    fn complete_pending_envelope(
        &self,
        topic: &StreamTopic,
        sequence: Cursor,
    ) -> Result<bool, StorageError> {
        let sequence = i64::try_from(sequence.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        stream_transaction(self, |conn| {
            let Some(head) = first_pending(conn, topic)? else {
                return Ok(false);
            };
            if topic.kind != NetworkEntityKind::Welcome && head.sequence_id != sequence {
                return Err(StreamStorageError::HeadChanged.into());
            }
            let deleted = diesel::delete(incoming::table.find((
                &topic.entity_id,
                topic.kind.entity_kind(),
                sequence,
            )))
            .execute(conn)?;
            if deleted == 0 {
                return Ok(false);
            }
            let handled = if topic.kind == NetworkEntityKind::Welcome {
                match first_pending(conn, topic)? {
                    Some(pending) => pending.sequence_id - 1,
                    None => load_progress(conn, topic)?.received.0 as i64,
                }
            } else {
                sequence
            };
            diesel::update(progress::table.find((&topic.entity_id, topic.kind.entity_kind())))
                .set(progress::sequence_id.eq(handled))
                .execute(conn)?;
            Ok(true)
        })
    }

    /// Retry metadata is written only while this work is still current.
    fn set_incoming_retry(
        &self,
        topic: &StreamTopic,
        sequence: Cursor,
        retry: &IncomingRetry,
    ) -> Result<bool, StorageError> {
        let sequence = i64::try_from(sequence.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        stream_transaction(self, |conn| {
            if topic.kind != NetworkEntityKind::Welcome
                && first_pending(conn, topic)?.is_none_or(|head| head.sequence_id != sequence)
            {
                return Ok(false);
            }
            let target =
                incoming::table.find((&topic.entity_id, topic.kind.entity_kind(), sequence));
            let deadline = target
                .select(incoming::retry_expires_at_ns)
                .first::<Option<i64>>(conn)
                .optional()?;
            let Some(deadline) = deadline else {
                return Ok(false);
            };
            Ok(diesel::update(target)
                .set((
                    incoming::retry_at_ns.eq(retry.retry_at_ns),
                    incoming::blocked.eq(retry.blocked),
                    incoming::error_code.eq(&retry.error_code),
                    incoming::retry_expires_at_ns.eq(deadline.or(retry.retry_expires_at_ns)),
                ))
                .execute(conn)?
                > 0)
        })
    }

    /// Read due, unblocked Welcome rows. Production callers must also set a byte bound.
    fn ready_welcomes(
        &self,
        now_ns: i64,
        limit: u32,
    ) -> Result<Vec<StoredIncomingEnvelope>, StorageError> {
        self.ready_welcomes_bounded(now_ns, limit, u64::MAX)
    }

    /// Read a due prefix without loading envelope bytes above the batch budget.
    /// Permanently blocked rows require a new coordinator generation, not a timer retry.
    fn ready_welcomes_bounded(
        &self,
        now_ns: i64,
        limit: u32,
        max_bytes: u64,
    ) -> Result<Vec<StoredIncomingEnvelope>, StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                let due = || {
                    incoming::table
                        .filter(incoming::entity_kind.eq(EntityKind::Welcome))
                        .filter(incoming::blocked.eq(false))
                        .filter(incoming::retry_at_ns.le(now_ns))
                        .order((incoming::sequence_id.asc(), incoming::entity_id.asc()))
                };
                let sizes = due()
                    .select(diesel::dsl::sql::<BigInt>("length(envelope)"))
                    .limit(i64::from(limit))
                    .load::<i64>(conn)?;
                let mut selected_rows = 0_i64;
                let mut selected_bytes = 0_u64;
                for bytes in sizes {
                    let bytes = u64::try_from(bytes).map_err(|_| StorageError::DbDeserialize)?;
                    if bytes > max_bytes.saturating_sub(selected_bytes) {
                        if selected_rows == 0 {
                            return Err(StreamStorageError::Capacity {
                                scope: BudgetScope::Batch,
                            }
                            .into());
                        }
                        break;
                    }
                    selected_rows += 1;
                    selected_bytes += bytes;
                }
                Ok(due()
                    .limit(selected_rows)
                    .select(StoredIncomingEnvelope::as_select())
                    .load(conn)?)
            }))
        })?
    }

    /// Read one bounded page for a new coordinator generation to recheck unsupported work.
    fn blocked_welcomes_bounded(
        &self,
        topic: &StreamTopic,
        after: Cursor,
        limit: u32,
        max_bytes: u64,
    ) -> Result<Vec<StoredIncomingEnvelope>, StorageError> {
        if topic.kind != NetworkEntityKind::Welcome {
            return Err(StreamStorageError::InvalidBatch.into());
        }
        let after = i64::try_from(after.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                let pending = || {
                    incoming::table
                        .filter(incoming::entity_id.eq(&topic.entity_id))
                        .filter(incoming::entity_kind.eq(EntityKind::Welcome))
                        .filter(incoming::blocked.eq(true))
                        .filter(incoming::sequence_id.gt(after))
                        .order(incoming::sequence_id.asc())
                };
                let sizes = pending()
                    .select(diesel::dsl::sql::<BigInt>("length(envelope)"))
                    .limit(i64::from(limit))
                    .load::<i64>(conn)?;
                let mut rows = 0_i64;
                let mut bytes_used = 0_u64;
                for bytes in sizes {
                    let bytes = u64::try_from(bytes).map_err(|_| StorageError::DbDeserialize)?;
                    if bytes > max_bytes.saturating_sub(bytes_used) {
                        if rows == 0 {
                            return Err(StreamStorageError::Capacity {
                                scope: BudgetScope::Batch,
                            }
                            .into());
                        }
                        break;
                    }
                    rows += 1;
                    bytes_used += bytes;
                }
                Ok(pending().limit(rows).load(conn)?)
            }))
        })?
    }

    /// Later welcome success cannot hide an earlier unresolved welcome.
    fn welcome_barrier_complete(
        &self,
        topic: &StreamTopic,
        target: Cursor,
    ) -> Result<bool, StorageError> {
        if topic.kind != NetworkEntityKind::Welcome {
            return Err(StreamStorageError::InvalidBatch.into());
        }
        let target_id = i64::try_from(target.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                if load_progress(conn, topic)?.received < target {
                    return Ok(false);
                }
                let count = incoming::table
                    .filter(incoming::entity_id.eq(&topic.entity_id))
                    .filter(incoming::entity_kind.eq(EntityKind::Welcome))
                    .filter(incoming::sequence_id.le(target_id))
                    .select(diesel::dsl::count_star())
                    .first::<i64>(conn)?;
                Ok(count == 0)
            }))
        })?
    }

    /// Keep welcome private keys while any unresolved welcome can still need them.
    fn has_pending_welcomes(&self) -> Result<bool, StorageError> {
        self.raw_query(|conn| {
            incoming::table
                .filter(incoming::entity_kind.eq(EntityKind::Welcome))
                .select(diesel::dsl::count_star())
                .first::<i64>(conn)
                .map(|count| count > 0)
        })
        .map_err(Into::into)
    }

    /// Install a validated join anchor without rewinding either durable position.
    /// The caller must check the group state and install MLS state in the same transaction.
    fn install_group_anchor(
        &self,
        group_id: GroupId,
        anchor: Cursor,
        mode: JoinAnchorMode,
    ) -> Result<(), StorageError> {
        let anchor = i64::try_from(anchor.0).map_err(|_| StreamStorageError::InvalidBatch)?;
        let topic = StreamTopic::group(group_id);
        stream_transaction(self, |conn| {
            let state = progress::table
                .find((&topic.entity_id, EntityKind::ApplicationMessage))
                .select((progress::sequence_id, progress::received_sequence_id))
                .first::<(i64, Option<i64>)>(conn)
                .optional()?;
            match state {
                None => {
                    if mode == JoinAnchorMode::InactiveReadd {
                        return Err(StreamStorageError::StaleJoinAnchor.into());
                    }
                    diesel::insert_into(progress::table)
                        .values((
                            progress::entity_id.eq(&topic.entity_id),
                            progress::entity_kind.eq(EntityKind::ApplicationMessage),
                            progress::sequence_id.eq(anchor),
                            progress::received_sequence_id.eq(anchor),
                        ))
                        .execute(conn)?;
                }
                Some((processed, received)) => {
                    let valid = match mode {
                        JoinAnchorMode::Advance => anchor > processed,
                        JoinAnchorMode::InactiveReadd => anchor == processed,
                    };
                    if !valid {
                        return Err(StreamStorageError::StaleJoinAnchor.into());
                    }
                    let changed = diesel::update(
                        progress::table
                            .find((&topic.entity_id, EntityKind::ApplicationMessage))
                            .filter(progress::sequence_id.eq(processed)),
                    )
                    .set((
                        progress::sequence_id.eq(anchor),
                        progress::received_sequence_id.eq(received.unwrap_or(0).max(anchor)),
                    ))
                    .execute(conn)?;
                    if changed == 0 {
                        return Err(StreamStorageError::StaleJoinAnchor.into());
                    }
                }
            }
            diesel::delete(
                incoming::table
                    .filter(incoming::entity_id.eq(&topic.entity_id))
                    .filter(incoming::entity_kind.eq(EntityKind::ApplicationMessage))
                    .filter(incoming::sequence_id.le(anchor)),
            )
            .execute(conn)?;
            Ok(())
        })
    }

    /// Largest queues come first; empty topics are absent and must not be paused.
    fn pending_topic_usage(
        &self,
        kind: NetworkEntityKind,
    ) -> Result<Vec<PendingTopicUsage>, StorageError> {
        self.raw_query(|conn| pending_usage(conn, kind))
            .map_err(Into::into)
    }
}

impl<C: ConnectionExt> QueryIncomingEnvelope for C {}

fn check_budget(
    rows: u64,
    bytes: u64,
    limit: PendingBudget,
    scope: BudgetScope,
) -> Result<(), StorageError> {
    if rows > limit.rows || bytes > limit.bytes {
        return Err(StreamStorageError::Capacity { scope }.into());
    }
    Ok(())
}

fn initialize_progress(
    conn: &mut diesel::SqliteConnection,
    topic: &StreamTopic,
) -> Result<(), StorageError> {
    diesel::insert_or_ignore_into(progress::table)
        .values((
            progress::entity_id.eq(&topic.entity_id),
            progress::entity_kind.eq(topic.kind.entity_kind()),
            progress::sequence_id.eq(0i64),
            progress::received_sequence_id.eq(0i64),
        ))
        .execute(conn)?;
    // A zero row from a legacy reader has no history to lose.
    diesel::update(
        progress::table
            .find((&topic.entity_id, topic.kind.entity_kind()))
            .filter(progress::sequence_id.eq(0))
            .filter(progress::received_sequence_id.is_null()),
    )
    .set(progress::received_sequence_id.eq(0i64))
    .execute(conn)?;
    Ok(())
}

fn load_progress(
    conn: &mut diesel::SqliteConnection,
    topic: &StreamTopic,
) -> Result<TopicProgress, StorageError> {
    let row = progress::table
        .find((&topic.entity_id, topic.kind.entity_kind()))
        .select((progress::sequence_id, progress::received_sequence_id))
        .first::<(i64, Option<i64>)>(conn)
        .optional()?;
    match row {
        None | Some((0, None)) => Ok(TopicProgress::default()),
        Some((processed, Some(received))) => Ok(TopicProgress {
            processed: Cursor(processed as u64),
            received: Cursor(received as u64),
        }),
        Some(_) => Err(StreamStorageError::UninitializedNetworkProgress.into()),
    }
}

fn first_pending(
    conn: &mut diesel::SqliteConnection,
    topic: &StreamTopic,
) -> QueryResult<Option<StoredIncomingEnvelope>> {
    incoming::table
        .filter(incoming::entity_id.eq(&topic.entity_id))
        .filter(incoming::entity_kind.eq(topic.kind.entity_kind()))
        .order(incoming::sequence_id.asc())
        .first(conn)
        .optional()
}

fn pending_usage(
    conn: &mut diesel::SqliteConnection,
    kind: NetworkEntityKind,
) -> QueryResult<Vec<PendingTopicUsage>> {
    diesel::sql_query("SELECT entity_id, COUNT(*) AS rows, SUM(length(envelope)) AS bytes FROM incoming_envelopes WHERE entity_kind = ? GROUP BY entity_id ORDER BY rows DESC, entity_id ASC")
        .bind::<Integer, _>(kind.entity_kind() as i32).load(conn)
}

#[cfg(test)]
mod tests;
