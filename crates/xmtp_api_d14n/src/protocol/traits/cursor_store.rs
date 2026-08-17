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
#[xmtp_common::async_trait]
pub trait CursorStore: MaybeSend + MaybeSync {
    /// Return the highest sequence id seen for each originator on a given topic.
    ///
    /// Pass `None` for `originators` to return cursors for all known originators (used by d14n
    /// callers that subscribe to every originator). Pass `Some(&[...])` to restrict the result
    /// to specific originators (used by v3 callers that only care about e.g. commits + app
    /// messages).
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError>;

    /// Convenience wrapper around [`latest`](Self::latest) that returns a single [`Cursor`] for
    /// one originator. Used when a caller needs the sequence id for exactly one originator on a
    /// topic (e.g. welcome messages on v3).
    async fn latest_for_originator(
        &self,
        topic: &Topic,
        originator: &OriginatorId,
    ) -> Result<Cursor, CursorStoreError> {
        let sid = self
            .latest(topic, Some(&[originator]))
            .await?
            .get(originator);
        Ok(Cursor::new(sid, *originator))
    }

    /// Batch version of [`latest`](Self::latest) — returns the latest cursor for every topic in
    /// the iterator, without originator filtering. Used when subscribing to many group topics at
    /// once so that the stream can resume from the right position per-topic.
    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError>;

    /// Look up the cursor that each locally-published intent depends on, keyed by the intent's
    /// payload hash. The returned cursors are attached as `depends_on` metadata when publishing
    /// group messages so that the ordering layer can enforce causal delivery.
    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError>;

    /// Stash envelopes whose causal dependencies have not yet been seen (the "icebox").
    /// They will be retried later when [`resolve_children`](Self::resolve_children) finds that
    /// their parent cursors have arrived.
    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError>;

    /// Check the icebox for envelopes whose causal dependencies are now satisfied by the given
    /// cursors. Returns the envelopes that are ready to be processed, removing them from the
    /// icebox.
    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError>;

    /// Set the d14n migration cutover timestamp (nanoseconds since epoch). Messages with a
    /// server timestamp at or after this value should be fetched from the d14n network instead
    /// of v3.
    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError>;

    /// Get the d14n migration cutover timestamp (nanoseconds since epoch).
    /// Returns `i64::MAX` when no cutover has been set yet.
    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError>;

    /// Get the last time (nanoseconds since epoch) we polled the network for a migration
    /// cutover update. Used to throttle how often we check.
    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError>;

    /// Record the current time (nanoseconds since epoch) as the last migration-cutover check.
    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError>;

    /// Returns `true` if the d14n migration has been fully completed and the client should
    /// operate exclusively against the d14n network.
    async fn has_migrated(&self) -> Result<bool, CursorStoreError>;

    /// Mark the d14n migration as completed (or not). Once set to `true`, the client stops
    /// querying v3 endpoints entirely.
    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError>;
}

#[xmtp_common::async_trait]
impl<T: CursorStore> CursorStore for Option<T> {
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        if let Some(c) = self {
            c.latest(topic, originators).await
        } else {
            NoCursorStore.latest(topic, originators).await
        }
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        if let Some(c) = self {
            c.latest_for_topics(topics).await
        } else {
            NoCursorStore.latest_for_topics(topics).await
        }
    }

    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        if let Some(c) = self {
            c.find_message_dependencies(hashes).await
        } else {
            NoCursorStore.find_message_dependencies(hashes).await
        }
    }
    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.ice(orphans).await
        } else {
            NoCursorStore.ice(orphans).await
        }
    }

    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        if let Some(c) = self {
            c.resolve_children(cursors).await
        } else {
            NoCursorStore.resolve_children(cursors).await
        }
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_cutover_ns(cutover_ns).await
        } else {
            NoCursorStore.set_cutover_ns(cutover_ns).await
        }
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        if let Some(c) = self {
            c.get_cutover_ns().await
        } else {
            NoCursorStore.get_cutover_ns().await
        }
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        if let Some(c) = self {
            c.has_migrated().await
        } else {
            NoCursorStore.has_migrated().await
        }
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_has_migrated(has_migrated).await
        } else {
            NoCursorStore.set_has_migrated(has_migrated).await
        }
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        if let Some(c) = self {
            c.get_last_checked_ns().await
        } else {
            NoCursorStore.get_last_checked_ns().await
        }
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        if let Some(c) = self {
            c.set_last_checked_ns(last_checked_ns).await
        } else {
            NoCursorStore.set_last_checked_ns(last_checked_ns).await
        }
    }
}

#[xmtp_common::async_trait]
impl<T: CursorStore + ?Sized> CursorStore for &T {
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators).await
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics).await
    }

    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        (**self).find_message_dependencies(hashes).await
    }

    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans).await
    }

    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors).await
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns).await
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns().await
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns().await
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns).await
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated().await
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated).await
    }
}

#[xmtp_common::async_trait]
impl<T: CursorStore + ?Sized> CursorStore for Arc<T> {
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators).await
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics).await
    }

    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        (**self).find_message_dependencies(hashes).await
    }

    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans).await
    }

    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors).await
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns).await
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns().await
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns().await
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns).await
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated().await
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated).await
    }
}

#[xmtp_common::async_trait]
impl<T: CursorStore + ?Sized> CursorStore for Box<T> {
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        (**self).latest(topic, originators).await
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        (**self).latest_for_topics(topics).await
    }

    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        (**self).find_message_dependencies(hashes).await
    }

    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        (**self).ice(orphans).await
    }

    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        (**self).resolve_children(cursors).await
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_cutover_ns(cutover_ns).await
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_cutover_ns().await
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        (**self).get_last_checked_ns().await
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        (**self).set_last_checked_ns(last_checked_ns).await
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        (**self).has_migrated().await
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        (**self).set_has_migrated(has_migrated).await
    }
}

/// This cursor store always returns 0
#[derive(Default, Copy, Clone)]
pub struct NoCursorStore;

#[xmtp_common::async_trait]
impl CursorStore for NoCursorStore {
    async fn latest(
        &self,
        _: &Topic,
        _: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        Ok(GlobalCursor::default())
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        Ok(HashMap::from_iter(
            topics.map(|t| (t.clone(), GlobalCursor::default())),
        ))
    }

    async fn find_message_dependencies(
        &self,
        _hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        Ok(HashMap::new())
    }

    async fn ice(&self, _orphans: Vec<OrphanedEnvelope>) -> Result<(), CursorStoreError> {
        Ok(())
    }

    async fn resolve_children(
        &self,
        _cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, CursorStoreError> {
        Ok(Vec::new())
    }

    async fn set_cutover_ns(&self, _cutover_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(i64::MAX)
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        Ok(0)
    }

    async fn set_last_checked_ns(&self, _last_checked_ns: i64) -> Result<(), CursorStoreError> {
        Ok(())
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        Ok(false)
    }

    async fn set_has_migrated(&self, _has_migrated: bool) -> Result<(), CursorStoreError> {
        Ok(())
    }
}
