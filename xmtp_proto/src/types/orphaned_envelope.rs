use derive_builder::Builder;

use crate::ConversionError;
use crate::types::{Cursor, GlobalCursor, GroupId};
use bytes::Bytes;

/// An envelope whose parent dependencies have not yet been seen
#[derive(Builder, Clone, Debug)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct OrphanedEnvelope {
    // the cursor of this envelope
    cursor: Cursor,
    /// the envelopes this orphan depends on
    depends_on: GlobalCursor,
    /// the original payload
    payload: Bytes,
    /// the group this orphan belongs to
    group_id: GroupId,
}

impl OrphanedEnvelope {
    pub fn builder() -> OrphanedEnvelopeBuilder {
        OrphanedEnvelopeBuilder::default()
    }

    /// get the cursor of this envelope
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// get the depending envelope
    pub fn depends_on(&self) -> &GlobalCursor {
        &self.depends_on
    }

    ///  turn this envelope back into its parts
    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    pub fn group_id(&self) -> &GroupId {
        &self.group_id
    }
}
