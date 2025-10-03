use crate::{V3Client, v3::*};
use xmtp_api_grpc::error::GrpcError;
use xmtp_api_grpc::streams::{TryFromItem, try_from_stream};
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::mls_v1;
use xmtp_proto::types::{GroupMessage, WelcomeMessage};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpMlsStreams for V3Client<C>
where
    C: Send + Sync + Client<Error = GrpcError>,
{
    type Error = ApiClientError<GrpcError>;

    type GroupMessageStream =
        TryFromItem<XmtpStream<<C as Client>::Stream, V3ProtoGroupMessage>, GroupMessage>;

    type WelcomeMessageStream =
        TryFromItem<XmtpStream<<C as Client>::Stream, V3ProtoWelcomeMessage>, WelcomeMessage>;

    async fn subscribe_group_messages(
        &self,
        req: mls_v1::SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        Ok(try_from_stream(
            SubscribeGroupMessages::builder()
                .filters(req.filters)
                .build()?
                .stream(&self.client)
                .await?,
        ))
    }

    async fn subscribe_welcome_messages(
        &self,
        req: mls_v1::SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        Ok(try_from_stream(
            SubscribeWelcomeMessages::builder()
                .filters(req.filters)
                .build()?
                .stream(&self.client)
                .await?,
        ))
    }
}
