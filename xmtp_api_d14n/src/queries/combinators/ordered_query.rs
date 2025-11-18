use std::marker::PhantomData;

use xmtp_common::RetryableError;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    api_client::Paged,
    types::TopicCursor,
};

use crate::protocol::{Ordered, OrderedEnvelopeCollection, ProtocolEnvelope, ResolveDependencies};

pub struct OrderedQuery<E, R, T> {
    endpoint: E,
    resolver: R,
    topic_cursor: TopicCursor,
    _marker: PhantomData<T>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C, R, T> Query<C> for OrderedQuery<E, R, T>
where
    E: Query<C, Output = T>,
    C: Client,
    C::Error: RetryableError,
    R: ResolveDependencies<ResolvedEnvelope = <T as Paged>::Message> + Clone,
    T: Default + prost::Message + Paged + 'static,
    for<'a> T::Message: ProtocolEnvelope<'a> + Clone,
{
    type Output = Vec<T::Message>;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>> {
        let envelopes = Query::<C>::query(&mut self.endpoint, client)
            .await?
            .messages();
        let mut ordering = Ordered::builder()
            .envelopes(envelopes)
            .resolver(&self.resolver)
            // todo: maybe no clone here?
            .topic_cursor(self.topic_cursor.clone())
            .build()?;
        ordering.order().await.map_err(ApiClientError::other)?;
        let (envelopes, _) = ordering.into_parts();
        Ok(envelopes)
    }
}

pub fn ordered<E, R, T>(
    endpoint: E,
    resolver: R,
    topic_cursor: TopicCursor,
) -> OrderedQuery<E, R, T> {
    OrderedQuery::<E, R, T> {
        endpoint,
        resolver,
        topic_cursor,
        _marker: PhantomData,
    }
}
