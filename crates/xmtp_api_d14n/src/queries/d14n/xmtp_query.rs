use std::collections::HashMap;

use xmtp_api_grpc::GrpcClient;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    prelude::{ApiBuilder, NetConnectConfig},
    types::{GlobalCursor, Topic},
    xmtp::xmtpv4::envelopes::OriginatorEnvelope,
};

use crate::{
    D14nClient,
    d14n::{GetNodes, QueryEnvelope},
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
    ) -> Result<HashMap<u32, Box<dyn Client + Send + Sync>>, Self::Error> {
        let response = GetNodes::builder().build()?.query(&self.client).await?;

        let mut clients: HashMap<u32, Box<dyn Client + Send + Sync>> = HashMap::new();
        for (node_id, url) in response.nodes {
            let mut builder = GrpcClient::builder();
            match url.parse() {
                Ok(host) => {
                    builder.set_host(host);
                    match builder.build() {
                        Ok(client) => {
                            clients.insert(node_id, Box::new(client));
                        }
                        Err(e) => {
                            tracing::warn!(
                                node_id,
                                %url,
                                error = %e,
                                "get_node_clients: failed to build grpc client for node"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        node_id,
                        %url,
                        error = %e,
                        "get_node_clients: failed to parse url for node"
                    );
                }
            }
        }
        Ok(clients)
    }
}
