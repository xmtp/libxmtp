use super::schema::group_messages;
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = group_messages)]
#[diesel(primary_key(id))]
pub struct StoredGroupMessage {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub decrypted_message_bytes: Vec<u8>,
    pub sent_at_ns: i64,
    pub sender_installation_id: Vec<u8>,
    pub sender_wallet_address: String,
}
