//! A global cursor is type of cursor representing a view of our position across all originators
//! in the network.
use crate::xmtp::xmtpv4::envelopes::Cursor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmtp_configuration::Originators;

/// a cursor which represents the position across many nodes in the network
/// a.k.a vector clock
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCursor {
    inner: HashMap<super::OriginatorId, super::SequenceId>,
}

impl GlobalCursor {
    pub fn new(map: HashMap<super::OriginatorId, super::SequenceId>) -> Self {
        Self { inner: map }
    }

    /// get latest sequence id for the v3 welcome message originator
    pub fn v3_welcome(&self) -> super::SequenceId {
        self.inner
            .get(&(Originators::WELCOME_MESSAGES as u32))
            .copied()
            .unwrap_or_default()
    }

    /// get latest sequence id for v3 application message originator
    pub fn v3_message(&self) -> super::SequenceId {
        self.inner
            .get(&(Originators::APPLICATION_MESSAGES as u32))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence id for the mls commit originator (v3/d14n)
    pub fn commit(&self) -> super::SequenceId {
        self.inner
            .get(&(Originators::MLS_COMMITS as u32))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence_id for the installation/key package originator
    pub fn v3_installations(&self) -> super::SequenceId {
        self.inner
            .get(&(Originators::INSTALLATIONS as u32))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence id for the inbox log originator (v3/d14n)
    pub fn inbox_log(&self) -> super::SequenceId {
        self.inner
            .get(&(Originators::INBOX_LOG as u32))
            .copied()
            .unwrap_or_default()
    }
}

impl From<Cursor> for GlobalCursor {
    fn from(value: Cursor) -> Self {
        GlobalCursor {
            inner: value.node_id_to_sequence_id,
        }
    }
}

impl From<GlobalCursor> for Cursor {
    fn from(value: GlobalCursor) -> Self {
        Cursor {
            node_id_to_sequence_id: value.inner,
        }
    }
}

impl From<crate::types::Cursor> for GlobalCursor {
    fn from(value: crate::types::Cursor) -> Self {
        let mut map = HashMap::new();
        map.insert(value.originator_id, value.sequence_id);
        GlobalCursor { inner: map }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrdering {
    Equal,
    Ancestor,
    Descendant,
    Concurrent,
}
