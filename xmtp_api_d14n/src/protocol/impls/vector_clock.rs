use std::collections::HashSet;

use xmtp_proto::types::{ClockOrdering, Cursor, GlobalCursor};

use crate::protocol::VectorClock;

impl VectorClock for GlobalCursor {
    fn dominates(&self, other: &Self) -> bool {
        other.iter().all(|(&node, &seq)| self.get(&node) >= seq)
    }

    /// gets all updates in `other` that are not seen by `self`.
    fn missing(&self, other: &Self) -> Vec<Cursor> {
        other
            .iter()
            .filter_map(|(&node, &seq)| {
                (self.get(&node) < seq).then_some(Cursor {
                    originator_id: node,
                    sequence_id: seq,
                })
            })
            .collect()
    }

    fn merge(&mut self, other: &Self) {
        for (&node, &seq) in other {
            let entry = self.entry(node).or_insert(0);
            *entry = (*entry).max(seq);
        }
    }

    fn merge_least(&mut self, other: &Self) {
        for (&node, &seq) in other {
            let entry = self.entry(node).or_insert(seq);
            *entry = (*entry).min(seq);
        }
    }

    fn compare(&self, other: &Self) -> ClockOrdering {
        let all_nodes: HashSet<_> = self.keys().chain(other.keys()).collect();

        let mut self_greater = false;
        let mut other_greater = false;

        for node in all_nodes {
            let a = self.get(node);
            let b = other.get(node);

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

    fn apply(&mut self, cursor: &Cursor) {
        let Cursor {
            originator_id: node,
            sequence_id: seq,
        } = &cursor;
        let entry = self.entry(*node).or_insert(0);
        *entry = (*entry).max(*seq)
    }
}
