use std::collections::HashMap;

use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::{
    api::{ApiClientError, Client, Query},
    types::{GlobalCursor, Topic},
    xmtp::xmtpv4::envelopes::OriginatorEnvelope,
};

use crate::{
    MigrationClient,
    d14n::{GetNodes, QueryEnvelope},
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

    fn is_d14n(&self) -> Result<bool, Self::Error> {
        Ok(self.store.has_migrated()?)
    }

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

    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, Box<dyn Client + Send + Sync>>, Self::Error> {
        use xmtp_api_grpc::GrpcClient;
        use xmtp_proto::prelude::{ApiBuilder, NetConnectConfig};

        let response = GetNodes::builder().build()?.query(&self.xmtpd_grpc).await?;

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
