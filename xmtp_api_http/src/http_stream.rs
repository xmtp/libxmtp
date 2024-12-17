//! Streams that work with HTTP POST requests

use crate::util::GrpcResponse;
use futures::{
    stream::{self, Stream, StreamExt},
    Future, FutureExt,
};
use reqwest::Response;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Deserializer;
use std::{marker::PhantomData, pin::Pin, task::Poll};
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
            NotStarted(ref mut f) => match f.poll_unpin(cx) {
                Ready(response) => {
                    let s = response.unwrap().bytes_stream();
                    self.set(Self::Started(Box::pin(s.boxed())));
                    self.poll_next(cx)
                }
                Pending => Pending,
            },
            Started(s) => s.poll_next_unpin(cx),
        }
    }
}

struct GrpcHttpStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    http: HttpPostStream<F>,
    remaining: Vec<u8>,
    _marker: PhantomData<R>,
}

impl<F, R> GrpcHttpStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>> + Unpin,
    R: DeserializeOwned + Send + std::fmt::Debug + Unpin + 'static,
{
    fn new(request: F) -> Self
    where
        F: Future<Output = Result<Response, reqwest::Error>>,
    {
        let mut http = HttpPostStream::NotStarted(request);
        // we need to poll the future once to establish the initial POST request
        // it will almost always be pending
        let _ = http.next().now_or_never();
        Self {
            http,
            remaining: vec![],
            _marker: PhantomData::<R>,
        }
    }
}

impl<F, R> Stream for GrpcHttpStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>> + Unpin,
    R: DeserializeOwned + Send + std::fmt::Debug + Unpin + 'static,
{
    type Item = Result<R, Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use futures::task::Poll::*;
        let this = self.get_mut();
        match this.http.poll_next_unpin(cx) {
            Ready(Some(bytes)) => {
                let bytes = bytes.map_err(|e| {
                    Error::new(ErrorKind::SubscriptionUpdateError).with(e.to_string())
                })?;
                let bytes = &[this.remaining.as_ref(), bytes.as_ref()].concat();
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
                                this.remaining = (&**bytes)[stream.byte_offset()..].to_vec();
                                tracing::info!("PENDING");
                                return Pending;
                            } else {
                                Err(Error::new(ErrorKind::MlsError).with(e.to_string()))
                            }
                        }
                        Some(Ok(GrpcResponse::Empty {})) => continue 'messages,
                        None => return Ready(None),
                    };
                    return Ready(Some(res));
                }
            }
            Ready(None) => Ready(None),
            Pending => {
                cx.waker().wake_by_ref();
                Pending
            }
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
    R: DeserializeOwned + Send + std::fmt::Debug + Unpin + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::BoxStream<'static, Result<R, Error>> {
    create_grpc_stream_inner(request, endpoint, http_client).boxed()
}

pub fn create_grpc_stream_inner<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + Unpin + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> impl Stream<Item = Result<R, Error>> {
    let request = http_client.post(endpoint).json(&request).send();
    GrpcHttpStream::new(request)
}
