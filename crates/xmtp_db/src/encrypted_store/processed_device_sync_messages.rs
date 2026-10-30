use super::{
    ConnectionExt, Sqlite,
    db_connection::DbConnection,
    group::ConversationType,
    group_message::StoredGroupMessage,
    schema::{
        group_messages::dsl as group_messages_dsl,
        groups::dsl as groups_dsl,
        processed_device_sync_messages::{self, dsl},
    },
};
use crate::{StorageError, impl_store, impl_store_or_ignore};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use xmtp_proto::types::GroupId;

/// The worker loads unprocessed sync messages in pages of this many rows.
pub const SYNC_MESSAGE_PAGE: i64 = 20;

/// The last row of a page: (group_id, sent_at_ns, id).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncMessageKey {
    pub group_id: GroupId,
    pub sent_at_ns: i64,
    pub id: Vec<u8>,
}

impl SyncMessageKey {
    pub fn of(message: &StoredGroupMessage) -> Self {
        Self {
            group_id: message.group_id,
            sent_at_ns: message.sent_at_ns,
            id: message.id.clone(),
        }
    }
}

/// The state of a device sync message processing
#[repr(i32)]
#[derive(
    Debug, Default, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow,
)]
#[diesel(sql_type = Integer)]
pub enum DeviceSyncProcessingState {
    /// Message is pending processing
    #[default]
    Pending = 0,
    /// Message has been successfully processed
    Processed = 1,
    /// Message processing failed permanently
    Failed = 2,
}

impl ToSql<Integer, Sqlite> for DeviceSyncProcessingState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for DeviceSyncProcessingState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(DeviceSyncProcessingState::Pending),
            1 => Ok(DeviceSyncProcessingState::Processed),
            2 => Ok(DeviceSyncProcessingState::Failed),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Insertable, Identifiable, Queryable)]
#[diesel(table_name = processed_device_sync_messages)]
#[diesel(primary_key(message_id))]
pub struct StoredProcessedDeviceSyncMessages {
    pub message_id: Vec<u8>,
    /// Number of processing attempts remaining
    pub attempts: i32,
    /// Current processing state
    pub state: DeviceSyncProcessingState,
}

impl StoredProcessedDeviceSyncMessages {
    /// Create a new stored processed device sync message with default values
    pub fn new(message_id: Vec<u8>) -> Self {
        Self {
            message_id,
            attempts: 0,
            state: DeviceSyncProcessingState::Pending,
        }
    }
}

impl_store!(
    StoredProcessedDeviceSyncMessages,
    processed_device_sync_messages
);
impl_store_or_ignore!(
    StoredProcessedDeviceSyncMessages,
    processed_device_sync_messages
);

pub trait QueryDeviceSyncMessages {
    /// Up to [`SYNC_MESSAGE_PAGE`] unprocessed sync messages after `after`,
    /// in (group_id, sent_at_ns, id) order.
    fn unprocessed_sync_group_messages(
        &self,
        after: Option<&SyncMessageKey>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError>;
    fn sync_group_messages_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<StoredGroupMessage>, StorageError>;
    /// Marks a device sync message as processed.
    fn mark_device_sync_msg_as_processed(&self, message_id: &[u8]) -> Result<(), StorageError>;
    /// This installation's sync messages that are not processed yet.
    fn own_unprocessed_sync_messages(
        &self,
        installation_id: &[u8],
    ) -> Result<Vec<StoredGroupMessage>, StorageError>;
}

impl<T> QueryDeviceSyncMessages for &T
where
    T: QueryDeviceSyncMessages,
{
    fn unprocessed_sync_group_messages(
        &self,
        after: Option<&SyncMessageKey>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        (**self).unprocessed_sync_group_messages(after)
    }

    fn sync_group_messages_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        (**self).sync_group_messages_paged(offset, limit)
    }

    fn mark_device_sync_msg_as_processed(&self, message_id: &[u8]) -> Result<(), StorageError> {
        (**self).mark_device_sync_msg_as_processed(message_id)
    }

    fn own_unprocessed_sync_messages(
        &self,
        installation_id: &[u8],
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        (**self).own_unprocessed_sync_messages(installation_id)
    }
}

