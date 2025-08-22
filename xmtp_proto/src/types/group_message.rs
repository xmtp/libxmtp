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
}

impl GroupMessage {
    pub fn builder() -> GroupMessageBuilder {
        GroupMessageBuilder::default()
    }

    pub fn is_commit(&self) -> bool {
        self.message.content_type() == ContentType::Commit
    }
}
