use chrono::Local;
use derive_builder::Builder;
use crate::ConversionError;
use super::{Cursor, GroupId};

/// A GroupMessage from the network
#[derive(Default, Clone, Builder)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct GroupMessage {
    /// Cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Local>,
    /// GroupId of the message
    pub group_id: GroupId,
    // group message payload bytes
    pub data: Vec<u8>,
    /// Sender HMAC key
    pub sender_hmac: Vec<u8>,
    /// Whether this message should result in a push notification
    pub should_push: bool,
    /// Whether this message represents an MLS Commit
    pub is_commit: bool
}

impl GroupMessage {
    pub fn builder() -> GroupMessageBuilder {
        GroupMessageBuilder::default()
    }
}
