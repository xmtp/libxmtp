//! Streams that work with HTTP POST requests

use crate::util::GrpcResponse;
use futures::{
    stream::{self, Stream, StreamExt},
    Future,
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

#[cfg(target_arch = "wasm32")]
pub type BytesStream = stream::LocalBoxStream<'static, Result<bytes::Bytes, reqwest::Error>>;

// #[cfg(not(target_arch = "wasm32"))]
// pub type BytesStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>;

#[cfg(not(target_arch = "wasm32"))]
pub type BytesStream = stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>;

pin_project_lite::pin_project! {
    #[project = PostStreamProject]
    enum HttpPostStream<F, R> {
        NotStarted{#[pin] fut: F},
        // `Reqwest::bytes_stream` returns `impl Stream` rather than a type generic,
        // so we can't use a type generic here
        // this makes wasm a bit tricky.
        Started {
            #[pin] http: BytesStream,
            remaining: Vec<u8>,
            _marker: PhantomData<R>,
        },
    }
}

impl<F, R> Stream for HttpPostStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Send + Deserialize<'de>,
{
    type Item = Result<R, Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use futures::task::Poll::*;
        match self.as_mut().project() {
            PostStreamProject::NotStarted { fut } => match fut.poll(cx) {
                Ready(response) => {
                    let s = response.unwrap().bytes_stream();
                    self.set(Self::started(s));
                    self.as_mut().poll_next(cx)
                }
                Pending => {
                    cx.waker().wake_by_ref();
                    Pending
                }
            },
            PostStreamProject::Started {
                ref mut http,
                ref mut remaining,
                ..
            } => {
                let mut pinned = std::pin::pin!(http);
                let next = pinned.as_mut().poll_next(cx);
                Self::on_bytes(next, remaining, cx)
            }
        }
    }
}

impl<F, R> HttpPostStream<F, R>
where
    R: Send,
{
    #[cfg(not(target_arch = "wasm32"))]
    fn started(
        http: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    ) -> Self {
        Self::Started {
            http: http.boxed(),
            remaining: Vec::new(),
            _marker: PhantomData,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn started(http: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + 'static) -> Self {
        Self::Started {
            http: http.boxed_local(),
            remaining: Vec::new(),
            _marker: PhantomData,
        }
    }
}

impl<F, R> HttpPostStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send,
{
    fn new(request: F) -> Self {
        Self::NotStarted { fut: request }
    }

    fn on_bytes(
        p: Poll<Option<Result<bytes::Bytes, reqwest::Error>>>,
        remaining: &mut Vec<u8>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use futures::task::Poll::*;
        match p {
            Ready(Some(bytes)) => {
                let bytes = bytes.map_err(|e| {
                    Error::new(ErrorKind::SubscriptionUpdateError).with(e.to_string())
                })?;
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
                                *remaining = (&**bytes)[stream.byte_offset()..].to_vec();
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
    /*
    fn on_request(
        self: &mut Pin<&mut Self>,
        p: Poll<Result<reqwest::Response, reqwest::Error>>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use futures::task::Poll::*;
        match p {
            Ready(response) => {
                let s = response.unwrap().bytes_stream();
                self.set(Self::started(s));
                self.as_mut().poll_next(cx)
            }
            Pending => Pending,
        }
    }
    */
}

impl<F, R> HttpPostStream<F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>> + Unpin,
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send,
{
    /// Establish the initial HTTP Stream connection
    fn establish(&mut self) -> () {
        // we need to poll the future once to progress the future state &
        // establish the initial POST request.
        // It should always be pending
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        // let mut this = Pin::new(self);
        let mut this = Pin::new(self);
        let _ = this.poll_next_unpin(&mut cx);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn create_grpc_stream<T: Serialize + Send + 'static, R: DeserializeOwned + Send + 'static>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::LocalBoxStream<'static, Result<R, Error>> {
    create_grpc_stream_inner(request, endpoint, http_client).boxed_local()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn create_grpc_stream<T, R>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::BoxStream<'static, Result<R, Error>>
where
    T: Serialize + 'static,
    R: DeserializeOwned + Send + 'static,
{
    create_grpc_stream_inner(request, endpoint, http_client).boxed()
}

fn create_grpc_stream_inner<T, R>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> impl Stream<Item = Result<R, Error>>
where
    T: Serialize + 'static,
    R: DeserializeOwned + Send + 'static,
{
    let request = http_client.post(endpoint).json(&request).send();
    let mut http = HttpPostStream::new(request);
    http.establish();
    http
}
