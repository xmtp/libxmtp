use crate::http_stream::SubscriptionItem;
use crate::Error;
use crate::ErrorResponse;
use crate::HttpClientError;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::Read;

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub(crate) enum GrpcResponse<T> {
    Ok(T),
    Err(ErrorResponse),
    SubscriptionItem(SubscriptionItem<T>),
    Empty {},
}

/// handle JSON response from gRPC, returning either
/// the expected deserialized response object or a gRPC [`Error`]
pub fn handle_error<R: Read, T>(reader: R) -> Result<T, Error>
where
    T: DeserializeOwned + Default,
{
    match serde_json::from_reader(reader) {
        Ok(GrpcResponse::Ok(response)) => Ok(response),
        Ok(GrpcResponse::Err(e)) => Err(Error::new(HttpClientError::from(e))),
        Ok(GrpcResponse::Empty {}) => Ok(Default::default()),
        Ok(GrpcResponse::SubscriptionItem(item)) => Ok(item.result),
        Err(e) => Err(Error::new(e)),
    }
}

#[cfg(feature = "test-utils")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl xmtp_proto::api_client::XmtpTestClient for crate::XmtpHttpApiClient {
    async fn create_local() -> Self {
        crate::XmtpHttpApiClient::new("http://localhost:5555".into())
            .expect("could not create client")
    }

    async fn create_dev() -> Self {
        crate::XmtpHttpApiClient::new("https://grpc.dev.xmtp.network:443".into())
            .expect("coult not create client")
    }
}

#[cfg(test)]
pub mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_error_handler_on_unit_value() {
        handle_error::<_, ()>(b"{}".as_slice()).unwrap();
    }
}
