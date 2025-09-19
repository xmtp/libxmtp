use xmtp_proto::types::{ClockOrdering, Cursor, GlobalCursor, Topic};

/// Trait defining how cursors should be stored, updated, and fetched
pub trait CursorStore {
    type Error;
    /// Get the last seen cursor per originator
    fn last_seen(&self, topic: &Topic) -> Result<GlobalCursor, Self::Error>;

    /// Compute the lowest common cursor across a set of topics.
    /// For each node_id, uses the **minimum** sequence ID seen across all topics.
    fn lowest_common_cursor(&self, topics: &[Topic]) -> Option<GlobalCursor>;

    /// mark the topic at cursor as tracked.
    // _*NOTE*_ If a topic at a cursor is tracked, has_seen
    // MUST return true.
    fn mark_seen(&self, topic: &Topic, cursor: Cursor);

    /// Has this item been processed yet
    fn has_seen(&self, topic: &Topic, cursor: Cursor);
}

/// common functions w.r.t vector clock types
pub trait VectorClock {
    /// Merges another clock into this one by taking the max ordering per node
    fn dominates(&self, other: &Self) -> bool;

    /// Returns true if this clock dominates (has seen all updates of) the other
    fn merge(&mut self, other: &Self);

    /// Compares this clock to another to determine their relative ordering
    fn compare(&self, other: &Self) -> ClockOrdering;
}
