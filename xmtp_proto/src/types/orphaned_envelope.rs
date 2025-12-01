use derive_builder::Builder;

use crate::ConversionError;
use crate::types::{Cursor, GlobalCursor, GroupId};
use bytes::Bytes;

/// An envelope whose parent dependencies have not yet been seen
#[derive(Builder, Clone, Debug, Hash, PartialEq, Eq)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct OrphanedEnvelope {
    // the cursor of this envelope
    pub cursor: Cursor,
    /// the envelopes this orphan depends on
    #[builder(setter(each(name = "depending_on")))]
    pub depends_on: GlobalCursor,
    /// the original payload
    pub payload: Bytes,
    /// the group this orphan belongs to
    pub group_id: GroupId,
}

impl OrphanedEnvelope {
    pub fn builder() -> OrphanedEnvelopeBuilder {
        OrphanedEnvelopeBuilder::default()
    }

    ///  turn this envelope back into its parts
    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    /// check if we are dependant on [`Cursor`]
    pub fn is_child_of(&self, cursor: &Cursor) -> bool {
        let sid = self.depends_on.get(&cursor.originator_id);

        sid == cursor.sequence_id
    }
}
