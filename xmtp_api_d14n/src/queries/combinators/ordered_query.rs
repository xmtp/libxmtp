use std::marker::PhantomData;

use xmtp_common::RetryableError;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    api_client::Paged,
    types::TopicCursor,
};

use crate::protocol::{
    CursorStore, Ordered, OrderedEnvelopeCollection, ProtocolEnvelope, ResolveDependencies,
    TypedNoopResolver,
};

pub struct OrderedQuery<E, R, T, S> {
    endpoint: E,
    resolver: R,
    topic_cursor: TopicCursor,
    store: S,
    _marker: PhantomData<T>,
    offline: bool,
}

#[xmtp_common::async_trait]
impl<E, C, R, T, S> Query<C> for OrderedQuery<E, R, T, S>
where
    E: Query<C, Output = T>,
    C: Client,
    C::Error: RetryableError,
    R: ResolveDependencies<ResolvedEnvelope = <T as Paged>::Message>,
    T: Default + prost::Message + Paged + 'static,
    S: CursorStore,
    T::Message: ProtocolEnvelope<'static> + prost::Message + Default,
{
    type Output = Vec<T::Message>;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>> {
        let envelopes = Query::<C>::query(&mut self.endpoint, client)
            .await?
            .messages();
        let mut ordering = Ordered::builder()
            .envelopes(envelopes)
            .resolver(&self.resolver)
            .store(&self.store)
            .topic_cursor(&mut self.topic_cursor)
            .build()?;
        if self.offline {
            ordering.order_offline().map_err(ApiClientError::other)?;
        } else {
            ordering.order().await.map_err(ApiClientError::other)?;
        }
        Ok(ordering.into_envelopes())
    }
}

pub fn ordered<E, R, T, S>(
    endpoint: E,
    resolver: R,
    topic_cursor: TopicCursor,
    store: S,
) -> OrderedQuery<E, R, T, S> {
    OrderedQuery::<E, R, T, S> {
        endpoint,
        resolver,
        topic_cursor,
        store,
        _marker: PhantomData,
        offline: false,
    }
}

pub fn offline_ordered<E, T, TResolvedEnvelope, S>(
    endpoint: E,
    topic_cursor: TopicCursor,
    store: S,
) -> OrderedQuery<E, TypedNoopResolver<TResolvedEnvelope>, T, S> {
    OrderedQuery::<E, TypedNoopResolver<TResolvedEnvelope>, T, S> {
        endpoint,
        resolver: TypedNoopResolver::<TResolvedEnvelope>::new(),
        topic_cursor,
        store,
        _marker: PhantomData,
        offline: true,
    }
}
