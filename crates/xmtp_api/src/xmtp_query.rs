use crate::ApiClientWrapper;
use std::collections::HashMap;
use xmtp_api_d14n::protocol::{XmtpEnvelope, XmtpQuery};
use xmtp_proto::types::{GlobalCursor, Topic};

#[xmtp_common::async_trait]
impl<C> XmtpQuery for ApiClientWrapper<C>
where
    C: XmtpQuery,
{
    type Error = <C as XmtpQuery>::Error;

    fn is_d14n(&self) -> Result<bool, Self::Error> {
        <C as XmtpQuery>::is_d14n(&self.api_client)
    }

    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <C as XmtpQuery>::query_at(&self.api_client, topic, at).await
    }

    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error> {
        <C as XmtpQuery>::get_node_clients(&self.api_client).await
    }
}
