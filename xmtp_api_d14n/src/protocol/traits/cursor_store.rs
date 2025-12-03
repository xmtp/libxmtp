use std::collections::HashMap;
use std::sync::Arc;
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
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
    #[error("no dependencies found for {_0:?}")]
    NoDependenciesFound(Vec<String>),
    #[error("{0}")]
    Other(Box<dyn RetryableError>),
}

impl CursorStoreError {
    pub fn other<E: RetryableError + 'static>(e: E) -> Self {
        CursorStoreError::Other(Box::new(e))
    }
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
    /// find dependencies of each locally-stored intent payload hash
    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError>;
}

impl<T: CursorStore> CursorStore for Option<T> {
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.lowest_common_cursor(topics)
        } else {
            NoCursorStore.lowest_common_cursor(topics)
        }
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest(topic)
        } else {
            NoCursorStore.latest(topic)
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
            NoCursorStore.latest_per_originator(topic, originators)
        }
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        if let Some(c) = self {
            c.latest_for_topics(topics)
        } else {
            NoCursorStore.latest_for_topics(topics)
        }
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.lcc_maybe_missing(topic)
        } else {
            NoCursorStore.lcc_maybe_missing(topic)
        }
    }
    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        if let Some(c) = self {
            c.find_message_dependencies(hashes)
        } else {
            NoCursorStore.find_message_dependencies(hashes)
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

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics)
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lcc_maybe_missing(topic)
    }

    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        (**self).find_message_dependencies(hashes)
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

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics)
    }

    fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        (**self).lcc_maybe_missing(topic)
    }

    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        (**self).find_message_dependencies(hashes)
    }
}

/// This cursor store always returns 0
#[derive(Default)]
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

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        Ok(HashMap::from_iter(
            topics.map(|t| (t.clone(), GlobalCursor::default())),
        ))
    }

    fn lcc_maybe_missing(&self, _: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    fn find_message_dependencies(
        &self,
        _hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        Ok(HashMap::new())
    }
}
