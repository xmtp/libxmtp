use crate::types::ClockOrdering;
use crate::xmtp::xmtpv4::envelopes::Cursor;
use std::collections::{HashMap, HashSet};

impl Cursor {
    /// Creates a new empty cursor
    pub fn new() -> Self {
        Cursor {
            node_id_to_sequence_id: HashMap::new(),
        }
    }

    /// Increments the sequence ID for the given node
    pub fn increment(&mut self, node_id: u32) {
        *self.node_id_to_sequence_id.entry(node_id).or_insert(0) += 1;
    }

    /// Merges another cursor into this one by taking max(seq_id) per node
    pub fn merge_cursor(&mut self, other: &Cursor) {
        for (&node, &seq) in &other.node_id_to_sequence_id {
            let entry = self.node_id_to_sequence_id.entry(node).or_insert(0);
            *entry = (*entry).max(seq);
        }
    }

    /// Compares this cursor to another to determine their relative ordering
    pub fn compare(&self, other: &Cursor) -> ClockOrdering {
        let all_nodes: HashSet<_> = self
            .node_id_to_sequence_id
            .keys()
            .chain(other.node_id_to_sequence_id.keys())
            .collect();

        let mut self_greater = false;
        let mut other_greater = false;

        for node in all_nodes {
            let a = self.node_id_to_sequence_id.get(node).cloned().unwrap_or(0);
            let b = other.node_id_to_sequence_id.get(node).cloned().unwrap_or(0);

            if a > b {
                self_greater = true;
            } else if a < b {
                other_greater = true;
            }
        }

        match (self_greater, other_greater) {
            (false, false) => ClockOrdering::Equal,
            (true, false) => ClockOrdering::Descendant,
            (false, true) => ClockOrdering::Ancestor,
            (true, true) => ClockOrdering::Concurrent,
        }
    }

    /// Returns true if this cursor dominates (has seen all updates of) the other
    pub fn dominates(&self, other: &Cursor) -> bool {
        other.node_id_to_sequence_id.iter().all(|(&node, &seq)| {
            self.node_id_to_sequence_id.get(&node).cloned().unwrap_or(0) >= seq
        })
    }
}
