use std::collections::HashMap;

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
impl<C, Store> XmtpQuery for D14nClient<C, Store>
where
    C: Client,
    Store: CursorStore,
{
    type Error = ApiClientError;

    fn is_d14n(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }

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

    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error> {
        crate::queries::build_node_clients(&self.client, self.app_version.as_ref()).await
    }
}
