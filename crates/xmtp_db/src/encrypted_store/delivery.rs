//! Database-local message order and fenced default-consumer progress.

use diesel::{
    prelude::*,
    sql_types::{BigInt, Integer, Nullable},
};
use serde::{Deserialize, Serialize};
use xmtp_proto::types::GroupId;

use super::{
    consent_record::{ConsentState, ConsentType},
    group::ConversationType,
    group_message::{DeliveryStatus, StoredGroupMessage},
    refresh_state::EntityKind,
    schema::{
        group_messages as messages, groups, refresh_state as progress,
        user_preferences as preferences,
    },
    stream_storage::{StreamStorageError, stream_transaction},
};
use crate::{ConnectionExt, NotFound, StorageError};

const PREFERENCES_ID: i32 = 0;
const ALLOCATOR_ID: &[u8] = &[];

/// An exclusive replay position in one client database, not a network cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCursor {
    /// Stable across reopen. Whole-database restore rotates it to reject stale cursors.
    pub database_id: [u8; 16],
    /// Immutable local order. Zero precedes all messages; deleted values are never reused.
    pub delivery_sequence: u64,
}

/// Random lease token that fences every default-delivery progress write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryOwner(pub [u8; 16]);

/// Groups eligible for selection. Default progress outside this scope stays unchanged.
#[derive(Debug, Clone, Default)]
pub enum DeliveryScope {
    #[default]
    All,
    Groups(Vec<GroupId>),
}

/// Current selection filters; excluded rows can advance D only within the active scope.
#[derive(Debug, Clone, Default)]
pub struct DeliveryFilter {
    pub conversation_type: Option<ConversationType>,
    pub consent_states: Option<Vec<ConsentState>>,
}

/// One retained, published message and its immutable local replay position.
#[derive(Debug, Clone)]
pub struct DeliveryMessage {
    pub message: StoredGroupMessage,
    pub cursor: DeliveryCursor,
}

/// History and its resume cursor read from one database snapshot.
#[derive(Debug, Clone)]
pub struct DeliverySnapshot {
    pub messages: Vec<DeliveryMessage>,
    /// Covers the snapshot, even when filters or limits omit some older messages.
    pub cursor: DeliveryCursor,
}

/// Local message order, retained-history reads, and fenced per-group D positions.
pub trait QueryDelivery: ConnectionExt + Sized {
    /// Read the database identity used to reject foreign or pre-restore cursors.
    fn stream_database_id(&self) -> Result<[u8; 16], StorageError> {
        self.raw_query(|conn| Ok(database_id(conn)))?
    }

    /// Rotate only under exclusive restore lifecycle access. Existing consumer tokens are fenced.
    fn rotate_stream_database_id(&self) -> Result<[u8; 16], StorageError> {
        let identity = xmtp_common::rand_array::<16>();
        stream_transaction(self, |conn| {
            diesel::update(preferences::table.find(PREFERENCES_ID))
                .set((
                    preferences::stream_database_id.eq(identity.as_slice()),
                    preferences::delivery_owner.eq(None::<Vec<u8>>),
                    preferences::delivery_owner_until_ns.eq(None::<i64>),
                ))
                .execute(conn)?;
            Ok(identity)
        })
    }

    /// Allocate within the transaction that makes a message deliverable.
    /// A duplicate keeps its number. An optimistic unpublished row has no number.
    fn assign_delivery_sequence(&self, message_id: &[u8]) -> Result<Option<u64>, StorageError> {
        stream_transaction(self, |conn| assign_sequence(conn, message_id))
    }

    /// Read the persistent allocator, not the maximum remaining message row.
    fn current_delivery_cursor(&self) -> Result<DeliveryCursor, StorageError> {
        self.raw_query(|conn| Ok(conn.transaction::<_, StorageError, _>(current_cursor)))?
    }

    /// Acquire the sole default-consumer lease at a supplied time; fail if one is active.
    fn acquire_delivery_owner(
        &self,
        now_ns: i64,
        until_ns: i64,
    ) -> Result<DeliveryOwner, StorageError> {
        let lease_ns = until_ns
            .checked_sub(now_ns)
            .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
        self.acquire_delivery_owner_with_clock(lease_ns, || now_ns)
    }

