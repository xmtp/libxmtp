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

#[xmtp_common::async_trait]
impl<C, Store, E> XmtpQuery for D14nClient<C, Store>
where
    C: Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>>,
    E: RetryableError + 'static,
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
            .query(&self.client)
            .await?;
        Ok(XmtpEnvelope::new(response.envelopes))
    }
}
