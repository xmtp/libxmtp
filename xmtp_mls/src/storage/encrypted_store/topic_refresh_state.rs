use super::schema::topic_refresh_state;
use crate::impl_fetch_and_store;
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = topic_refresh_state)]
#[diesel(primary_key(topic))]
pub struct TopicRefreshState {
    pub topic: String,
    pub last_message_timestamp_ns: i64,
}

impl_fetch_and_store!(TopicRefreshState, topic_refresh_state);
