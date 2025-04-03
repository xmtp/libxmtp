use super::D14nClient;
use futures::stream;
use xmtp_common::RetryableError;
use xmtp_proto::XmtpApiError;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::mls_v1;
use xmtp_proto::traits::{ApiClientError, Client};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpMlsStreams for D14nClient<C, P>
where
    C: Send + Sync + Client<Error = E>,
    P: Send + Sync + Client,
    E: XmtpApiError + std::error::Error + RetryableError + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    #[cfg(not(target_arch = "wasm32"))]
    type GroupMessageStream<'a>
        = stream::BoxStream<'a, Result<mls_v1::GroupMessage, Self::Error>>
    where
        C: 'a,
        P: 'a;
    #[cfg(not(target_arch = "wasm32"))]
    type WelcomeMessageStream<'a>
        = stream::BoxStream<'a, Result<mls_v1::WelcomeMessage, Self::Error>>
    where
        C: 'a,
        P: 'a;

    #[cfg(target_arch = "wasm32")]
    type GroupMessageStream<'a>
        = stream::LocalBoxStream<'a, Result<mls_v1::GroupMessage, Self::Error>>
    where
        C: 'a;
    #[cfg(target_arch = "wasm32")]
    type WelcomeMessageStream<'a>
        = stream::LocalBoxStream<'a, Result<mls_v1::WelcomeMessage, Self::Error>>
    where
        C: 'a;

    async fn subscribe_group_messages(
        &self,
        _request: mls_v1::SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error> {
        todo!()
    }

    async fn subscribe_welcome_messages(
        &self,
        _request: mls_v1::SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error> {
        todo!()
    }
}