    /// Read the clock after the writer is acquired, not before a possible lock wait.
    fn acquire_delivery_owner_with_clock(
        &self,
        lease_ns: i64,
        clock: impl FnOnce() -> i64,
    ) -> Result<DeliveryOwner, StorageError> {
        if lease_ns <= 0 {
            return Err(StreamStorageError::InvalidDeliveryPosition.into());
        }
        let owner = DeliveryOwner(xmtp_common::rand_array::<16>());
        stream_transaction(self, |conn| {
            let now_ns = clock();
            let until_ns = now_ns
                .checked_add(lease_ns)
                .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
            let changed = diesel::update(
                preferences::table.find(PREFERENCES_ID).filter(
                    preferences::delivery_owner
                        .is_null()
                        .or(preferences::delivery_owner_until_ns.le(now_ns)),
                ),
            )
            .set((
                preferences::delivery_owner.eq(owner.0.as_slice()),
                preferences::delivery_owner_until_ns.eq(until_ns),
            ))
            .execute(conn)?;
            if changed == 0 {
                return Err(StreamStorageError::AlreadyActive.into());
            }
            Ok(owner)
        })
    }

    /// An expired token cannot be renewed. The caller must acquire a new token.
    fn renew_delivery_owner(
        &self,
        owner: DeliveryOwner,
        now_ns: i64,
        until_ns: i64,
    ) -> Result<(), StorageError> {
        let lease_ns = until_ns
            .checked_sub(now_ns)
            .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
        self.renew_delivery_owner_with_clock(owner, lease_ns, || now_ns)
    }

    /// Extend only the current, unexpired token using time read after the writer lock.
    fn renew_delivery_owner_with_clock(
        &self,
        owner: DeliveryOwner,
        lease_ns: i64,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), StorageError> {
        if lease_ns <= 0 {
            return Err(StreamStorageError::InvalidDeliveryPosition.into());
        }
        stream_transaction(self, |conn| {
            let now_ns = clock();
            let until_ns = now_ns
                .checked_add(lease_ns)
                .ok_or(StreamStorageError::InvalidDeliveryPosition)?;
            let changed = diesel::update(
                preferences::table
                    .find(PREFERENCES_ID)
                    .filter(preferences::delivery_owner.eq(owner.0.as_slice()))
                    .filter(preferences::delivery_owner_until_ns.gt(now_ns)),
            )
            .set(preferences::delivery_owner_until_ns.eq(until_ns))
            .execute(conn)?;
            if changed == 0 {
                return Err(StreamStorageError::NotCurrentOwner.into());
            }
            Ok(())
        })
    }

    /// Reject expired or replaced tokens before handing a message to the app.
    fn check_delivery_owner(&self, owner: DeliveryOwner, now_ns: i64) -> Result<(), StorageError> {
        self.check_delivery_owner_with_clock(owner, || now_ns)
    }