impl<C: ConnectionExt> QueryDeviceSyncMessages for DbConnection<C> {
    fn own_unprocessed_sync_messages(
        &self,
        installation_id: &[u8],
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let processed = dsl::processed_device_sync_messages
            .filter(dsl::message_id.eq(group_messages_dsl::id))
            .filter(dsl::state.ne(DeviceSyncProcessingState::Pending));
        let query = group_messages_dsl::group_messages
            .inner_join(groups_dsl::groups.on(group_messages_dsl::group_id.eq(groups_dsl::id)))
            .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
            .filter(group_messages_dsl::sender_installation_id.eq(installation_id))
            .filter(diesel::dsl::not(diesel::dsl::exists(processed)))
            .order((group_messages_dsl::sent_at_ns, group_messages_dsl::id))
            .select(StoredGroupMessage::as_select());
        Ok(self.raw_query(|conn| query.load(conn))?)
    }

    fn unprocessed_sync_group_messages(
        &self,
        after: Option<&SyncMessageKey>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        // One sync group at a time, so each read is a range of
        // `group_messages_sent_at_id_sort` with no sort.
        let result = self.raw_query(|conn| {
            let mut groups = groups_dsl::groups
                .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
                .select(groups_dsl::id)
                .order(groups_dsl::id)
                .into_boxed();
            if let Some(after) = after {
                groups = groups.filter(groups_dsl::id.ge(after.group_id));
            }
            let mut page = vec![];
            for group_id in groups.load::<GroupId>(conn)? {
                let mut query = group_messages_dsl::group_messages
                    .filter(group_messages_dsl::group_id.eq(group_id))
                    // Include messages that either:
                    // 1. Don't have an entry in processed_device_sync_messages, OR
                    // 2. Have an entry with state = Pending
                    .filter(
                        diesel::dsl::not(diesel::dsl::exists(
                            dsl::processed_device_sync_messages
                                .filter(dsl::message_id.eq(group_messages_dsl::id)),
                        ))
                        .or(diesel::dsl::exists(
                            dsl::processed_device_sync_messages
                                .filter(dsl::message_id.eq(group_messages_dsl::id))
                                .filter(dsl::state.eq(DeviceSyncProcessingState::Pending)),
                        )),
                    )
                    .into_boxed();
                if let Some(k) = after.filter(|k| k.group_id == group_id) {
                    // The `ge` bound lets SQLite start the index range at the cursor.
                    query = query
                        .filter(group_messages_dsl::sent_at_ns.ge(k.sent_at_ns))
                        .filter(
                            group_messages_dsl::sent_at_ns.gt(k.sent_at_ns).or(
                                group_messages_dsl::sent_at_ns
                                    .eq(k.sent_at_ns)
                                    .and(group_messages_dsl::id.gt(&k.id)),
                            ),
                        );
                }
                page.extend(
                    query
                        .order((group_messages_dsl::sent_at_ns, group_messages_dsl::id))
                        .limit(SYNC_MESSAGE_PAGE - page.len() as i64)
                        .select(StoredGroupMessage::as_select())
                        .load::<StoredGroupMessage>(conn)?,
                );
                if page.len() as i64 == SYNC_MESSAGE_PAGE {
                    break;
                }
            }
            Ok(page)
        })?;
        Ok(result)
    }

