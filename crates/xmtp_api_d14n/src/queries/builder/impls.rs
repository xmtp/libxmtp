use std::collections::HashMap;
use std::sync::Arc;

use xmtp_proto::types::{GlobalCursor, Topic};

use crate::protocol::{XmtpEnvelope, XmtpQuery};

#[xmtp_common::async_trait]
impl<T> XmtpQuery for Box<T>
where
    T: XmtpQuery + ?Sized,
{
    type Error = T::Error;

    fn is_d14n(&self) -> Result<bool, Self::Error> {
        <T as XmtpQuery>::is_d14n(&**self)
    }

    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <T as XmtpQuery>::query_at(&**self, topic, at).await
    }

    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error> {
        <T as XmtpQuery>::get_node_clients(&**self).await
    }
}

#[xmtp_common::async_trait]
impl<T> XmtpQuery for Arc<T>
where
    T: XmtpQuery + ?Sized,
{
    type Error = T::Error;

    fn is_d14n(&self) -> Result<bool, Self::Error> {
        <T as XmtpQuery>::is_d14n(&**self)
    }

    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <T as XmtpQuery>::query_at(&**self, topic, at).await
    }

    async fn get_node_clients(
        &self,
    ) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error> {
        <T as XmtpQuery>::get_node_clients(&**self).await
    }
}
