use super::{Cursor, GroupId};
use crate::ConversionError;
use chrono::Local;
use derive_builder::Builder;
use openmls::prelude::ContentType;

/// A GroupMessage from the network
#[derive(Clone, Builder)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct GroupMessage {
    /// Cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Local>,
    /// GroupId of the message
    pub group_id: GroupId,
    // MLS Group Message
    pub message: openmls::framing::ProtocolMessage,
    /// Sender HMAC key
    pub sender_hmac: Vec<u8>,
    /// Whether this message should result in a push notification
    pub should_push: bool,
    /// Payload hash of the message
    /// TODO: make payload hash constant array
    pub payload_hash: Vec<u8>,
}

impl GroupMessage {
    pub fn builder() -> GroupMessageBuilder {
        GroupMessageBuilder::default()
    }

    pub fn is_commit(&self) -> bool {
        self.message.content_type() == ContentType::Commit
    }

    pub fn timestamp(&self) -> i64 {
        self.created_ns
            .timestamp_nanos_opt()
            .expect("timestamp out of range for i64, are we in 2262 A.D?")
    }

    pub fn originator_id(&self) -> u32 {
        self.cursor.originator_id
    }

    pub fn sequence_id(&self) -> u64 {
        self.cursor.sequence_id
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl xmtp_common::Generate for GroupMessage {
    fn generate() -> Self {
        GroupMessage {
            cursor: Default::default(),
            created_ns: chrono::DateTime::from_timestamp_nanos(xmtp_common::rand_i64()).into(),
            group_id: GroupId::generate(),
            message: openmls::prelude::PublicMessage::generate().into(),
            sender_hmac: xmtp_common::rand_vec::<2>(),
            should_push: true,
            payload_hash: xmtp_common::rand_vec::<32>(),
        }
    }
}

#[cfg(test)]
mod test {
    use openmls::prelude::ContentType;
    use xmtp_common::Generate;

    use super::*;

    #[xmtp_common::test]
    fn test_is_commit() {
        let group_message = GroupMessage::generate();
        assert_eq!(
            group_message.is_commit(),
            group_message.message.content_type() == ContentType::Commit
        );
    }

    #[xmtp_common::test]
    fn test_timestamp() {
        let test_time = chrono::Local::now();
        let mut group_message = GroupMessage::generate();
        group_message.created_ns = test_time;
        assert_eq!(
            group_message.timestamp(),
            test_time.timestamp_nanos_opt().unwrap()
        );
    }
}
