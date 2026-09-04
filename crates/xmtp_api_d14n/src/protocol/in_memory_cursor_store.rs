use crate::protocol::{CursorStore, CursorStoreError};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use xmtp_proto::api::VectorClock;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, OrphanedEnvelope, Topic};

#[derive(Clone)]
pub struct InMemoryCursorStore {
    topics: HashMap<Topic, GlobalCursor>,
    icebox: Arc<Mutex<HashSet<OrphanedEnvelope>>>,
    cutover_ns: Arc<Mutex<i64>>,
    last_checked_ns: Arc<Mutex<i64>>,
    migrated: Arc<Mutex<bool>>,
}

impl Default for InMemoryCursorStore {
    fn default() -> Self {
        Self {
            topics: HashMap::new(),
            icebox: Arc::new(Mutex::new(HashSet::new())),
            cutover_ns: Arc::new(Mutex::new(i64::MAX)),
            last_checked_ns: Arc::new(Mutex::new(0)),
            migrated: Arc::new(Mutex::new(false)),
        }
    }
}

impl InMemoryCursorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a message for this topic with the given clock was received
    pub fn received(&mut self, topic: Topic, new_clock: &GlobalCursor) {
        let current = self.topics.entry(topic).or_default();
        current.merge(new_clock);
    }

    /// Get the current vector clock for this topic
    pub fn get_latest(&self, topic: &Topic) -> Option<&GlobalCursor> {
        self.topics.get(topic)
    }

    #[cfg(test)]
    pub fn icebox(&self) -> Vec<OrphanedEnvelope> {
        let icebox = self.icebox.lock();
        Vec::from_iter(icebox.clone())
    }
}

impl CursorStore for InMemoryCursorStore {
    fn latest(
        &self,
        topic: &xmtp_proto::types::Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, crate::protocol::CursorStoreError> {
        let cursor = self.get_latest(topic).cloned().unwrap_or_default();
        if let Some(oids) = originators {
            Ok(cursor
                .iter()
                .filter(|(k, _)| oids.contains(k))
                .map(|(&k, &v)| (k, v))
                .collect())
        } else {
            Ok(cursor)
        }
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, super::CursorStoreError> {
        Ok(topics
            .map(|topic| (topic.clone(), self.latest(topic, None).unwrap_or_default()))
            .collect())
    }

    fn find_message_dependencies(
        &self,
        hash: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, super::CursorStoreError> {
        // in mem does not keep track of deps/commits
        Err(CursorStoreError::NoDependenciesFound(
            hash.iter().map(hex::encode).collect(),
        ))
    }

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        let mut icebox = self.icebox.lock();
        (*icebox).extend(orphans);
        Ok(())
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        let icebox = self.icebox.lock();
        Ok(Vec::from_iter(resolve_children_inner(cursors, &icebox)))
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        *self.cutover_ns.lock() = cutover_ns;
        Ok(())
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(*self.cutover_ns.lock())
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        Ok(*self.migrated.lock())
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        *self.migrated.lock() = has_migrated;
        Ok(())
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(*self.last_checked_ns.lock())
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        *self.last_checked_ns.lock() = last_checked_ns;
        Ok(())
    }
}

fn resolve_children_inner(
    cursors: &[Cursor],
    icebox: &HashSet<OrphanedEnvelope>,
) -> HashSet<OrphanedEnvelope> {
    let mut children: HashSet<OrphanedEnvelope> =
        cursors.iter().fold(HashSet::new(), |mut acc, cursor| {
            // extract if item in an icebox is child of the cursor
            let children = icebox
                .iter()
                .filter(|o| o.is_child_of(cursor))
                .cloned()
                .collect::<HashSet<_>>();
            acc.extend(children);
            acc
        });
    // recursively work through deps
    let cursors = children.iter().fold(Vec::new(), |mut acc, c| {
        if !c.depends_on.is_empty() {
            acc.push(c.cursor);
        }
        acc
    });
    if !cursors.is_empty() {
        let v = resolve_children_inner(&cursors, icebox);
        children.extend(v);
    }
    children
}

impl fmt::Debug for InMemoryCursorStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entries = f.debug_map();

        for (topic, cursor) in &self.topics {
            // display topic as hex for readability
            let topic_hex = hex::encode(topic);
            entries.entry(&topic_hex, cursor);
        }

        entries.finish()
    }
}
