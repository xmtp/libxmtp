use super::schema::group_messages;
use crate::impl_fetch_and_store;
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = group_messages)]
#[diesel(primary_key(id))]
/// Successfully processed messages to be returned to the User.
pub struct StoredGroupMessage {
    /// Id of the message.
    pub id: Vec<u8>,
    /// Id of the group this message is tied to.
    pub group_id: Vec<u8>,
    /// Contents of message after decryption.
    pub decrypted_message_bytes: Vec<u8>,
    /// Time in nanoseconds the message was sent.
    pub sent_at_ns: i64,
    /// The ID of the App Installation this message was sent from.
    pub sender_installation_id: Vec<u8>,
    /// Network wallet address of the Sender
    pub sender_wallet_address: String,
}

impl_fetch_and_store!(StoredGroupMessage, group_messages, Vec<u8>);

