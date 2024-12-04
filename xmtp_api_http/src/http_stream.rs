//! Streams that work with HTTP POST requests

use crate::util::GrpcResponse;
use futures::{
    stream::{self, Stream, StreamExt},
    Future,
};
use reqwest::Response;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Deserializer;
use std::pin::Pin;
use xmtp_proto::{Error, ErrorKind};

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct SubscriptionItem<T> {
    pub result: T,
}

enum HttpPostStream<F>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    NotStarted(F),
    // NotStarted(Box<dyn Future<Output = Result<Response, Error>>>),
    Started(Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin + Send>>),
}

impl<F> Stream for HttpPostStream<F>
where
    F: Future<Output = Result<Response, reqwest::Error>> + Unpin,
{
    type Item = Result<bytes::Bytes, reqwest::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use futures::task::Poll::*;
        use HttpPostStream::*;
        match self.as_mut().get_mut() {
            NotStarted(ref mut f) => {
                tracing::info!("Polling");
                let f = std::pin::pin!(f);
                match f.poll(cx) {
                    Ready(response) => {
                        let s = response.unwrap().bytes_stream();
                        self.set(Self::Started(Box::pin(s.boxed())));
                        self.poll_next(cx)
                    }
                    Pending => {
                        // cx.waker().wake_by_ref();
                        Pending
                    }
                }
            }
            Started(s) => s.poll_next_unpin(cx),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn create_grpc_stream<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::LocalBoxStream<'static, Result<R, Error>> {
    create_grpc_stream_inner(request, endpoint, http_client).boxed_local()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn create_grpc_stream<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::BoxStream<'static, Result<R, Error>> {
    create_grpc_stream_inner(request, endpoint, http_client).boxed()
}

pub fn create_grpc_stream_inner<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> impl Stream<Item = Result<R, Error>> {
    let request = http_client.post(endpoint).json(&request).send();
    let http_stream = HttpPostStream::NotStarted(request);

    async_stream::stream! {
        tracing::info!("spawning grpc http stream");
        let mut remaining = vec![];
        for await bytes in http_stream {
            let bytes = bytes
                .map_err(|e| Error::new(ErrorKind::SubscriptionUpdateError).with(e.to_string()))?;
            let bytes = &[remaining.as_ref(), bytes.as_ref()].concat();
            let de = Deserializer::from_slice(bytes);
            let mut stream = de.into_iter::<GrpcResponse<R>>();
            'messages: loop {
                tracing::debug!("Waiting on next response ...");
                let response = stream.next();
                let res = match response {
                    Some(Ok(GrpcResponse::Ok(response))) => Ok(response),
                    Some(Ok(GrpcResponse::SubscriptionItem(item))) => Ok(item.result),
                    Some(Ok(GrpcResponse::Err(e))) => {
                        Err(Error::new(ErrorKind::MlsError).with(e.message))
                    }
                    Some(Err(e)) => {
                        if e.is_eof() {
                            remaining = (&**bytes)[stream.byte_offset()..].to_vec();
                            break 'messages;
                        } else {
                            Err(Error::new(ErrorKind::MlsError).with(e.to_string()))
                        }
                    }
                    Some(Ok(GrpcResponse::Empty {})) => continue 'messages,
                    None => break 'messages,
                };
                yield res;
            }
        }
    }
}