    /// Check ownership using fresh time after the database connection is available.
    fn check_delivery_owner_with_clock(
        &self,
        owner: DeliveryOwner,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| Ok(check_owner(conn, owner, clock())))?
    }

    /// Release this token only; a stale consumer cannot release its replacement.
    fn release_delivery_owner(&self, owner: DeliveryOwner) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(
                preferences::table
                    .find(PREFERENCES_ID)
                    .filter(preferences::delivery_owner.eq(owner.0.as_slice())),
            )
            .set((
                preferences::delivery_owner.eq(None::<Vec<u8>>),
                preferences::delivery_owner_until_ns.eq(None::<i64>),
            ))
            .execute(conn)
        })?;
        Ok(())
    }

    /// A buffered candidate can expire or be deleted while its previous item is held.
    fn delivery_message_is_retained(
        &self,
        message_id: &[u8],
        cursor: DeliveryCursor,
        now_ns: i64,
    ) -> Result<bool, StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                validate_cursor(conn, cursor)?;
                Ok(diesel::select(diesel::dsl::exists(
                    messages::table
                        .find(message_id)
                        .filter(messages::delivery_sequence.eq(cursor.delivery_sequence as i64))
                        .filter(messages::delivery_status.eq(DeliveryStatus::Published))
                        .filter(
                            messages::expire_at_ns
                                .is_null()
                                .or(messages::expire_at_ns.gt(now_ns)),
                        ),
                ))
                .get_result::<bool>(conn)?)
            }))
        })?
    }

    /// Read retained candidates above each group's default position.
    /// The caller applies consent/type filters and acknowledges scanned rows by the same owner.
    fn default_delivery_messages(
        &self,
        owner: DeliveryOwner,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
    ) -> Result<Vec<DeliveryMessage>, StorageError> {
        self.default_delivery_messages_bounded(owner, scope, now_ns, limit, u64::MAX)
    }

    /// Bound rows and bytes before loading message bodies; this read never advances D.
    fn default_delivery_messages_bounded(
        &self,
        owner: DeliveryOwner,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
        max_bytes: u64,
    ) -> Result<Vec<DeliveryMessage>, StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                check_owner(conn, owner, now_ns)?;
                read_messages(
                    conn,
                    scope,
                    DeliveryReadOptions {
                        after: None,
                        filter: None,
                        now_ns,
                        limit,
                        max_bytes,
                        descending: false,
                    },
                )
            }))
        })?
    }

    /// Explicit replay does not read or write default delivery positions.
    fn replay_delivery_messages(
        &self,
        after: DeliveryCursor,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
    ) -> Result<Vec<DeliveryMessage>, StorageError> {
        self.replay_delivery_messages_bounded(after, scope, now_ns, limit, u64::MAX)
    }

    /// Read a bounded retained prefix strictly after the cursor, without an owner or D writes.
    fn replay_delivery_messages_bounded(
        &self,
        after: DeliveryCursor,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
        max_bytes: u64,
    ) -> Result<Vec<DeliveryMessage>, StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                validate_cursor(conn, after)?;
                read_messages(
                    conn,
                    scope,
                    DeliveryReadOptions {
                        after: Some(after.delivery_sequence),
                        filter: None,
                        now_ns,
                        limit,
                        max_bytes,
                        descending: false,
                    },
                )
            }))
        })?
    }

    /// Return recent retained history and a cursor from the same database snapshot.
    fn delivery_history_snapshot(
        &self,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
    ) -> Result<DeliverySnapshot, StorageError> {
        self.delivery_history_snapshot_bounded(scope, now_ns, limit, u64::MAX)
    }

    /// Bound history allocation while capturing its resume cursor in the same snapshot.
    fn delivery_history_snapshot_bounded(
        &self,
        scope: &DeliveryScope,
        now_ns: i64,
        limit: u32,
        max_bytes: u64,
    ) -> Result<DeliverySnapshot, StorageError> {
        self.delivery_history_snapshot_filtered(
            scope,
            &DeliveryFilter::default(),
            now_ns,
            limit,
            max_bytes,
        )
    }

    /// Apply history filters before its limit, with history and cursor in one read transaction.
    fn delivery_history_snapshot_filtered(
        &self,
        scope: &DeliveryScope,
        filter: &DeliveryFilter,
        now_ns: i64,
        limit: u32,
        max_bytes: u64,
    ) -> Result<DeliverySnapshot, StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                let cursor = current_cursor(conn)?;
                let mut messages = read_messages(
                    conn,
                    scope,
                    DeliveryReadOptions {
                        after: Some(0),
                        filter: Some(filter),
                        now_ns,
                        limit,
                        max_bytes,
                        descending: true,
                    },
                )?;
                messages.reverse();
                Ok(DeliverySnapshot { messages, cursor })
            }))
        })?
    }

    /// Acknowledge after callback return or the next iterator request, never on queue insertion.
    /// This also fences progress for rows excluded by a consent/type filter.
    fn acknowledge_delivery(
        &self,
        owner: DeliveryOwner,
        group_id: GroupId,
        cursor: DeliveryCursor,
        now_ns: i64,
    ) -> Result<(), StorageError> {
        self.acknowledge_delivery_with_clock(owner, group_id, cursor, || now_ns)
    }

    /// Advance this group's D only after a fresh owner check under the state writer.
    fn acknowledge_delivery_with_clock(
        &self,
        owner: DeliveryOwner,
        group_id: GroupId,
        cursor: DeliveryCursor,
        clock: impl FnOnce() -> i64,
    ) -> Result<(), StorageError> {
        stream_transaction(self, |conn| {
            check_owner(conn, owner, clock())?;
            validate_cursor(conn, cursor)?;
            use diesel::{query_dsl::methods::FilterDsl, upsert::excluded};
            diesel::insert_into(progress::table)
                .values((
                    progress::entity_id.eq(group_id.as_ref()),
                    progress::entity_kind.eq(EntityKind::Delivery),
                    progress::sequence_id.eq(cursor.delivery_sequence as i64),
                ))
                .on_conflict((progress::entity_id, progress::entity_kind))
                .do_update()
                .set(progress::sequence_id.eq(excluded(progress::sequence_id)))
                .filter(progress::sequence_id.lt(excluded(progress::sequence_id)))
                .execute(conn)?;
            Ok(())
        })
    }
}

