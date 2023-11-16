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

    pub fn update_last_synced_timestamp_for_topic(
        conn: &mut DbConnection,
        topic: &str,
        timestamp_ns: i64,
    ) -> Result<bool, StorageError> {
        let state: Option<TopicRefreshState> = conn.fetch(&topic.to_string())?;
        match state {
            Some(state) => {
                use super::schema::topic_refresh_state::dsl;
                let num_updated = diesel::update(&state)
                    .filter(dsl::last_message_timestamp_ns.lt(timestamp_ns))
                    .set(dsl::last_message_timestamp_ns.eq(timestamp_ns))
                    .execute(conn)?;
                Ok(num_updated == 1)
            }
            None => Err(StorageError::NotFound),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{storage::encrypted_store::tests::with_store, Fetch, Store};

    #[test]
    fn get_timestamp_with_no_existing_state() {
        with_store(|mut conn| {
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert!(entry.is_none());
            assert_eq!(
                EncryptedMessageStore::get_last_synced_timestamp_for_topic(&mut conn, "topic")
                    .unwrap(),
                0
            );
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert!(entry.is_some());
        })
    }

    #[test]
    fn get_timestamp_with_existing_state() {
        with_store(|mut conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(&mut conn).unwrap();
            assert_eq!(
                EncryptedMessageStore::get_last_synced_timestamp_for_topic(&mut conn, "topic")
                    .unwrap(),
                123
            );
        })
    }

    #[test]
    fn update_timestamp_when_bigger() {
        with_store(|mut conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(&mut conn).unwrap();
            assert_eq!(
                EncryptedMessageStore::update_last_synced_timestamp_for_topic(
                    &mut conn, "topic", 124
                )
                .unwrap(),
                true
            );
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert_eq!(entry.unwrap().last_message_timestamp_ns, 124);
        })
    }

    #[test]
    fn dont_update_timestamp_when_smaller() {
        with_store(|mut conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(&mut conn).unwrap();
            assert_eq!(
                EncryptedMessageStore::update_last_synced_timestamp_for_topic(
                    &mut conn, "topic", 122
                )
                .unwrap(),
                false
            );
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert_eq!(entry.unwrap().last_message_timestamp_ns, 123);
        })
    }
}
