use crate::TryExtractorStream;
use crate::d14n::SubscribeEnvelopes;
use crate::protocol::{EnvelopeCollection, GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;

use super::D14nClient;
use xmtp_common::{MaybeSend, RetryableError};
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::mls_v1;
use xmtp_proto::xmtp::xmtpv4::message_api::{EnvelopesQuery, SubscribeEnvelopesResponse};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, P, E> XmtpMlsStreams for D14nClient<C, P>
where
    C: Send + Sync + Client<Error = E>,
    <C as Client>::Stream: MaybeSend + 'static,
    P: Send + Sync + Client<Error = E>,
    E: std::error::Error + RetryableError + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    type GroupMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        GroupMessageExtractor,
    >;

    type WelcomeMessageStream = TryExtractorStream<
        XmtpStream<<C as Client>::Stream, SubscribeEnvelopesResponse>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        _request: mls_v1::SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        let _stream = SubscribeEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: _request.topics()?,
                originator_node_ids: todo!(),
                last_seen: todo!(), // TODO: requires cursor store
            })
            .build()?
            .stream(&self.message_client)
            .await?;
        Ok(stream::try_extractor(_stream))
    }

    async fn subscribe_welcome_messages(
        &self,
        request: mls_v1::SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        let _stream = SubscribeEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: request.topics()?,
                originator_node_ids: todo!(),
                last_seen: todo!(), // TODO: requires cursor store
            })
            .build()?
            .stream(&self.message_client)
            .await?;
        Ok(stream::try_extractor(_stream))
    }
}
