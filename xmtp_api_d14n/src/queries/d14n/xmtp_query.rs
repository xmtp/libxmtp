use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    types::{GlobalCursor, Topic},
    xmtp::xmtpv4::message_api::QueryEnvelopesResponse,
};

use crate::{
    D14nClient,
    d14n::QueryEnvelope,
    protocol::{CursorStore, XmtpEnvelope, XmtpQuery},
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G, Store, E> XmtpQuery for D14nClient<C, G, Store>
where
    C: Client<Error = E>,
    G: Client<Error = <C as Client>::Error>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>> + Send + Sync + 'static,
    E: RetryableError,
    Store: CursorStore,
{
    type Error = ApiClientError<E>;

    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        let response: QueryEnvelopesResponse = QueryEnvelope::builder()
            .topic(topic)
            .last_seen(at.unwrap_or_default())
            .limit(MAX_PAGE_SIZE)
            .build()?
            .query(&self.message_client)
            .await?;
        Ok(XmtpEnvelope::new(response.envelopes))
    }
}
