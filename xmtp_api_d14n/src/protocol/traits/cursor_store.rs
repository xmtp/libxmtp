use std::sync::Arc;

use xmtp_common::RetryableError;
use xmtp_proto::{
    api::ApiClientError,
    types::{Cursor, GlobalCursor, OriginatorId, Topic, TopicKind},
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

    /// A temporary function to get the latest cursor for
    /// a topic & originator. it may be missing updates.
    fn latest_maybe_missing_per(
        &self,
        topic: &Topic,
        originator: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError>;

    /// Temporary until reliable streams
    fn latest_maybe_missing(
        &self,
        topic: &Topic,
        originator: &OriginatorId,
    ) -> Result<Cursor, CursorStoreError> {
        let sid = self
            .latest_maybe_missing_per(topic, &[originator])?
            .get(originator);
        Ok(Cursor {
            originator_id: *originator,
            sequence_id: sid,
        })
    }

    // temp until reliable streams
    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;
}

impl<T: CursorStore> CursorStore for Option<T> {
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.lowest_common_cursor(topics)
        } else {
            Ok(GlobalCursor::default())
        }
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest(topic)
        } else {
            Ok(GlobalCursor::default())
        }
    }

    fn latest_per_originator(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest_per_originator(topic, originators)
        } else {
            Ok(GlobalCursor::default())
        }
    }

    fn latest_maybe_missing_per(
        &self,
        topic: &Topic,
        originator: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest_maybe_missing_per(topic, originator)
        } else {
            Ok(GlobalCursor::default())
        }
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.lcc_maybe_missing(topic)
        } else {
            Ok(GlobalCursor::default())
        }
    }
}

impl<T: CursorStore + ?Sized> CursorStore for Arc<T> {
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lowest_common_cursor(topics)
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic)
    }

    fn latest_per_originator(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest_per_originator(topic, originators)
    }

    fn latest_maybe_missing_per(
        &self,
        topic: &Topic,
        originator: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest_maybe_missing_per(topic, originator)
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lcc_maybe_missing(topic)
    }
}

impl<T: CursorStore + ?Sized> CursorStore for Box<T> {
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lowest_common_cursor(topics)
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic)
    }

    fn latest_per_originator(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest_per_originator(topic, originators)
    }

    fn latest_maybe_missing_per(
        &self,
        topic: &Topic,
        originator: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest_maybe_missing_per(topic, originator)
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lcc_maybe_missing(topic)
    }
}
/// This cursor store always returns 0
#[derive(Default, Copy, Clone)]
pub struct NoCursorStore;

impl CursorStore for NoCursorStore {
    fn lowest_common_cursor(&self, _: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    fn latest(&self, _: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    fn latest_per_originator(
        &self,
        _: &Topic,
        _: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    fn latest_maybe_missing_per(
        &self,
        _: &Topic,
        _: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    fn lcc_maybe_missing(&self, _: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }
}
