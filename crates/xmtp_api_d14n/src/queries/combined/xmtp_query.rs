use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    types::{GlobalCursor, Topic},
    xmtp::xmtpv4::envelopes::OriginatorEnvelope,
};

use crate::{
    MigrationClient,
    d14n::QueryEnvelope,
    protocol::{CursorStore, Sort, XmtpEnvelope, XmtpQuery, sort},
};

#[xmtp_common::async_trait]
impl<V3, D14n, Store> XmtpQuery for MigrationClient<V3, D14n, Store>
where
    V3: Client,
    D14n: Client,
    Store: CursorStore,
{
    type Error = ApiClientError;

    // WARN query_at is used only in tests so far, so
    // defaulting to XMTPD is no big deal
    // if query_at is used outside of tests it may not have expected behavior
    // for integrators until after migration cutover date.
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
            .query(&self.xmtpd_grpc)
            .await?
            .envelopes;
        // sort the envelopes by their originator timestamp
        sort::timestamp(&mut envelopes).sort()?;
        Ok(XmtpEnvelope::new(envelopes))
    }
}
