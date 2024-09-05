use futures::{stream, Stream, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Deserializer;
use std::io::Read;
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
    log::info!("About to spawn stream");
    let stream = create_grpc_stream_inner(request, endpoint, http_client);
    stream.boxed_local()
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
    log::info!("About to spawn stream");
    let stream = create_grpc_stream_inner(request, endpoint, http_client);
    stream.boxed()
}

fn create_grpc_stream_inner<
    T: Serialize + Send + 'static,
    R: DeserializeOwned + Send + std::fmt::Debug + 'static,
>(
    request: T,
    endpoint: String,
    http_client: reqwest::Client,
) -> impl Stream<Item = Result<R, Error>> {
    async_stream::stream! {
        log::info!("Spawning grpc http stream");
        let request = http_client
                .post(endpoint)
                .json(&request)
                .send()
                .await
                .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        let mut remaining = vec![];
        for await bytes in request.bytes_stream() {
            let bytes = bytes
                .map_err(|e| Error::new(ErrorKind::SubscriptionUpdateError).with(e.to_string()))?;
            let bytes = &[remaining.as_ref(), bytes.as_ref()].concat();
            let de = Deserializer::from_slice(bytes);
            let mut stream = de.into_iter::<GrpcResponse<R>>();
            'messages: loop {
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
