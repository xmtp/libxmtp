//! Streams that work with HTTP POST requests

use crate::util::GrpcResponse;
use futures::{
    stream::{self, Stream, StreamExt},
    Future,
};
use pin_project_lite::pin_project;
use reqwest::Response;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Deserializer;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use xmtp_common::{StreamWrapper, Fairness};
use xmtp_proto::{Error, ErrorKind};

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct SubscriptionItem<T> {
    pub result: T,
}

pin_project! {
    /// The establish future for the http post stream
    struct HttpStreamEstablish<'a, F> {
        #[pin] inner: F,
        _marker: PhantomData<&'a F>
    }
}

impl<'a, F> HttpStreamEstablish<'a, F> {
    fn new(inner: F) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<'a, F> Future for HttpStreamEstablish<'a, F>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    type Output = Result<StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use Poll::*;
        let this = self.as_mut().project();
        match this.inner.poll(cx) {
            Ready(response) => {
                tracing::info!("ESTABLISH READY");
                let stream = response
                    .inspect_err(|e| {
                        tracing::error!(
                            "Error during http subscription with grpc http gateway {e}"
                        );
                    })
                    .map_err(|_| Error::new(ErrorKind::SubscribeError))?;
                tracing::info!("Calling bytes stream!");
                Ready(Ok(StreamWrapper::new(stream.bytes_stream())))
            }
            Pending => {
                Fairness::wake();
                Pending
            }
        }
    }
}

pin_project! {
    struct HttpPostStream<'a, R> {
        #[pin] http: StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>,
        remaining: Vec<u8>,
        _marker: PhantomData<&'a R>,
    }
}

impl<'a, R> Stream for HttpPostStream<'a, R>
where
    for<'de> R: Send + Deserialize<'de>,
{
    type Item = Result<R, Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use Poll::*;
        let this = self.project();
        match this.http.poll_next(cx) {
            Ready(Some(bytes)) => {
                tracing::info!("READY THIS IS WHAT WE WANT");
                let bytes = bytes
                    .inspect_err(|e| tracing::error!("Error in http stream to grpc gateway {e}"))
                    .map_err(|_| Error::new(ErrorKind::SubscribeError))?;
                match Self::on_bytes(bytes, this.remaining)? {
                    None => {
                        tracing::info!("ON BYTES NONE PENDING");
                        xmtp_common::Fairness::wake();
                        Pending
                    },
                    Some(r) => {
                        tracing::info!("READY");
                        Ready(Some(Ok(r)))
                    },
                }
            }
            Ready(None) => Ready(None),
            Pending => {
                xmtp_common::Fairness::wake();
                Pending
            },
        }
    }
}

impl<'a, R> HttpPostStream<'a, R>
where
    R: Send + 'static,
{
    pub fn new(establish: StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>) -> Self {
        tracing::info!("New post stream");
        Self {
            http: establish,
            remaining: Vec::new(),
            _marker: PhantomData,
        }
    }
}

impl<R> HttpPostStream<'_, R>
where
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send,
{
    fn on_bytes(bytes: bytes::Bytes, remaining: &mut Vec<u8>) -> Result<Option<R>, Error> {
        let bytes = &[remaining.as_ref(), bytes.as_ref()].concat();
        let de = Deserializer::from_slice(bytes);
        let mut deser_stream = de.into_iter::<GrpcResponse<R>>();
        loop {
            let item = deser_stream.next();
            match item {
                Some(Ok(GrpcResponse::Ok(response))) => return Ok(Some(response)),
                Some(Ok(GrpcResponse::SubscriptionItem(item))) => return Ok(Some(item.result)),
                Some(Ok(GrpcResponse::Err(e))) => {
                    return Err(Error::new(ErrorKind::MlsError).with(e.message));
                }
                Some(Err(e)) => {
                    if e.is_eof() {
                        *remaining = (&**bytes)[deser_stream.byte_offset()..].to_vec();
                        tracing::debug!("IS EOF");
                        return Ok(None);
                    } else {
                        return Err(Error::new(ErrorKind::MlsError).with(e.to_string()));
                    }
                }
                Some(Ok(GrpcResponse::Empty {})) => continue,
                None => {
                    tracing::debug!("IS NONE");
                    return Ok(None)
                },
            }
        }
    }
}

