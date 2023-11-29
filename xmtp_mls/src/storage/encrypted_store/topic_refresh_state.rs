use diesel::prelude::*;

use super::{db_connection::DbConnection, schema::topic_refresh_state};
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

impl DbConnection<'_> {
    pub fn get_last_synced_timestamp_for_topic(&self, topic: &str) -> Result<i64, StorageError> {
        let state: Option<TopicRefreshState> = self.fetch(&topic.to_string())?;
        match state {
            Some(state) => Ok(state.last_message_timestamp_ns),
            None => {
                let new_state = TopicRefreshState {
                    topic: topic.to_string(),
                    last_message_timestamp_ns: 0,
                };
                new_state.store(self)?;
                Ok(0)
            }
        }
    }

    pub fn update_last_synced_timestamp_for_topic(
        &self,
        topic: &str,
        timestamp_ns: i64,
    ) -> Result<bool, StorageError> {
        let state: Option<TopicRefreshState> = self.fetch(&topic.to_string())?;
        match state {
            Some(state) => {
                use super::schema::topic_refresh_state::dsl;
                let num_updated = self.raw_query(|conn| {
                    diesel::update(&state)
                        .filter(dsl::last_message_timestamp_ns.lt(timestamp_ns))
                        .set(dsl::last_message_timestamp_ns.eq(timestamp_ns))
                        .execute(conn)
                })?;
                Ok(num_updated == 1)
            }
            None => Err(StorageError::NotFound),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{storage::encrypted_store::tests::with_connection, Fetch, Store};

    #[test]
    fn get_timestamp_with_no_existing_state() {
        with_connection(|conn| {
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert!(entry.is_none());
            assert_eq!(
                conn.get_last_synced_timestamp_for_topic("topic").unwrap(),
                0
            );
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert!(entry.is_some());
        })
    }

    #[test]
    fn get_timestamp_with_existing_state() {
        with_connection(|conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(conn).unwrap();
            assert_eq!(
                conn.get_last_synced_timestamp_for_topic("topic").unwrap(),
                123
            );
        })
    }

    #[test]
    fn update_timestamp_when_bigger() {
        with_connection(|conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(conn).unwrap();
            assert!(conn
                .update_last_synced_timestamp_for_topic("topic", 124)
                .unwrap());
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert_eq!(entry.unwrap().last_message_timestamp_ns, 124);
        })
    }

    #[test]
    fn dont_update_timestamp_when_smaller() {
        with_connection(|conn| {
            let entry = TopicRefreshState {
                topic: "topic".to_string(),
                last_message_timestamp_ns: 123,
            };
            entry.store(conn).unwrap();
            assert!(!conn
                .update_last_synced_timestamp_for_topic("topic", 122)
                .unwrap());
            let entry: Option<TopicRefreshState> = conn.fetch(&"topic".to_string()).unwrap();
            assert_eq!(entry.unwrap().last_message_timestamp_ns, 123);
        })
    }
}
