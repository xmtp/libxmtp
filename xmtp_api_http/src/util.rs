use crate::http_stream::SubscriptionItem;
use crate::Error;
use crate::ErrorResponse;
use crate::HttpClientError;
use prost::Message;
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
pub async fn handle_error_proto<T>(response: reqwest::Response) -> Result<T, Error>
where
    T: prost::Message + Default,
{
    if response.status().is_success() {
        let res = response.bytes().await.map_err(HttpClientError::from)?;
        return Ok(Message::decode(res).map_err(HttpClientError::from)?);
    }

    Err(HttpClientError::Grpc(ErrorResponse {
        code: response.status().as_u16() as usize,
        message: response
            .text_with_charset("utf-8")
            .await
            .map_err(HttpClientError::from)?,
        details: vec![],
    })
    .into())
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
#[allow(clippy::unwrap_used)]
impl xmtp_proto::api_client::XmtpTestClient for crate::XmtpHttpApiClient {
    async fn create_local() -> Self {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        api.set_host(crate::constants::ApiUrls::LOCAL_ADDRESS.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api.build().await.unwrap()
    }

    async fn create_dev() -> Self {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        api.set_host(crate::constants::ApiUrls::DEV_ADDRESS.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api.build().await.unwrap()
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