pin_project! {
    /// The establish future for the http post stream
    #[project = ProjectHttpStream]
    enum HttpStream<'a, F, R> {
        NotStarted {
            #[pin] future: HttpStreamEstablish<'a, F>,
        },
        Started {
            #[pin] stream: HttpPostStream<'a, R>,
        }
    }
}

impl<'a, F, R> HttpStream<'a, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    fn new(request: F) -> Self {
        Self::NotStarted{ future: HttpStreamEstablish::new(request) }
    }
}

impl<'a, F, R> Stream for HttpStream<'a, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Send + Deserialize<'de> + 'static,
{
    type Item = Result<R, Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use ProjectHttpStream::*;
        use Poll::*;
        let this = self.as_mut().project();
        match this {
            NotStarted { future } => match future.poll(cx) {
                Ready(stream) => {
                    tracing::info!("READY TOP LEVEL");
                    self.set(Self::Started { stream: HttpPostStream::new(stream?) } );
                    // cx.waker().wake_by_ref();
                    // tracing::info!("POLLING STREAM NEXT");
                    self.poll_next(cx)
                },
                Pending => {
                    Fairness::wake();
                    cx.waker().wake_by_ref();
                    Pending
                }
            },
            Started { stream } => {
                Fairness::wake();
                let p = stream.poll_next(cx);
                if let Pending = p {
                    Fairness::wake();
                    cx.waker().wake_by_ref();
                }
                p
            }
        }
    }
}

impl<'a, F, R> std::fmt::Debug for HttpStream<'a, F, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::NotStarted{..} => write!(f, "not started"),
            Self::Started{..} => write!(f, "started"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, F, R> HttpStream<'a, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>> + Unpin,
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send + 'static,
{
    /// Establish the initial HTTP Stream connection
    async fn establish(&mut self) {
        // we need to poll the future once to progress the future state &
        // establish the initial POST request.
        // It should always be pending
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        // let mut this = Pin::new(self);
        let mut this = Pin::new(self);
        if let Poll::Ready(_) = this.poll_next_unpin(&mut cx) {
            tracing::error!("Stream ready before established");
            unreachable!()
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, F, R> HttpStream<'a, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send + 'static,
{
    async fn establish(&mut self) {
        tracing::info!("Establishing...");
        // we need to poll the future once to progress the future state &
        // establish the initial POST request.
        // It should always be pending
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        let mut this = unsafe { Pin::new_unchecked(self) };
        if let Poll::Ready(_) = this.as_mut().poll_next(&mut cx) {
            tracing::info!("stream ready before established...");
            unreachable!()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn create_grpc_stream<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> Result<stream::LocalBoxStream<'static, Result<R, Error>>, Error> {
    Ok(create_grpc_stream_inner(request, endpoint, http_client)
        .await?
        .boxed_local())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn create_grpc_stream<T, R>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> Result<stream::BoxStream<'static, Result<R, Error>>, Error>
where
    T: Serialize + 'static,
    R: DeserializeOwned + Send + Sync + 'static,
{
    Ok(create_grpc_stream_inner(request, endpoint, http_client)
        .await?
        .boxed())
}

#[tracing::instrument(skip_all)]
pub async fn create_grpc_stream_inner<T, R>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> Result<impl Stream<Item = Result<R, Error>>, Error>
where
    T: Serialize + 'static,
    R: DeserializeOwned + Send + 'static,
{
    tracing::info!("CREATING STREAM");
    let request = http_client.post(endpoint).json(&request).send();
    let mut http = HttpStream::new(request);
    http.establish().await;
    Ok(http)
}
