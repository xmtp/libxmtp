use bytes::Bytes;
use futures::{
    stream::{self, StreamExt},
    Stream,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Deserializer;
use std::{collections::VecDeque, io::Read, ops::DerefMut, pin::Pin, task::Poll};
use xmtp_proto::api_client::{Error, ErrorKind};

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub(crate) enum GrpcResponse<T> {
    Ok(T),
    Err(ErrorResponse),
    SubscriptionItem(SubscriptionItem<T>),
    Empty {},
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct ErrorResponse {
    code: usize,
    pub message: String,
    details: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct SubscriptionItem<T> {
    pub result: T,
}

/// handle JSON response from gRPC, returning either
/// the expected deserialized response object or a gRPC [`Error`]
pub fn handle_error<R: Read, T>(reader: R) -> Result<T, Error>
where
    T: DeserializeOwned + Default,
{
    match serde_json::from_reader(reader) {
        Ok(GrpcResponse::Ok(response)) => Ok(response),
        Ok(GrpcResponse::Err(e)) => Err(Error::new(ErrorKind::IdentityError).with(e.message)),
        Ok(GrpcResponse::Empty {}) => Ok(Default::default()),
        Ok(GrpcResponse::SubscriptionItem(item)) => Ok(item.result),
        Err(e) => Err(Error::new(ErrorKind::QueryError).with(e.to_string())),
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
pub async fn create_grpc_stream<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static + Unpin,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> stream::BoxStream<'static, Result<R, Error>> {
    create_grpc_stream_inner(request, endpoint, http_client)
        .await
        .boxed()
}

struct MessageStream<R>
where
    R: DeserializeOwned + Send + std::fmt::Debug + 'static + Unpin,
{
    bytes_stream: stream::BoxStream<'static, Result<Bytes, reqwest::Error>>,
    is_bytes_stream_empty: bool,
    bytes: Vec<u8>,
    messages: VecDeque<Result<R, Error>>,
}

impl<R> futures::Stream for MessageStream<R>
where
    R: DeserializeOwned + Send + std::fmt::Debug + 'static + Unpin,
{
    type Item = Result<R, Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let ms = Pin::deref_mut(&mut self);

        // Poll the network stream
        while let Poll::Ready(response) = ms.bytes_stream.poll_next_unpin(cx) {
            if let Some(Ok(bytes)) = response {
                ms.bytes.extend_from_slice(&bytes);
                continue;
            } else if response.is_none() {
                ms.is_bytes_stream_empty = true;
            }
        }

        // Deserialize messages from the bytes we've collected
        let de = Deserializer::from_slice(&ms.bytes);
        let mut stream = de.into_iter::<GrpcResponse<R>>();
        while let Some(message) = stream.next() {
            let message = match message {
                Ok(GrpcResponse::Ok(r)) => Ok(r),
                Ok(GrpcResponse::SubscriptionItem(item)) => Ok(item.result),
                Ok(GrpcResponse::Err(e)) => Err(Error::new(ErrorKind::MlsError).with(e.message)),
                Err(e) if e.is_eof() => {
                    ms.bytes.drain(..stream.byte_offset());
                    break;
                }
                Ok(GrpcResponse::Empty {}) => continue,
                Err(e) => Err(Error::new(ErrorKind::MlsError).with(e.to_string())),
            };
            ms.messages.push_back(message);
        }

        if let Some(message) = ms.messages.pop_front() {
            Poll::Ready(Some(message))
        } else if ms.is_bytes_stream_empty {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

pub async fn create_grpc_stream_inner<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static + Unpin,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> impl Stream<Item = Result<R, Error>> {
    tracing::info!("Spawning grpc http stream");
    let response = http_client
        .post(endpoint)
        .json(&request)
        .send()
        .await
        .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
        .unwrap();
    let bytes_stream = response.bytes_stream().boxed();

    MessageStream {
        bytes_stream,
        is_bytes_stream_empty: false,
        bytes: Vec::new(),
        messages: VecDeque::new(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_error_handler_on_unit_value() {
        handle_error::<_, ()>(b"{}".as_slice()).unwrap();
    }
}
