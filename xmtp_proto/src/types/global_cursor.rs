//! A global cursor is type of cursor representing a view of our position across all originators
//! in the network.
use crate::{
    types::{OriginatorId, SequenceId},
    xmtp::xmtpv4::envelopes::Cursor,
};
use core::fmt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use xmtp_configuration::Originators;

/// a cursor which represents the position across many nodes in the network
/// a.k.a vector clock
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCursor {
    inner: HashMap<OriginatorId, SequenceId>,
}

impl GlobalCursor {
    /// Construct a new cursor from a HashMap
    pub fn new(map: HashMap<OriginatorId, SequenceId>) -> Self {
        Self { inner: map }
    }

    /// Get the maximum sequence id for [`crate::xmtpv4::Originator`]
    pub fn get(&self, originator: &OriginatorId) -> SequenceId {
        self.inner.get(originator).copied().unwrap_or_default()
    }

    /// Get the max sequence id across all originator ids
    pub fn max(&self) -> SequenceId {
        self.inner.values().copied().max().unwrap_or(0)
    }

    /// get latest sequence id for the v3 welcome message originator
    pub fn v3_welcome(&self) -> SequenceId {
        self.inner
            .get(&(Originators::WELCOME_MESSAGES))
            .copied()
            .unwrap_or_default()
    }

    /// get latest sequence id for v3 application message originator
    pub fn v3_message(&self) -> SequenceId {
        self.inner
            .get(&(Originators::APPLICATION_MESSAGES))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence id for the mls commit originator (v3/d14n)
    pub fn commit(&self) -> SequenceId {
        self.inner
            .get(&(Originators::MLS_COMMITS))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence_id for the installation/key package originator
    pub fn v3_installations(&self) -> SequenceId {
        self.inner
            .get(&(Originators::INSTALLATIONS))
            .copied()
            .unwrap_or_default()
    }

    /// get the latest sequence id for the inbox log originator (v3/d14n)
    pub fn inbox_log(&self) -> SequenceId {
        self.inner
            .get(&(Originators::INBOX_LOG))
            .copied()
            .unwrap_or_default()
    }
}

impl fmt::Display for GlobalCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (oid, sid) in self.inner.iter() {
            write!(
                f,
                "GloballySeen [{}]",
                crate::types::Cursor {
                    sequence_id: *sid,
                    originator_id: *oid
                }
            )?;
        }
        Ok(())
    }
}

impl FromIterator<(OriginatorId, SequenceId)> for GlobalCursor {
    fn from_iter<T: IntoIterator<Item = (OriginatorId, SequenceId)>>(iter: T) -> Self {
        GlobalCursor::new(HashMap::from_iter(iter))
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

impl IntoIterator for GlobalCursor {
    type Item = (OriginatorId, SequenceId);
    type IntoIter = <HashMap<OriginatorId, SequenceId> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a GlobalCursor {
    type Item = (&'a OriginatorId, &'a SequenceId);
    type IntoIter = std::collections::hash_map::Iter<'a, OriginatorId, SequenceId>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a> IntoIterator for &'a mut GlobalCursor {
    type Item = (&'a OriginatorId, &'a mut SequenceId);
    type IntoIter = std::collections::hash_map::IterMut<'a, OriginatorId, SequenceId>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

impl Deref for GlobalCursor {
    type Target = HashMap<OriginatorId, SequenceId>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for GlobalCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrdering {
    Equal,
    Ancestor,
    Descendant,
    Concurrent,
}
