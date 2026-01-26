use super::{
    ConnectionExt,
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
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Insertable, Identifiable, Queryable)]
#[diesel(table_name = processed_device_sync_messages)]
#[diesel(primary_key(message_id))]
pub struct StoredProcessedDeviceSyncMessages {
    pub message_id: Vec<u8>,
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
}

impl<C: ConnectionExt> QueryDeviceSyncMessages for DbConnection<C> {
    fn unprocessed_sync_group_messages(&self) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let result = self.raw_query_read(|conn| {
            group_messages_dsl::group_messages
                .inner_join(groups_dsl::groups.on(group_messages_dsl::group_id.eq(groups_dsl::id)))
                .filter(groups_dsl::conversation_type.eq(ConversationType::Sync))
                .filter(diesel::dsl::not(diesel::dsl::exists(
                    dsl::processed_device_sync_messages
                        .filter(dsl::message_id.eq(group_messages_dsl::id)),
                )))
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
}

#[cfg(test)]
mod tests {
    use crate::{
        Store,
        group::{ConversationType, tests::generate_group},
        group_message::tests::generate_message,
        prelude::*,
        processed_device_sync_messages::StoredProcessedDeviceSyncMessages,
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

            let message = generate_message(None, Some(&group.id), None, None, None, None);
            message.store(conn)?;
            let message = generate_message(None, Some(&group2.id), None, None, None, None);
            message.store(conn)?;

            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 2);

            StoredProcessedDeviceSyncMessages {
                message_id: message.id.clone(),
            }
            .store(conn)?;

            let unprocessed = conn.unprocessed_sync_group_messages()?;
            assert_eq!(unprocessed.len(), 1);
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
