//! Streams that work with HTTP POST requests

use crate::{util::GrpcResponse, HttpClientError};
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
    task::{ready, Context, Poll},
};
use xmtp_common::StreamWrapper;
use xmtp_proto::traits::ApiClientError;

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

impl<F> HttpStreamEstablish<'_, F> {
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
    type Output = Result<StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>, HttpClientError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use Poll::*;
        let this = self.as_mut().project();
        let response = ready!(this.inner.poll(cx));
        let stream = response.inspect_err(|e| {
            tracing::error!("Error during http subscription with grpc http gateway {e}");
        })?;
        Ready(Ok(StreamWrapper::new(stream.bytes_stream())))
    }
}

pin_project! {
    struct HttpPostStream<'a, R> {
        #[pin] http: StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>,
        remaining: Vec<u8>,
        _marker: PhantomData<&'a R>,
    }
}

impl<R> Stream for HttpPostStream<'_, R>
where
    for<'de> R: Send + Deserialize<'de>,
{
    type Item = Result<R, ApiClientError<HttpClientError>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use Poll::*;
        let mut this = self.as_mut().project();
        let item = ready!(this.http.as_mut().poll_next(cx));
        match item {
            Some(bytes) => {
                let bytes = bytes.map_err(HttpClientError::from)?;
                let mut items = Self::on_bytes(bytes, this.remaining)?;
                let item = items.pop();
                if let Some(item) = item {
                    Ready(Some(Ok(item)))
                } else {
                    self.poll_next(cx)
                }
            }
            None => Ready(None),
        }
    }
}

impl<'a, R> HttpPostStream<'a, R>
where
    R: Send + 'static,
{
    pub fn new(establish: StreamWrapper<'a, Result<bytes::Bytes, reqwest::Error>>) -> Self {
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
    fn on_bytes(bytes: bytes::Bytes, remaining: &mut Vec<u8>) -> Result<Vec<R>, HttpClientError> {
        let bytes = &[remaining.as_ref(), bytes.as_ref()].concat();
        remaining.clear();
        let de = Deserializer::from_slice(bytes);
        let mut deser_stream = de.into_iter::<GrpcResponse<R>>();
        let mut items = Vec::new();
        while let Some(item) = deser_stream.next() {
            match item {
                Ok(GrpcResponse::Ok(response)) => items.push(response),
                Ok(GrpcResponse::SubscriptionItem(item)) => items.push(item.result),
                Ok(GrpcResponse::Err(e)) => {
                    return Err(HttpClientError::Grpc(e));
                }
                Err(e) => {
                    if e.is_eof() {
                        *remaining = bytes[deser_stream.byte_offset()..].to_vec();
                    } else {
                        return Err(HttpClientError::from(e));
                    }
                }
                Ok(GrpcResponse::Empty {}) => continue,
            }
        }

        if items.len() > 1 {
            tracing::warn!("more than one item deserialized from http stream");
        }
        Ok(items)
    }
}

pin_project! {
    struct HttpStream<'a, F, R> {
        #[pin] state: HttpStreamState<'a, F, R>,
        id: String
    }
}

pin_project! {
    /// The establish future for the http post stream
    #[project = ProjectHttpStream]
    enum HttpStreamState<'a, F, R> {
        NotStarted {
            #[pin] future: HttpStreamEstablish<'a, F>,
        },
        Started {
            #[pin] stream: HttpPostStream<'a, R>,
        }
    }
}

impl<F, R> HttpStream<'_, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
{
    fn new(request: F) -> Self {
        let id = xmtp_common::rand_string::<12>();
        tracing::info!("new http stream id={}", &id);
        Self {
            state: HttpStreamState::NotStarted {
                future: HttpStreamEstablish::new(request),
            },
            id,
        }
    }
}

impl<F, R> Stream for HttpStream<'_, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Send + Deserialize<'de> + 'static,
{
    type Item = Result<R, ApiClientError<HttpClientError>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use ProjectHttpStream::*;
        let mut this = self.as_mut().project();
        match this.state.as_mut().project() {
            NotStarted { future } => {
                let stream = ready!(future.poll(cx))?;
                this.state.set(HttpStreamState::Started {
                    stream: HttpPostStream::new(stream),
                });
                tracing::debug!("Stream {} ready, polling for the first time...", &self.id);
                self.poll_next(cx)
            }
            Started { mut stream } => {
                let item = ready!(stream.as_mut().poll_next(cx));
                tracing::trace!("stream id={} ready with item", &self.id);
                Poll::Ready(item)
            }
        }
    }
}

