use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    types::{GlobalCursor, Topic},
    xmtp::xmtpv4::envelopes::OriginatorEnvelope,
};

use crate::{
    D14nClient,
    d14n::QueryEnvelope,
    protocol::{CursorStore, Sort, XmtpEnvelope, XmtpQuery, sort},
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
        let mut envelopes: Vec<OriginatorEnvelope> = QueryEnvelope::builder()
            .topic(topic)
            .last_seen(at.unwrap_or_default())
            .limit(MAX_PAGE_SIZE)
            .build()?
            .query(&self.client)
            .await?
            .envelopes;
        // sort the envelopes by their originator timestamp
        sort::timestamp(&mut envelopes).sort()?;
        Ok(XmtpEnvelope::new(envelopes))
    }
}
