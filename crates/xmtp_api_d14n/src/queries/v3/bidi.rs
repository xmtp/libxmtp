//! XIP-83 bidirectional Subscribe transport for the v3 backend (native-only).

use crate::V3Client;
use crate::protocol::CursorStore;
use futures::StreamExt;
use prost::Message;
use prost::bytes::Bytes;
use xmtp_proto::ApiEndpoint;
use xmtp_proto::api::{ApiClientError, Client, XmtpStream};
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::mls_v1::{SubscribeRequest, SubscribeResponse};

const SUBSCRIBE_PATH: &str = "/xmtp.mls.api.v1.MlsApi/Subscribe";

#[xmtp_common::async_trait]
impl<C, Store> XmtpMlsBidiStreams for V3Client<C, Store>
where
    C: Client,
    Store: CursorStore,
{
    type SubscribeStream = XmtpStream<SubscribeResponse>;

    type Error = ApiClientError;

    async fn subscribe_bidi(
        &self,
        requests: futures::stream::BoxStream<'static, SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error> {
        tracing::debug!("opening bidirectional subscription");
        let outbound = requests.map(|frame| Bytes::from(frame.encode_to_vec()));
        let response = self
            .client
            .bidi_stream(
                http::Request::builder(),
                http::uri::PathAndQuery::from_static(SUBSCRIBE_PATH),
                Box::pin(outbound),
            )
            .await
            .map_err(|e| e.endpoint(SUBSCRIBE_PATH.to_string()))?;
        Ok(XmtpStream::new(
            response.into_body(),
            ApiEndpoint::Path(SUBSCRIBE_PATH.to_string()),
        ))
    }
}