impl<F, R> std::fmt::Debug for HttpStream<'_, F, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.state {
            HttpStreamState::NotStarted { .. } => write!(f, "not started"),
            HttpStreamState::Started { .. } => write!(f, "started"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F, R> HttpStream<'_, F, R>
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
        let mut this = Pin::new(self);
        if this.poll_next_unpin(&mut cx).is_ready() {
            tracing::error!("Stream ready before established");
            unreachable!()
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, R> HttpStream<'_, F, R>
where
    F: Future<Output = Result<Response, reqwest::Error>>,
    for<'de> R: Deserialize<'de> + DeserializeOwned + Send + 'static,
{
    async fn establish(&mut self) {
        tracing::debug!("establishing new http stream {}...", self.id);
        // we need to poll the future once to progress the future state &
        // establish the initial POST request.
        // It should always be pending
        let noop_waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&noop_waker);
        let mut this = unsafe { Pin::new_unchecked(self) };
        if this.as_mut().poll_next(&mut cx).is_ready() {
            tracing::error!("stream ready before established...");
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
) -> Result<
    stream::LocalBoxStream<'static, Result<R, ApiClientError<HttpClientError>>>,
    ApiClientError<HttpClientError>,
> {
    Ok(create_grpc_stream_inner(request, endpoint, http_client)
        .await?
        .boxed_local())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn create_grpc_stream<T, R>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> Result<
    stream::BoxStream<'static, Result<R, ApiClientError<HttpClientError>>>,
    ApiClientError<HttpClientError>,
>
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
) -> Result<
    impl Stream<Item = Result<R, ApiClientError<HttpClientError>>>,
    ApiClientError<HttpClientError>,
>
where
    T: Serialize + 'static,
    R: DeserializeOwned + Send + 'static,
{
    tracing::debug!("JSON REQUEST: {:?}", serde_json::to_string_pretty(&request));
    let request = http_client.post(endpoint).json(&request).send();
    let mut http = HttpStream::new(request);
    http.establish().await;
    Ok(http)
}

#[cfg(test)]
mod tests {
    use crate::{constants::ApiUrls, XmtpHttpApiClient};
    use futures::StreamExt;
    use xmtp_proto::{
        api_client::ApiBuilder, mls_v1::subscribe_group_messages_request::Filter,
        prelude::XmtpMlsStreams,
    };

    #[xmtp_common::test]
    async fn test_bytes_stream() {
        let json = serde_json::json!({
            "filters": [
                {
                    "groupId": "9h4VW3x4hs6MJekAbHbguA==",
                    "idCursor": "1"
                }
            ]
        });
        let mut client = XmtpHttpApiClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        let client = client.build().await.unwrap();
        let http = client.http_client;
        let mut stream = http
            .post(format!(
                "{}/mls/v1/subscribe-group-messages",
                ApiUrls::LOCAL_ADDRESS.to_string()
            ))
            .json(&json)
            .send()
            .await
            .unwrap()
            .bytes_stream();
        let mut items = 0;
        while let Some(item) = stream.next().await {
            items += 1;
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&item.unwrap()) {
                // println!("{}", serde_json::to_string_pretty(&val).unwrap());
            }
        }
    }

    #[xmtp_common::test]
    async fn test_bytes_stream_custom() {
        let json = serde_json::json!({
            "filters": [
                {
                    "groupId": "9h4VW3x4hs6MJekAbHbguA==",
                    "idCursor": "1"
                }
            ]
        });
        let mut client = XmtpHttpApiClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_libxmtp_version(env!("CARGO_PKG_VERSION").into());
        client.set_host("http://localhost:5555".into());
        let client = client.build().await.unwrap();
        let mut stream = client
            .subscribe_group_messages(xmtp_proto::mls_v1::SubscribeGroupMessagesRequest {
                filters: vec![Filter {
                    group_id: hex::decode("f61e155b7c7886ce8c25e9006c76e0b8").unwrap(),
                    id_cursor: 1,
                }],
            })
            .await
            .unwrap();
        let mut items = 0;
        while let Some(item) = stream.next().await {
            items += 1;
            // if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&item.unwrap()) {
            // println!("{}", serde_json::to_string_pretty(&val).unwrap());
            // }
            println!("got {} items", items);
        }
    }
}
