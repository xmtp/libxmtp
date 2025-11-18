//! D14n-specific api combinators

use xmtp_proto::{api::Endpoint, api_client::Paged, types::TopicCursor};

use crate::protocol::ResolveDependencies;

mod ordered_query;

pub trait D14nCombinatorExt<S>: Endpoint<S> {
    fn ordered<R>(
        self,
        resolver: R,
        topic_cursor: TopicCursor,
    ) -> ordered_query::OrderedQuery<Self, R, <Self as Endpoint<S>>::Output>
    where
        Self: Sized + Endpoint<S>,
        <Self as Endpoint<S>>::Output: Paged,
        R: ResolveDependencies,
    {
        ordered_query::ordered(self, resolver, topic_cursor)
    }
}

impl<S, E> D14nCombinatorExt<S> for E where E: Endpoint<S> {}
