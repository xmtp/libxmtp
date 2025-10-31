use xmtp_proto::types::ClockOrdering;

/// common functions w.r.t vector clock types
pub trait VectorClock {
    /// Merges another clock into this one by taking the max ordering per node
    fn dominates(&self, other: &Self) -> bool;

    /// Returns true if this clock dominates (has seen all updates of) the other
    fn merge(&mut self, other: &Self);

    /// Compares this clock to another to determine their relative ordering
    fn compare(&self, other: &Self) -> ClockOrdering;
}
