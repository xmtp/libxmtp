use std::collections::HashMap;
use std::sync::Arc;
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::{
    api::ApiClientError,
    types::{Cursor, GlobalCursor, OriginatorId, OrphanedEnvelope, Topic, TopicKind},
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
        Ok(Cursor::new(sid, *originator))
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

    /// ice envelopes that cannot yet be processed
    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError>;

    /// try to resolve any children that may depend on [`Cursor`]
    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError>;

    /// Update the d14n migration cutover timestamp (nanoseconds)
    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError>;

    /// Get the d14n migration cutover timestamp (nanoseconds)
    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError>;

    /// Get the last time we checked for migration cutover (nanoseconds)
    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError>;

    /// Update the last time we checked for migration cutover (nanoseconds)
    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError>;

    /// Check whether the d14n migration has already been completed
    fn has_migrated(&self) -> Result<bool, CursorStoreError>;

    /// Mark the d14n migration as completed
    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError>;
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
    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.ice(orphans)
        } else {
            NoCursorStore.ice(orphans)
        }
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        if let Some(c) = self {
            c.resolve_children(cursors)
        } else {
            NoCursorStore.resolve_children(cursors)
        }
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_cutover_ns(cutover_ns)
        } else {
            NoCursorStore.set_cutover_ns(cutover_ns)
        }
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        if let Some(c) = self {
            c.get_cutover_ns()
        } else {
            NoCursorStore.get_cutover_ns()
        }
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        if let Some(c) = self {
            c.has_migrated()
        } else {
            NoCursorStore.has_migrated()
        }
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_has_migrated(has_migrated)
        } else {
            NoCursorStore.set_has_migrated(has_migrated)
        }
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        if let Some(c) = self {
            c.get_last_checked_ns()
        } else {
            NoCursorStore.get_last_checked_ns()
        }
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_last_checked_ns(last_checked_ns)
        } else {
            NoCursorStore.set_last_checked_ns(last_checked_ns)
        }
    }
}

impl<T: CursorStore + ?Sized> CursorStore for &T {
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

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans)
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors)
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns)
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns()
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns()
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns)
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated()
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated)
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

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans)
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors)
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns)
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns()
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns()
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns)
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated()
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated)
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

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans)
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors)
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns)
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns()
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns()
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns)
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated()
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated)
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

    fn ice(&self, _orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        Ok(())
    }

    fn resolve_children(
        &self,
        _cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        Ok(Vec::new())
    }

    fn set_cutover_ns(&self, _cutover_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(i64::MAX)
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(0)
    }

    fn set_last_checked_ns(&self, _last_checked_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        Ok(false)
    }

    fn set_has_migrated(&self, _has_migrated: bool) -> Result<(), CursorStoreError> {
        Ok(())
    }
}
