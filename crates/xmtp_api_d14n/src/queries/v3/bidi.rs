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

    // Spans the open handshake (not the stream's lifetime) as `rpc.subscribe_bidi`.
    // Bidi is consumed directly by `xmtp_mls` with no `xmtp_api` wrapper in front,
    // so this transport impl is the RPC boundary — the same layer the other
    // `rpc_span`s live at for unary calls. Move it up if a wrapper is ever added.
    #[xmtp_common::rpc_span]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::protocol::NoCursorStore;
    use futures::stream;
    use xmtp_common::BoxDynStream;
    use xmtp_proto::api::BytesStream;
    use xmtp_proto::api::mock::MockNetworkClient;
    use xmtp_proto::mls_v1::subscribe_request::v1::Mutate;
    use xmtp_proto::mls_v1::{Ping, Pong, subscribe_request, subscribe_response};

    fn req(request: subscribe_request::v1::Request) -> SubscribeRequest {
        SubscribeRequest {
            version: Some(subscribe_request::Version::V1(subscribe_request::V1 {
                request: Some(request),
            })),
        }
    }

    fn resp(response: subscribe_response::v1::Response) -> SubscribeResponse {
        SubscribeResponse {
            version: Some(subscribe_response::Version::V1(subscribe_response::V1 {
                response: Some(response),
            })),
        }
    }

    fn ping_req(nonce: u64) -> SubscribeRequest {
        req(subscribe_request::v1::Request::Ping(Ping { nonce }))
    }

    /// `subscribe_bidi` must prost-encode each outbound `SubscribeRequest` in
    /// order, dial the `Subscribe` path, and decode the inbound byte frames back
    /// into `SubscribeResponse`s through `XmtpStream`.
    #[xmtp_common::test(unwrap_try = true)]
    async fn encodes_outbound_and_decodes_inbound() {
        let captured: std::sync::Arc<std::sync::Mutex<Option<BoxDynStream<'static, Bytes>>>> =
            Default::default();
        let sink = captured.clone();

        let mut mock = MockNetworkClient::new();
        mock.expect_bidi_stream()
            .return_once(move |_req, path, body| {
                assert_eq!(path.path(), "/xmtp.mls.api.v1.MlsApi/Subscribe");
                *sink.lock().unwrap() = Some(body);
                let frames: Vec<Result<Bytes, ApiClientError>> = vec![
                    Ok(Bytes::from(
                        resp(subscribe_response::v1::Response::Ping(Ping { nonce: 7 }))
                            .encode_to_vec(),
                    )),
                    Ok(Bytes::from(
                        resp(subscribe_response::v1::Response::Pong(Pong { nonce: 9 }))
                            .encode_to_vec(),
                    )),
                ];
                Ok(http::Response::new(BytesStream::new(stream::iter(frames))))
            });

        let client = V3Client::new(mock, NoCursorStore);
        let outbound = stream::iter(vec![
            req(subscribe_request::v1::Request::Mutate(Mutate::default())),
            ping_req(3),
        ])
        .boxed();

        let inbound = client.subscribe_bidi(outbound).await?;
        let decoded: Vec<SubscribeResponse> = inbound.map(|r| r.unwrap()).collect().await;
        assert_eq!(
            decoded,
            vec![
                resp(subscribe_response::v1::Response::Ping(Ping { nonce: 7 })),
                resp(subscribe_response::v1::Response::Pong(Pong { nonce: 9 })),
            ],
        );

        // The outbound stream handed to the transport carries the same requests,
        // prost-encoded, in order. Take it out of the mutex before awaiting so no
        // lock is held across the `.await`.
        let captured_outbound = captured.lock().unwrap().take().unwrap();
        let sent_bytes: Vec<Bytes> = captured_outbound.collect().await;
        let sent: Vec<SubscribeRequest> = sent_bytes
            .iter()
            .map(|b| SubscribeRequest::decode(b.clone()).unwrap())
            .collect();
        assert_eq!(
            sent,
            vec![
                req(subscribe_request::v1::Request::Mutate(Mutate::default())),
                ping_req(3),
            ],
        );
    }

    /// A transport error opening the stream is surfaced as a `ClientWithEndpoint`
    /// tagged with the `Subscribe` path, not a bare client error.
    #[xmtp_common::test(unwrap_try = true)]
    async fn tags_open_error_with_subscribe_endpoint() {
        #[derive(Debug, thiserror::Error)]
        #[error("boom")]
        struct Boom;
        impl xmtp_common::RetryableError for Boom {
            fn is_retryable(&self) -> bool {
                false
            }
        }

        let mut mock = MockNetworkClient::new();
        mock.expect_bidi_stream()
            .return_once(|_req, _path, _body| Err(ApiClientError::client(Boom)));

        let client = V3Client::new(mock, NoCursorStore);
        let outbound = stream::iter(vec![ping_req(1)]).boxed();

        // `XmtpStream` isn't `Debug`, so match instead of `unwrap_err`.
        match client.subscribe_bidi(outbound).await {
            Err(ApiClientError::ClientWithEndpoint { endpoint, .. }) => {
                assert_eq!(endpoint, "/xmtp.mls.api.v1.MlsApi/Subscribe");
            }
            Err(other) => panic!("expected ClientWithEndpoint, got {other:?}"),
            Ok(_) => panic!("subscribe_bidi should error when the transport fails to open"),
        }
    }
}
