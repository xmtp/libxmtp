use crate::ConversionError;
use chrono::Utc;
use derive_builder::Builder;

use crate::types::{Cursor, GroupId};

#[derive(Clone, Builder, Debug)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct GroupMessageMetadata {
    /// Cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Utc>,
    /// GroupId of the message
    pub group_id: GroupId,
    #[builder(default)]
    pub envelope_hash: Option<Vec<u8>>,
    #[builder(default)]
    pub expiry_ns: Option<u64>,
}

impl GroupMessageMetadata {
    pub fn builder() -> GroupMessageMetadataBuilder {
        GroupMessageMetadataBuilder::default()
    }

    pub fn sequence_id(&self) -> u64 {
        self.cursor.0
    }
}