impl<C: ConnectionExt> QueryDelivery for C {}

/// The caller holds the writer and has made the message visible in this transaction.
pub(crate) fn assign_sequence(
    conn: &mut diesel::SqliteConnection,
    message_id: &[u8],
) -> Result<Option<u64>, StorageError> {
    let (sequence, status) = messages::table
        .find(message_id)
        .select((messages::delivery_sequence, messages::delivery_status))
        .first::<(Option<i64>, DeliveryStatus)>(conn)
        .optional()?
        .ok_or_else(|| NotFound::MessageById(message_id.to_vec()))?;
    if sequence.is_some() || status != DeliveryStatus::Published {
        return Ok(sequence.map(|value| value as u64));
    }
    let sequence = diesel::update(
        progress::table
            .find((ALLOCATOR_ID, EntityKind::DeliveryAllocator))
            .filter(progress::sequence_id.lt(i64::MAX)),
    )
    .set(progress::sequence_id.eq(progress::sequence_id + 1))
    .returning(progress::sequence_id)
    .get_result::<i64>(conn)
    .optional()?
    .ok_or(StreamStorageError::DeliveryExhausted)?;
    diesel::update(
        messages::table
            .find(message_id)
            .filter(messages::delivery_sequence.is_null()),
    )
    .set(messages::delivery_sequence.eq(sequence))
    .execute(conn)?;
    Ok(Some(sequence as u64))
}

fn database_id(conn: &mut diesel::SqliteConnection) -> Result<[u8; 16], StorageError> {
    preferences::table
        .find(PREFERENCES_ID)
        .select(preferences::stream_database_id)
        .first::<Vec<u8>>(conn)?
        .try_into()
        .map_err(|_| StorageError::DbDeserialize)
}

fn current_cursor(conn: &mut diesel::SqliteConnection) -> Result<DeliveryCursor, StorageError> {
    let sequence = progress::table
        .find((ALLOCATOR_ID, EntityKind::DeliveryAllocator))
        .select(progress::sequence_id)
        .first::<i64>(conn)?;
    Ok(DeliveryCursor {
        database_id: database_id(conn)?,
        delivery_sequence: sequence as u64,
    })
}

fn validate_cursor(
    conn: &mut diesel::SqliteConnection,
    cursor: DeliveryCursor,
) -> Result<(), StorageError> {
    let current = current_cursor(conn)?;
    if cursor.database_id != current.database_id {
        return Err(StreamStorageError::ForeignCursor.into());
    }
    if cursor.delivery_sequence > current.delivery_sequence {
        return Err(StreamStorageError::InvalidDeliveryPosition.into());
    }
    Ok(())
}

fn check_owner(
    conn: &mut diesel::SqliteConnection,
    owner: DeliveryOwner,
    now_ns: i64,
) -> Result<(), StorageError> {
    let valid = preferences::table
        .find(PREFERENCES_ID)
        .filter(preferences::delivery_owner.eq(owner.0.as_slice()))
        .filter(preferences::delivery_owner_until_ns.gt(now_ns))
        .select(preferences::id)
        .first::<i32>(conn)
        .optional()?
        .is_some();
    if !valid {
        return Err(StreamStorageError::NotCurrentOwner.into());
    }
    Ok(())
}

/// Selection and allocation bounds for one retained-message read.
struct DeliveryReadOptions<'a> {
    /// An absent replay position selects each group's saved default progress.
    after: Option<u64>,
    filter: Option<&'a DeliveryFilter>,
    now_ns: i64,
    limit: u32,
    max_bytes: u64,
    descending: bool,
}

