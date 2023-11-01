use super::schema::outbound_welcome_messages;
use crate::impl_fetch_and_store;
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = outbound_welcome_messages)]
#[diesel(primary_key(id))]
pub struct StoredOutboundWelcomeMessage {
    pub id: Vec<u8>,
    pub state: i32,
    pub installation_id: Vec<u8>,
    pub commit_hash: Vec<u8>,
    pub group_id: Vec<u8>,
    pub welcome_message: Vec<u8>,
}


impl_fetch_and_store!(StoredOutboundWelcomeMessage, outbound_welcome_messages, Vec<u8>);
