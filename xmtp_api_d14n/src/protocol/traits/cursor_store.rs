use std::collections::HashMap;
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::{
    api::ApiClientError,
    types::{ClockOrdering, Cursor, GlobalCursor, OriginatorId, Topic, TopicKind},
};

#[derive(thiserror::Error, Debug)]
pub enum CursorStoreError {
    #[error("error writing cursors to persistent store")]
    Write,
    #[error("error reading cursors from persistent store")]
    Read,
    #[error("the store cannot handle topic of kind {0}")]
    UnhandledTopicKind(TopicKind),
    #[error("{0}")]
    Other(Box<dyn RetryableError>),
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
pub trait CursorStore: MaybeSend + MaybeSync {
    // /// Get the last seen cursor per originator
    // fn last_seen(&self, topic: &Topic) -> Result<GlobalCursor, Self::Error>;

    /// Compute the lowest common cursor across a set of topics.
    /// For each node_id, uses the **minimum** sequence ID seen across all topics.
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;

    /// get the highest sequence id for a topic, regardless of originator
    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError>;

    /// Get the latest cursor for each originator
    fn latest_per_originator(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError>;

    fn latest_for_originator(
        &self,
        topic: &Topic,
        originator: &OriginatorId,
    ) -> Result<Cursor, CursorStoreError> {
        let sid = self
            .latest_per_originator(topic, &[originator])?
            .get(originator);
        Ok(Cursor {
            originator_id: *originator,
            sequence_id: sid,
        })
    }

    /// Get the latest cursor for multiple topics at once.
    /// Returns a HashMap mapping each topic to its GlobalCursor.
    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError>;

    // temp until reliable streams
    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;
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
