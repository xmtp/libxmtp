use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::LocalBoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{FutureExt, StreamExt, TryStreamExt};
use serde::Deserialize;
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;
use xmtp_proto::api_client::{Error, ErrorKind, XmtpApiClient, XmtpApiSubscription};
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse, QueryRequest,
    QueryResponse, SubscribeRequest,
};

// TODO: consider moving these (and other address const) into `xmtp_proto`
pub const LOCALHOST_ADDRESS: &str = "http://localhost:5555";
pub const DEV_ADDRESS: &str = "https://dev.xmtp.network:5555";

pub struct XmtpGrpcGatewayClient<'a> {
    url: String,
    http: reqwest::Client,
    phantom: PhantomData<&'a XmtpGrpcGatewaySubscription<'a>>,
}

impl<'a> XmtpGrpcGatewayClient<'a> {
    pub fn new(url: String) -> Self {
        XmtpGrpcGatewayClient {
            url,
            http: reqwest::Client::new(),
            phantom: PhantomData,
        }
    }
}

#[async_trait(?Send)]
impl<'a> XmtpApiClient for XmtpGrpcGatewayClient<'a> {
    type Subscription = XmtpGrpcGatewaySubscription<'a>;

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

    async fn subscribe(
        &self,
        request: SubscribeRequest,
    ) -> Result<XmtpGrpcGatewaySubscription<'a>, Error> {
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

        Ok(XmtpGrpcGatewaySubscription::start(response))
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

pub struct XmtpGrpcGatewaySubscription<'a> {
    // When this is `None`, the stream has been closed.
    pub stream: Option<LocalBoxStream<'a, Result<Envelope, Error>>>,
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

impl<'a> XmtpGrpcGatewaySubscription<'a> {
    pub fn start(req: LocalBoxStream<'a, Result<Bytes, reqwest::Error>>) -> Self {
        let bytes_reader = StreamReader::new(
            req.map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)),
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
            .boxed_local();

        XmtpGrpcGatewaySubscription {
            stream: Some(stream),
        }
    }
}

#[async_trait(?Send)]
impl<'a> XmtpApiSubscription for XmtpGrpcGatewaySubscription<'a> {
    fn is_closed(&self) -> bool {
        return self.stream.is_none();
    }

    // HACK: this consumes from the stream whatever is already ready.
    // TODO: implement a JS-friendly promise/future interface instead
    async fn get_messages(&mut self) -> Vec<Envelope> {
        if self.stream.is_none() {
            return vec![];
        }
        let stream = self.stream.as_mut().unwrap();
        let mut items: Vec<Envelope> = Vec::new();

        // TODO: consider using `size_hint` and fixing buffer size
        // let (lower, upper) = self.stream.unwrap().size_hint();
        // let capacity = clamp(lower, 10, 50);
        // items.reserve(capacity);
        // ... and on append: if items.len() >= capacity { return items; }

        // For now we rely on the subscriber to periodically call `get_messages`.
        // There is no hint to JS to tell it when to wake up and check for more.
        // So we use this no-op waker as context.
        // TODO: implement JS event or promise instead.
        let mut cx = Context::from_waker(noop_waker_ref());
        loop {
            match stream.as_mut().poll_next(&mut cx) {
                Poll::Pending => {
                    return items;
                }
                Poll::Ready(Some(item)) => {
                    if item.is_ok() {
                        items.push(item.unwrap());
                    }
                    // else item.is_err() and we discard it.
                }
                Poll::Ready(None) => {
                    self.stream = None;
                    return items;
                }
            }
        }
    }

    fn close_stream(&mut self) {
        self.stream = None;
    }
}
