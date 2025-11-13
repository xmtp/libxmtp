use xmtp_proto::types::{ClockOrdering, Cursor};

/// common functions w.r.t vector clock types
pub trait VectorClock {
    /// Returns true if this clock dominates (has seen all updates of) the other
    fn dominates(&self, other: &Self) -> bool;

    /// Merges another clock into this one by taking the max ordering per node
    fn merge(&mut self, other: &Self);

    /// Compares this clock to another to determine their relative ordering
    fn compare(&self, other: &Self) -> ClockOrdering;

    /// apply a single update to this clock
    fn apply(&mut self, cursor: &Cursor);
}
