//! D14n-specific api combinators

use xmtp_proto::{api::Endpoint, api_client::Paged, types::TopicCursor};

use crate::protocol::{CursorStore, ResolveDependencies, TypedNoopResolver};

mod ordered_query;

// the resolved envelope type for the resolver is the `Message` associated type on the `Paged`
// trait. (the nested vec in a protobuf wrapper).
type OfflineResolver<T, S> = TypedNoopResolver<<<T as Endpoint<S>>::Output as Paged>::Message>;

pub trait D14nCombinatorExt<S>: Endpoint<S> {
    /// order envelopes and try to resolve any missing envelopes according to resolver,
    /// [`ResolveDependencies`](crate::protocol::ResolveDependencies)
    fn ordered<R, Store>(
        self,
        resolver: R,
        topic_cursor: TopicCursor,
        store: Store,
    ) -> ordered_query::OrderedQuery<Self, R, <Self as Endpoint<S>>::Output, Store>
    where
        Self: Sized + Endpoint<S>,
        <Self as Endpoint<S>>::Output: Paged,
        R: ResolveDependencies,
        Store: CursorStore,
    {
        ordered_query::ordered(self, resolver, topic_cursor, store)
    }

    /// order envelopes without extra network queries
    /// if any envelopes are missing, they are put into an icebox and will be resolved on future
    /// queries.
    fn offline_ordered<Store>(
        self,
        topic_cursor: TopicCursor,
        store: Store,
    ) -> ordered_query::OrderedQuery<
        Self,
        OfflineResolver<Self, S>,
        <Self as Endpoint<S>>::Output,
        Store,
    >
    where
        Self: Sized + Endpoint<S>,
        <Self as Endpoint<S>>::Output: Paged,
        Store: CursorStore,
    {
        ordered_query::offline_ordered(self, topic_cursor, store)
    }
}

impl<S, E> D14nCombinatorExt<S> for E where E: Endpoint<S> {}
