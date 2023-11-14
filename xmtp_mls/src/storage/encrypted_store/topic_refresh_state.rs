use diesel::prelude::*;

use super::{schema::topic_refresh_state, DbConnection, EncryptedMessageStore};
use crate::{impl_fetch, impl_store, storage::StorageError, Fetch, Store};

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = topic_refresh_state)]
#[diesel(primary_key(topic))]
pub struct TopicRefreshState {
    pub topic: String,
    pub last_message_timestamp_ns: i64,
}

impl_fetch!(TopicRefreshState, topic_refresh_state, String);
impl_store!(TopicRefreshState, topic_refresh_state);

impl EncryptedMessageStore {
    pub fn get_last_synced_timestamp_for_topic(
        &self,
        conn: &mut DbConnection,
        topic: &str,
    ) -> Result<i64, StorageError> {
        let state: Option<TopicRefreshState> = conn.fetch(&topic.to_string())?;
        match state {
            Some(state) => Ok(state.last_message_timestamp_ns),
            None => {
                let new_state = TopicRefreshState {
                    topic: topic.to_string(),
                    last_message_timestamp_ns: 0,
                };
                new_state.store(conn)?;
                Ok(0)
            }
        }
    }
}
