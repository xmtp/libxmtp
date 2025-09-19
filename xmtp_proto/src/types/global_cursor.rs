//! A global cursor is type of cursor representing a view of our position across all originators
//! in the network.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::xmtp::xmtpv4::envelopes::Cursor;

/// a cursor which represents the position across many nodes in the network
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCursor {
    inner: HashMap<super::OriginatorId, super::SequenceId>,
}

impl From<Cursor> for GlobalCursor {
    fn from(value: Cursor) -> Self {
        GlobalCursor {
            inner: value.node_id_to_sequence_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrdering {
    Equal,
    Ancestor,
    Descendant,
    Concurrent,
}
