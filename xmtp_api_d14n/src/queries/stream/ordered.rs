//! Orders a stream with an [`Order`](crate::protocol::Ordered) according to XMTP XIP-49

use crate::protocol::{
    CursorStore, Envelope, EnvelopeError, Ordered, OrderedEnvelopeCollection, ResolutionError,
    TypedNoopResolver,
};
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use std::{
    error::Error,
    marker::PhantomData,
    task::{Poll, ready},
};
use xmtp_common::RetryableError;
use xmtp_proto::{api::ApiClientError, types::TopicCursor};

pin_project! {
    pub struct OrderedStream<S, Store, T> {
        #[pin] inner: S,
        cursor_store: Store,
        topic_cursor: TopicCursor,
        _marker: PhantomData<T>
    }
}

// this is an error which should never occur,
// and if it does is a bug in libxmtp
#[derive(thiserror::Error, Debug)]
pub enum OrderedStreamError {
    #[error(transparent)]
    Resolver(#[from] ResolutionError),
}

impl<E: Error> From<OrderedStreamError> for ApiClientError<E> {
    fn from(value: OrderedStreamError) -> Self {
        ApiClientError::Other(Box::new(value) as _)
    }
}

impl RetryableError for OrderedStreamError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl From<OrderedStreamError> for EnvelopeError {
    fn from(value: OrderedStreamError) -> Self {
        EnvelopeError::DynError(Box::new(value) as _)
    }
}

/// Wrap a `TryStream<T>` who's items are [Envelope's](crate::protocol::Envelope) such that
/// it orders the envelopes according to [XIP-49 Cross-Originator Message Ordering](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#335-cross-originator-message-ordering).
/// If an envelope cannot yet be processed due to missing required dependencies, the streamed
/// message will be put into a persistent "icebox" until the required dependency is streamed.
/// This stream implementation will _not_ attempt to do any further dependency resolution
/// with [`ResolveDependencies`](crate::protocol::ResolveDependencies). there is an implicit
/// assumption that if an item in the stream is required for processing,
/// it will at some point be made available in the stream.
/// This stream instead uses the [`NoopResolver`](crate::protocol::NoopResolver)
pub fn ordered<S, Store, T>(
    s: S,
    cursor_store: Store,
    initial_topic_cursor: TopicCursor,
) -> OrderedStream<S, Store, T> {
    OrderedStream::<S, Store, T> {
        inner: s,
        cursor_store,
        topic_cursor: initial_topic_cursor,
        _marker: PhantomData,
    }
}

impl<S, Store, T> Stream for OrderedStream<S, Store, T>
where
    S: TryStream<Ok = Vec<T>>,
    T: Envelope<'static> + prost::Message + Default + Clone,
    S::Error: From<EnvelopeError> + From<OrderedStreamError>,
    Store: CursorStore,
{
    type Item = Result<S::Ok, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let item = match ready!(self.as_mut().project().inner.try_poll_next(cx)) {
            Some(v) => v,
            None => return Poll::Ready(None),
        };
        let envelopes = item?;
        let mut ordering = Ordered::builder()
            .envelopes(envelopes)
            .resolver(TypedNoopResolver::<T>::new())
            .topic_cursor(self.topic_cursor.clone())
            .store(&self.cursor_store)
            .build()?;
        ordering.order_offline().map_err(OrderedStreamError::from)?;
        let (envelopes, mut new_cursor) = ordering.into_parts();
        let this = self.as_mut().project();
        std::mem::swap(this.topic_cursor, &mut new_cursor);
        Poll::Ready(Some(Ok(envelopes)))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::{InMemoryCursorStore, test::missing_dependencies};
    use futures::{FutureExt, StreamExt, future, stream};
    use proptest::prelude::*;
    use xmtp_proto::api::VectorClock;

    proptest! {
        #[xmtp_common::test]
        fn orders_stream_and_ices_missing(
            envelopes in missing_dependencies(10, vec![10, 20, 30])
        ) {
            let store = InMemoryCursorStore::new();
            let envs = envelopes.envelopes.clone();
            let s = stream::once(future::ready(Ok::<_, EnvelopeError>(envs)));
            let ordered_stream = ordered(s, store.clone(), TopicCursor::default());
            futures::pin_mut!(ordered_stream);

            let result = ordered_stream.next().now_or_never()
                .expect("Stream should yield immediately")
                .expect("Stream should not be empty")
                .expect("Should not error");

            let mut topic_cursor = TopicCursor::default();
            for env in &result {
                let topic = env.topic().unwrap();
                topic_cursor.entry(topic.clone()).or_default().apply(&env.cursor());
            }

            for env in &result {
                let topic = env.topic().unwrap();
                let clock = topic_cursor.get_or_default(&topic);
                prop_assert!(
                    clock.dominates(&env.depends_on()),
                    "Envelope {} should have satisfied dependencies. Topic clock: {}",
                    env, clock
                );
            }

            // Verify that if dependencies are missing, some envelopes are iced
            if !envelopes.removed.is_empty() {
                let has_deps_on_removed = envelopes.envelopes.iter()
                    .any(|e| envelopes.removed.iter().any(|r| e.has_dependency_on(r)));
                if has_deps_on_removed {
                    prop_assert!(!store.icebox().is_empty(),
                        "Expected some envelopes to be iced when dependencies are missing");
                }
            }
        }
    }
}
