use crate::http_stream::SubscriptionItem;
use crate::ErrorResponse;
use crate::HttpClientError;
use prost::Message;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub(crate) enum GrpcResponse<T> {
    SubscriptionItem(SubscriptionItem<T>),
    Ok(T),
    Err(ErrorResponse),
    Empty {},
}

/// handle JSON response from gRPC, returning either
/// the expected deserialized response object or a gRPC [`Error`]
pub async fn handle_error_proto<T>(response: reqwest::Response) -> Result<T, HttpClientError>
where
    T: prost::Message + Default,
{
    if response.status().is_success() {
        let res = response.bytes().await.map_err(HttpClientError::from)?;
        return Message::decode(res).map_err(HttpClientError::from);
    }

    Err(HttpClientError::Grpc(ErrorResponse {
        code: response.status().as_u16() as usize,
        message: response.text().await.map_err(HttpClientError::from)?,
        details: vec![],
    }))
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::unwrap_used)]
impl xmtp_proto::api_client::XmtpTestClient for crate::XmtpHttpApiClient {
    type Builder = crate::XmtpHttpApiClientBuilder;
    fn local_port() -> &'static str {
        "5055"
    }

    fn create_local_d14n() -> Self::Builder {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        api.set_host(crate::constants::ApiUrls::LOCAL_D14N_ADDRESS.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api
    }

    fn create_local_payer() -> Self::Builder {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        // payer has same address as d14n locally
        api.set_host(crate::constants::ApiUrls::LOCAL_D14N_ADDRESS.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api
    }

    fn create_custom(addr: &str) -> Self::Builder {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        api.set_host(addr.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api
    }

    fn create_local() -> Self::Builder {
        Self::create_custom(crate::constants::ApiUrls::LOCAL_ADDRESS)
    }

    fn create_dev() -> Self::Builder {
        use xmtp_proto::api_client::ApiBuilder;
        let mut api = crate::XmtpHttpApiClient::builder();
        api.set_host(crate::constants::ApiUrls::DEV_ADDRESS.into());
        api.set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        api.set_app_version("0.0.0".into()).unwrap();
        api
    }
}

#[cfg(test)]
pub mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
}
