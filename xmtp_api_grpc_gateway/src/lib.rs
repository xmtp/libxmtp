use async_trait::async_trait;
use futures_util::stream::LocalBoxStream;
use futures_util::{FutureExt, StreamExt, TryStreamExt};
use serde::Deserialize;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;
use xmtp_proto::api_client::{Error, ErrorKind, XmtpApiClient};
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse, QueryRequest,
    QueryResponse, SubscribeRequest,
};

// TODO: consider moving these (and other address const) into `xmtp_proto`
pub const LOCALHOST_ADDRESS: &str = "http://localhost:5555";
pub const DEV_ADDRESS: &str = "https://dev.xmtp.network:5555";

pub struct XmtpGrpcGatewayClient {
    url: String,
    http: reqwest::Client,
}

impl XmtpGrpcGatewayClient {
    pub fn new(url: String) -> Self {
        XmtpGrpcGatewayClient {
            url,
            http: reqwest::Client::new(),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl XmtpApiClient for XmtpGrpcGatewayClient {
    type Subscription = LocalBoxStream<'static, Envelope>;

    fn set_app_version(&mut self, _version: String) {
        // TODO
    }

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        let response = self
            .http
            .post(&format!("{}/message/v1/publish", self.url))
            .bearer_auth(token)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))?;
        Ok(response
            .json::<PublishResponse>()
            .await
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))?)
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error> {
        // grpc-gateway streams newline-delimited JSON bodies
        let response = self
            .http
            .post(&format!("{}/message/v1/subscribe", self.url))
            .json(&request)
            .send()
            .into_stream()
            .filter_map(|r| async move { r.ok() })
            .map(|r| r.bytes_stream())
            .flatten()
            .boxed_local();

        let bytes_reader = StreamReader::new(
            response.map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)),
        );
        let codec = LinesCodec::new_with_max_length(1024 * 1024);
        let frames_reader = FramedRead::with_capacity(bytes_reader, codec, 8 * 1024 * 1024);
        let stream = frames_reader
            .map(|frame_res| match frame_res {
                Ok(frame_str) => serde_json::from_str::<SubscribeResult>(frame_str.as_str())
                    .map(|v| v.result)
                    .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e)),
                Err(err) => Err(Error::new(ErrorKind::SubscribeError).with(err)),
            })
            // Discard any messages that we can't parse.
            // TODO: consider surfacing these in a log somewhere
            .filter_map(|r| async move { r.ok() })
            .boxed_local();
        return Ok(stream);
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        let response = self
            .http
            .post(&format!("{}/message/v1/query", self.url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::QueryError).with(e))?;
        Ok(response
            .json::<QueryResponse>()
            .await
            .map_err(|e| Error::new(ErrorKind::QueryError).with(e))?)
    }

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        let response = self
            .http
            .post(&format!("{}/message/v1/batch-query", self.url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::BatchQueryError).with(e))?;
        Ok(response
            .json::<BatchQueryResponse>()
            .await
            .map_err(|e| Error::new(ErrorKind::BatchQueryError).with(e))?)
    }
}

// The result of calling .subscribe()
// The grpc-gateway streams newline-delimited JSON bodies
// in this shape:
//  { result: { ... Envelope ... } }\n
//  { result: { ... Envelope ... } }\n
//  { result: { ... Envelope ... } }\n
// So we use this to pluck the `Envelope` as the `result`.
#[derive(Deserialize, Debug)]
struct SubscribeResult {
    result: Envelope,
}
