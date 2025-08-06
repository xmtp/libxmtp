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
}

impl<T> QueryDeviceSyncMessages for &T
where
    T: QueryDeviceSyncMessages,
{
    fn unprocessed_sync_group_messages(&self) -> Result<Vec<StoredGroupMessage>, StorageError> {
        (**self).unprocessed_sync_group_messages()
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
    async fn it_marks_as_processed() {
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
        .await;
    }
}
