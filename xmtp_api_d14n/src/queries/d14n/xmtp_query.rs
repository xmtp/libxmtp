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
    protocol::{XmtpEnvelope, XmtpQuery},
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G, E> XmtpQuery for D14nClient<C, G>
where
    C: Client<Error = E>,
    G: Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>>,
    E: RetryableError + 'static,
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
