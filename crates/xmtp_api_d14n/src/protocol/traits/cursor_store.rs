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

impl From<CursorStoreError> for ApiClientError {
    fn from(value: CursorStoreError) -> Self {
        ApiClientError::Other(Box::new(value) as Box<_>)
    }
}

/// Trait defining how cursors should be stored, updated, and fetched
/// _NOTE:_, implementations decide retry strategy. the exact implementation of persistence (or lack)
/// is up to implementors. functions are assumed to be idempotent & atomic.
pub trait CursorStore: MaybeSend + MaybeSync {
    /// Return the highest sequence id seen for each originator on a given topic.
    ///
    /// Pass `None` for `originators` to return cursors for all known originators (used by d14n
    /// callers that subscribe to every originator). Pass `Some(&[...])` to restrict the result
    /// to specific originators (used by v3 callers that only care about e.g. commits + app
    /// messages).
    fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError>;

    /// Convenience wrapper around [`latest`](Self::latest) that returns a single [`Cursor`] for
    /// one originator. Used when a caller needs the sequence id for exactly one originator on a
    /// topic (e.g. welcome messages on v3).
    fn latest_for_originator(
        &self,
        topic: &Topic,
        originator: &OriginatorId,
    ) -> Result<Cursor, CursorStoreError> {
        let sid = self.latest(topic, Some(&[originator]))?.get(originator);
        Ok(Cursor::new(sid, *originator))
    }

    /// Batch version of [`latest`](Self::latest) — returns the latest cursor for every topic in
    /// the iterator, without originator filtering. Used when subscribing to many group topics at
    /// once so that the stream can resume from the right position per-topic.
    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError>;

    /// Look up the cursor that each locally-published intent depends on, keyed by the intent's
    /// payload hash. The returned cursors are attached as `depends_on` metadata when publishing
    /// group messages so that the ordering layer can enforce causal delivery.
    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError>;

    /// Stash envelopes whose causal dependencies have not yet been seen (the "icebox").
    /// They will be retried later when [`resolve_children`](Self::resolve_children) finds that
    /// their parent cursors have arrived.
    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError>;

    /// Check the icebox for envelopes whose causal dependencies are now satisfied by the given
    /// cursors. Returns the envelopes that are ready to be processed, removing them from the
    /// icebox.
    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError>;

    /// Set the d14n migration cutover timestamp (nanoseconds since epoch). Messages with a
    /// server timestamp at or after this value should be fetched from the d14n network instead
    /// of v3.
    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError>;

    /// Get the d14n migration cutover timestamp (nanoseconds since epoch).
    /// Returns `i64::MAX` when no cutover has been set yet.
    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError>;

    /// Get the last time (nanoseconds since epoch) we polled the network for a migration
    /// cutover update. Used to throttle how often we check.
    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError>;

    /// Record the current time (nanoseconds since epoch) as the last migration-cutover check.
    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError>;

    /// Returns `true` if the d14n migration has been fully completed and the client should
    /// operate exclusively against the d14n network.
    fn has_migrated(&self) -> Result<bool, CursorStoreError>;

    /// Mark the d14n migration as completed (or not). Once set to `true`, the client stops
    /// querying v3 endpoints entirely.
    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError>;
}

impl<T: CursorStore> CursorStore for Option<T> {
    fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest(topic, originators)
        } else {
            NoCursorStore.latest(topic, originators)
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
    fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators)
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics)
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
    fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators)
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics)
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
    fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators)
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics)
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
    fn latest(
        &self,
        _: &Topic,
        _: Option<&[&OriginatorId]>,
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