/// Select scalar sizes first so message bodies cannot exceed the read budget.
fn read_messages(
    conn: &mut diesel::SqliteConnection,
    scope: &DeliveryScope,
    options: DeliveryReadOptions<'_>,
) -> Result<Vec<DeliveryMessage>, StorageError> {
    let DeliveryReadOptions {
        after,
        filter,
        now_ns,
        limit,
        max_bytes,
        descending,
    } = options;
    // Read only scalar lengths first. Large blobs never enter the candidate buffer before the budget check.
    let row_bytes = diesel::dsl::sql::<BigInt>(
        "length(group_messages.id) + length(group_messages.group_id) + length(group_messages.decrypted_message_bytes) + \
        length(group_messages.sender_installation_id) + length(CAST(group_messages.sender_inbox_id AS BLOB)) + \
        length(CAST(group_messages.authority_id AS BLOB)) + COALESCE(length(group_messages.reference_id), 0) + \
        COALESCE(length(group_messages.envelope_hash), 0) + length(CAST(group_messages.idempotency_key AS BLOB)) + ")
        .bind::<BigInt, _>(std::mem::size_of::<DeliveryMessage>() as i64);
    let mut query = messages::table
        .inner_join(groups::table)
        .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
        .filter(messages::delivery_sequence.is_not_null())
        .filter(messages::delivery_status.eq(DeliveryStatus::Published))
        .filter(
            messages::expire_at_ns
                .is_null()
                .or(messages::expire_at_ns.gt(now_ns)),
        )
        .select((messages::delivery_sequence.assume_not_null(), row_bytes))
        .into_boxed();
    if let DeliveryScope::Groups(groups) = scope {
        query = query.filter(messages::group_id.eq_any(groups));
    }
    if let Some(filter) = filter {
        if let Some(kind) = filter.conversation_type {
            query = query.filter(groups::conversation_type.eq(kind));
        }
        if let Some(states) = &filter.consent_states {
            let consent = diesel::dsl::sql::<Integer>(
                "COALESCE((SELECT state FROM consent_records WHERE entity_type = ",
            )
            .bind::<Integer, _>(ConsentType::ConversationId as i32)
            .sql(" AND entity = lower(hex(group_messages.group_id))), ")
            .bind::<Integer, _>(ConsentState::Unknown as i32)
            .sql(")");
            query = query.filter(
                consent.eq_any(states.iter().map(|state| *state as i32).collect::<Vec<_>>()),
            );
        }
    }
    if let Some(after) = after {
        query = query.filter(messages::delivery_sequence.gt(after as i64));
    } else {
        query = query.filter(
            messages::delivery_sequence.gt(diesel::dsl::sql::<Nullable<BigInt>>(
                "COALESCE((SELECT sequence_id FROM refresh_state WHERE entity_kind = ",
            )
            .bind::<Integer, _>(EntityKind::Delivery as i32)
            .sql(" AND entity_id = group_messages.group_id), 0)")),
        );
    }
    query = if descending {
        query.order(messages::delivery_sequence.desc())
    } else {
        query.order(messages::delivery_sequence.asc())
    };
    let sizes = query.limit(i64::from(limit)).load::<(i64, i64)>(conn)?;
    let mut sequences = Vec::with_capacity(sizes.len());
    let mut total_bytes = 0_u64;
    for (sequence, bytes) in sizes {
        let bytes = u64::try_from(bytes).map_err(|_| StorageError::DbDeserialize)?;
        if bytes > max_bytes.saturating_sub(total_bytes) {
            if sequences.is_empty() {
                return Err(StreamStorageError::LocalReadCapacity {
                    bytes,
                    limit: max_bytes,
                }
                .into());
            }
            break;
        }
        total_bytes += bytes;
        sequences.push(sequence);
    }
    let mut query = messages::table
        .filter(messages::delivery_sequence.eq_any(sequences))
        .select((
            StoredGroupMessage::as_select(),
            messages::delivery_sequence.assume_not_null(),
        ))
        .into_boxed();
    query = if descending {
        query.order(messages::delivery_sequence.desc())
    } else {
        query.order(messages::delivery_sequence.asc())
    };
    let database_id = database_id(conn)?;
    let rows = query.load::<(StoredGroupMessage, i64)>(conn)?;
    Ok(rows
        .into_iter()
        .map(|(message, sequence)| DeliveryMessage {
            message,
            cursor: DeliveryCursor {
                database_id,
                delivery_sequence: sequence as u64,
            },
        })
        .collect())
}

#[cfg(test)]
mod tests;
