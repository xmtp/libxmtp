use xmtp_common::RetryableError;
use xmtp_proto::{
    api::ApiClientError,
    types::{ClockOrdering, Cursor, GlobalCursor, OriginatorId, Topic},
};

#[derive(thiserror::Error, Debug)]
pub enum CursorStoreError {
    #[error("error writing cursors to persistent store")]
    Write,
    #[error("error reading cursors from persistent store")]
    Read,
    #[error("{0}")]
    Other(Box<dyn RetryableError + Send + Sync>),
}

impl RetryableError for CursorStoreError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Other(s) => s.is_retryable(),
            // retries should be an implementation detail
            _ => false,
        }
    }
}

impl<E: std::error::Error> From<CursorStoreError> for ApiClientError<E> {
    fn from(value: CursorStoreError) -> Self {
        ApiClientError::Other(Box::new(value) as Box<_>)
    }
}

/// Trait defining how cursors should be stored, updated, and fetched
/// _NOTE:_, implementations decide retry strategy. the exact implementation of persistence (or lack)
/// is up to implementors. functions are assumed to be idempotent & atomic.
pub trait CursorStore: Send + Sync {
    // /// Get the last seen cursor per originator
    // fn last_seen(&self, topic: &Topic) -> Result<GlobalCursor, Self::Error>;

    /// Compute the lowest common cursor across a set of topics.
    /// For each node_id, uses the **minimum** sequence ID seen across all topics.
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;

    /// get the highest sequence id of each originator at a topic
    fn latest_for_each(
        &self,
        originators: &[OriginatorId],
        topic: &Topic,
    ) -> Result<Vec<Cursor>, CursorStoreError>;

    /// get the latest sequence id for a single originator at a topic
    fn latest(&self, originator: OriginatorId, topic: &Topic) -> Result<Cursor, CursorStoreError> {
        Ok(self
            .latest_for_each(&[originator], topic)?
            .first()
            .copied()
            .unwrap_or_default())
    }

    // /// mark the topic at cursor as tracked.
    // _*NOTE*_ If a topic at a cursor is tracked, has_seen
    // MUST return true.
    //fn mark_seen(&self, topic: &Topic, cursor: Cursor);

    // /// Has this item been processed yet
    //fn has_seen(&self, topic: &Topic, cursor: Cursor);
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