    fn sync_group_messages_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let result = self.raw_query(|conn| {
            group_messages_dsl::group_messages
                .inner_join(groups_dsl::groups.on(group_messages_dsl::group_id.eq(groups_dsl::id)))
                .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
                .select(StoredGroupMessage::as_select())
                .order_by(group_messages_dsl::sent_at_ns.desc())
                .limit(limit)
                .offset(offset)
                .load::<StoredGroupMessage>(conn)
        })?;
        Ok(result)
    }

    fn mark_device_sync_msg_as_processed(&self, message_id: &[u8]) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(dsl::processed_device_sync_messages)
                .values(StoredProcessedDeviceSyncMessages {
                    message_id: message_id.to_vec(),
                    attempts: 0,
                    state: DeviceSyncProcessingState::Processed,
                })
                .on_conflict(dsl::message_id)
                .do_update()
                .set(dsl::state.eq(DeviceSyncProcessingState::Processed))
                .execute(conn)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Store,
        group::{ConversationType, tests::generate_group},
        group_message::tests::generate_message,
        test_utils::with_connection,
    };

    #[xmtp_common::test(unwrap_try = true)]
    fn unprocessed_sync_messages_page_through_every_group_once() {
        with_connection(|conn| {
            let page = SYNC_MESSAGE_PAGE as usize;
            let [mut first, mut second, mut dm] = [(); 3].map(|_| generate_group(None));
            if second.id < first.id {
                std::mem::swap(&mut first, &mut second);
            }
            first.conversation_type = ConversationType::Sync;
            second.conversation_type = ConversationType::Sync;
            dm.conversation_type = ConversationType::Dm;
            let mut expected = vec![];
            for (group, rows) in [(&mut first, page + 5), (&mut second, 7), (&mut dm, 1)] {
                group.store(conn)?;
                for i in 0..rows as i64 {
                    // Rows page-3..=page+3 of the first group share one send time.
                    let tie = SYNC_MESSAGE_PAGE - 3..=SYNC_MESSAGE_PAGE + 3;
                    let sent_at_ns = if tie.contains(&i) {
                        SYNC_MESSAGE_PAGE
                    } else {
                        i
                    };
                    let msg =
                        generate_message(None, Some(&group.id), Some(sent_at_ns), None, None, None);
                    msg.store(conn)?;
                    expected.push(msg.id);
                }
            }
            expected.pop(); // The DM row.
            let processed = generate_message(None, Some(&first.id), Some(0), None, None, None);
            processed.store(conn)?;
            conn.mark_device_sync_msg_as_processed(&processed.id)?;
            let pending = expected[1].clone();
            StoredProcessedDeviceSyncMessages::new(pending.clone()).store(conn)?;

            let page_through = || -> Result<Vec<Vec<u8>>, StorageError> {
                let (mut seen, mut after) = (std::collections::HashSet::new(), None);
                loop {
                    let rows = conn.unprocessed_sync_group_messages(after.as_ref())?;
                    assert!(rows.len() <= page, "{} rows in one page", rows.len());
                    let Some(last) = rows.last() else { break };
                    after = Some(SyncMessageKey::of(last));
                    for row in rows {
                        assert!(seen.insert(row.id), "a row came back twice");
                    }
                }
                let mut seen: Vec<_> = seen.into_iter().collect();
                seen.sort();
                Ok(seen)
            };
            expected.sort();
            assert_eq!(page_through()?, expected);
            conn.mark_device_sync_msg_as_processed(&pending)?;
            expected.retain(|id| *id != pending);
            assert_eq!(page_through()?, expected);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn it_returns_sync_group_messages_paged() {
        with_connection(|conn| {
            let mut sync_group = generate_group(None);
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn)?;

            // Create a non-sync group to verify filtering works
            let mut dm_group = generate_group(None);
            dm_group.conversation_type = ConversationType::Dm;
            dm_group.store(conn)?;

            // Create 5 messages in the sync group with specific sent_at_ns values
            // Messages are ordered by sent_at_ns DESC, so we store IDs in reverse order
            let mut sync_message_ids = Vec::new();
            for i in 0..5 {
                let message = generate_message(
                    None,
                    Some(&sync_group.id),
                    Some(((5 - i) * 1000) as i64),
                    None,
                    None,
                    None,
                );
                message.store(conn)?;
                sync_message_ids.push(message.id);
            }

            // Create a message in the non-sync group (should be filtered out)
            let dm_message = generate_message(None, Some(&dm_group.id), None, None, None, None);
            dm_message.store(conn)?;

            // Test pagination: get first 2 messages
            let page1 = conn.sync_group_messages_paged(0, 2)?;
            assert_eq!(page1.len(), 2);
            assert_eq!(page1[0].id, sync_message_ids[0]);
            assert_eq!(page1[1].id, sync_message_ids[1]);

            // Test pagination: get next 2 messages
            let page2 = conn.sync_group_messages_paged(2, 2)?;
            assert_eq!(page2.len(), 2);
            assert_eq!(page2[0].id, sync_message_ids[2]);
            assert_eq!(page2[1].id, sync_message_ids[3]);

            // Test pagination: get last message
            let page3 = conn.sync_group_messages_paged(4, 2)?;
            assert_eq!(page3.len(), 1);
            assert_eq!(page3[0].id, sync_message_ids[4]);

            // Test pagination: offset beyond available messages
            let page4 = conn.sync_group_messages_paged(10, 2)?;
            assert_eq!(page4.len(), 0);

            // Test getting all messages at once
            let all_messages = conn.sync_group_messages_paged(0, 100)?;
            assert_eq!(all_messages.len(), 5);

            // Verify all returned messages are in order and belong to the sync group
            for (i, msg) in all_messages.iter().enumerate() {
                assert_eq!(msg.id, sync_message_ids[i]);
                assert_eq!(msg.group_id.as_slice(), sync_group.id.as_slice());
            }
        })
    }
}
