use derive_builder::Builder;

use crate::ConversionError;
use crate::types::{Cursor, GlobalCursor, GroupId};
use bytes::Bytes;
use std::hash::Hash;

/// An envelope whose parent dependencies have not yet been seen
#[derive(Builder, Clone, Debug)]
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

// prost grpc encoding is _not_ deterministic.
// https://github.com/tokio-rs/prost/issues/965
// so we ned to write a custom Hash implementation to
// ignore the payload field
impl Hash for OrphanedEnvelope {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cursor.hash(state);
        self.group_id.hash(state);
        self.depends_on.hash(state);
    }
}

// needed for parity with Hash impl
impl PartialEq<OrphanedEnvelope> for OrphanedEnvelope {
    fn eq(&self, other: &OrphanedEnvelope) -> bool {
        self.cursor == other.cursor
            && self.depends_on == other.depends_on
            && self.group_id == other.group_id
    }
}

impl Eq for OrphanedEnvelope {}

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
        self.depends_on.contains_key(&cursor.originator_id)
            && self.depends_on.get(&cursor.originator_id) == cursor.sequence_id
    }
}
