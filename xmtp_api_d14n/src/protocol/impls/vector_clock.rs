use std::collections::HashSet;

use xmtp_proto::types::{ClockOrdering, GlobalCursor};

use crate::protocol::VectorClock;

impl VectorClock for GlobalCursor {
    fn dominates(&self, other: &Self) -> bool {
        other
            .inner
            .iter()
            .all(|(&node, &seq)| self.inner.get(&node).cloned().unwrap_or(0) >= seq)
    }

    fn merge(&mut self, other: &Self) {
        for (&node, &seq) in &other.inner {
            let entry = self.inner.entry(node).or_insert(0);
            *entry = (*entry).max(seq);
        }
    }

    fn compare(&self, other: &Self) -> ClockOrdering {
        let all_nodes: HashSet<_> = self.inner.keys().chain(other.inner.keys()).collect();

        let mut self_greater = false;
        let mut other_greater = false;

        for node in all_nodes {
            let a = self.inner.get(node).cloned().unwrap_or(0);
            let b = other.inner.get(node).cloned().unwrap_or(0);

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
}
