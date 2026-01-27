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
    /// Maximum number of attempts before giving up on processing a device sync message
    pub const MAX_ATTEMPTS: i32 = 3;

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
    fn unprocessed_sync_group_messages(&self) -> Result<Vec<StoredGroupMessage>, StorageError>;
    fn sync_group_messages_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<StoredGroupMessage>, StorageError>;
    /// Marks a device sync message as processed.
    fn mark_device_sync_msg_as_processed(&self, message_id: &[u8]) -> Result<(), StorageError>;
    /// Increments the attempt count for a device sync message.
    /// If the attempt count reaches MAX_ATTEMPTS, the state is set to Failed.
    /// Returns the new attempt count.
    fn increment_device_sync_msg_attempt(&self, message_id: &[u8]) -> Result<i32, StorageError>;
}

impl<T> QueryDeviceSyncMessages for &T
where
    T: QueryDeviceSyncMessages,
{
    fn unprocessed_sync_group_messages(&self) -> Result<Vec<StoredGroupMessage>, StorageError> {
        (**self).unprocessed_sync_group_messages()
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

    fn increment_device_sync_msg_attempt(&self, message_id: &[u8]) -> Result<i32, StorageError> {
        (**self).increment_device_sync_msg_attempt(message_id)
    }
}

impl<C: ConnectionExt> QueryDeviceSyncMessages for DbConnection<C> {
    fn unprocessed_sync_group_messages(&self) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let result = self.raw_query_read(|conn| {
            group_messages_dsl::group_messages
                .inner_join(groups_dsl::groups.on(group_messages_dsl::group_id.eq(groups_dsl::id)))
                .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
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
                .select(group_messages_dsl::group_messages::all_columns())
                .load::<StoredGroupMessage>(conn)
        })?;
        Ok(result)
    }

    fn sync_group_messages_paged(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let result = self.raw_query_read(|conn| {
            group_messages_dsl::group_messages
                .inner_join(groups_dsl::groups.on(group_messages_dsl::group_id.eq(groups_dsl::id)))
                .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
                .select(group_messages_dsl::group_messages::all_columns())
                .order_by(group_messages_dsl::sent_at_ns.desc())
                .limit(limit)
                .offset(offset)
                .load::<StoredGroupMessage>(conn)
        })?;
        Ok(result)
    }

    fn mark_device_sync_msg_as_processed(&self, message_id: &[u8]) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
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

    fn increment_device_sync_msg_attempt(&self, message_id: &[u8]) -> Result<i32, StorageError> {
        let attempts = self.raw_query_write(|conn| {
            // First increment the attempt count
            diesel::update(dsl::processed_device_sync_messages.find(message_id))
                .set(dsl::attempts.eq(dsl::attempts + 1))
                .execute(conn)?;

            // Get the updated record
            let record: StoredProcessedDeviceSyncMessages = dsl::processed_device_sync_messages
                .find(message_id)
                .first(conn)?;

            // If we've reached max attempts, set state to Failed
            if record.attempts >= StoredProcessedDeviceSyncMessages::MAX_ATTEMPTS {
                diesel::update(dsl::processed_device_sync_messages.find(message_id))
                    .set(dsl::state.eq(DeviceSyncProcessingState::Failed))
                    .execute(conn)?;
            }

            Ok(record.attempts)
        })?;
        Ok(attempts)
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
    fn it_marks_as_processed() {
        with_connection(|conn| {
            let mut group = generate_group(None);
            group.conversation_type = ConversationType::Sync;
            group.store(conn)?;

            let mut group2 = generate_group(None);
            group2.conversation_type = ConversationType::Sync;
            group2.store(conn)?;

            let message1 = generate_message(None, Some(&group.id), None, None, None, None);
            message1.store(conn)?;
            let message2 = generate_message(None, Some(&group2.id), None, None, None, None);
            message2.store(conn)?;

            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 2);

            // Storing with Pending state still counts as unprocessed
            StoredProcessedDeviceSyncMessages::new(message2.id.clone()).store(conn)?;
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 2);

            // Setting state to Processed marks it as processed
            conn.mark_device_sync_msg_as_processed(&message2.id)?;

            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 1);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn it_stores_with_attempts_and_state() {
        with_connection(|conn| {
            let mut group = generate_group(None);
            group.conversation_type = ConversationType::Sync;
            group.store(conn)?;

            let message = generate_message(None, Some(&group.id), None, None, None, None);
            message.store(conn)?;

            // Store with default values (Pending state)
            let stored = StoredProcessedDeviceSyncMessages::new(message.id.clone());
            assert_eq!(stored.attempts, 0);
            assert_eq!(stored.state, DeviceSyncProcessingState::Pending);
            stored.store(conn)?;

            // Pending state is still considered unprocessed
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 1);

            // Update to Processed state using mark_device_sync_msg_as_processed
            conn.mark_device_sync_msg_as_processed(&message.id)?;

            // Now it's no longer in unprocessed
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 0);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn it_preserves_attempts_when_marking_as_processed() {
        with_connection(|conn| {
            let mut group = generate_group(None);
            group.conversation_type = ConversationType::Sync;
            group.store(conn)?;

            let message = generate_message(None, Some(&group.id), None, None, None, None);
            message.store(conn)?;

            // Store with Pending state
            StoredProcessedDeviceSyncMessages::new(message.id.clone()).store(conn)?;

            // Increment attempts a couple times
            conn.increment_device_sync_msg_attempt(&message.id)?;
            conn.increment_device_sync_msg_attempt(&message.id)?;

            // Now mark as processed
            conn.mark_device_sync_msg_as_processed(&message.id)?;

            // Verify attempts are preserved (should be 2)
            let record: StoredProcessedDeviceSyncMessages = conn.raw_query_read(|c| {
                dsl::processed_device_sync_messages
                    .find(&message.id)
                    .first(c)
            })?;
            assert_eq!(record.attempts, 2);
            assert_eq!(record.state, DeviceSyncProcessingState::Processed);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn it_increments_attempts_and_sets_failed_at_max() {
        with_connection(|conn| {
            let mut group = generate_group(None);
            group.conversation_type = ConversationType::Sync;
            group.store(conn)?;

            let message = generate_message(None, Some(&group.id), None, None, None, None);
            message.store(conn)?;

            // Store with default values (attempts = 0)
            StoredProcessedDeviceSyncMessages::new(message.id.clone()).store(conn)?;

            // Increment attempt 1
            let attempts = conn.increment_device_sync_msg_attempt(&message.id)?;
            assert_eq!(attempts, 1);
            // Still pending (below max)
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 1);

            // Increment attempt 2
            let attempts = conn.increment_device_sync_msg_attempt(&message.id)?;
            assert_eq!(attempts, 2);
            // Still pending (below max)
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 1);

            // Increment attempt 3 (reaches MAX_ATTEMPTS)
            let attempts = conn.increment_device_sync_msg_attempt(&message.id)?;
            assert_eq!(attempts, 3);
            // Should now be Failed and no longer in unprocessed
            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 0);
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
                assert_eq!(msg.group_id, sync_group.id);
            }
        })
    }
}
