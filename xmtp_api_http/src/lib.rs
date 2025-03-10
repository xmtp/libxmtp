#![warn(clippy::unwrap_used)]

pub mod constants;
pub mod error;
pub mod http_client;
pub mod http_stream;

pub mod util;

use futures::stream;
use http_stream::create_grpc_stream;
use prost::Message;
use reqwest::header;
use reqwest::header::HeaderMap;
use util::handle_error_proto;

use xmtp_proto::api_client::{ApiBuilder, ApiStats, IdentityStats, XmtpIdentityClient};
use xmtp_proto::traits::ApiClientError;
use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
    GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
    GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::mls::api::v1::{GroupMessage, WelcomeMessage};
use xmtp_proto::{
    api_client::{XmtpMlsClient, XmtpMlsStreams},
    xmtp::mls::api::v1::{
        FetchKeyPackagesRequest, FetchKeyPackagesResponse, QueryGroupMessagesRequest,
        QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
        SendGroupMessagesRequest, SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
        SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    },
    ApiEndpoint,
};

#[macro_use]
extern crate tracing;

use crate::constants::ApiEndpoints;
pub use crate::error::{ErrorResponse, HttpClientError};
pub const LOCALHOST_ADDRESS: &str = "http://localhost:5555";

#[cfg(target_arch = "wasm32")]
fn reqwest_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
}

#[cfg(not(target_arch = "wasm32"))]
fn reqwest_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().connection_verbose(true)
}

#[derive(Debug, Clone)]
pub struct XmtpHttpApiClient {
    http_client: reqwest::Client,
    host_url: String,
    app_version: String,
    libxmtp_version: String,
    stats: ApiStats,
    identity_stats: IdentityStats,
}

impl XmtpHttpApiClient {
    pub fn new(host_url: String, app_version: String) -> Result<Self, HttpClientError> {
        let client = reqwest_builder();
        let libxmtp_version = env!("CARGO_PKG_VERSION").to_string();
        let mut headers = header::HeaderMap::new();
        headers.insert("x-libxmtp-version", libxmtp_version.parse()?);
        let client = client.default_headers(headers).build()?;

        Ok(XmtpHttpApiClient {
            http_client: client,
            host_url,
            app_version,
            libxmtp_version,
            stats: ApiStats::default(),
            identity_stats: IdentityStats::default(),
        })
    }

    pub fn builder() -> XmtpHttpApiClientBuilder {
        Default::default()
    }

    fn endpoint(&self, endpoint: &str) -> String {
        format!("{}{}", self.host_url, endpoint)
    }

    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    pub fn libxmtp_version(&self) -> &str {
        &self.libxmtp_version
    }
}

#[derive(Debug)]
pub struct XmtpHttpApiClientBuilder {
    host_url: String,
    app_version: Option<String>,
    headers: header::HeaderMap,
    libxmtp_version: Option<String>,
    reqwest: reqwest::ClientBuilder,
}

impl Default for XmtpHttpApiClientBuilder {
    fn default() -> Self {
        Self {
            host_url: "".to_string(),
            app_version: None,
            libxmtp_version: None,
            headers: header::HeaderMap::new(),
            reqwest: reqwest_builder(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HttpClientBuilderError {
    #[error("missing core libxmtp version")]
    MissingLibxmtpVersion,
    #[error("missing app version")]
    MissingAppVersion,
    #[error(transparent)]
    ReqwestErrror(#[from] reqwest::Error),
    #[error(transparent)]
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    InvalidUriParts(#[from] http::uri::InvalidUriParts),
}

impl ApiBuilder for XmtpHttpApiClientBuilder {
    type Output = XmtpHttpApiClient;
    type Error = HttpClientBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.libxmtp_version = Some(version.clone());
        self.headers.insert("x-libxmtp-version", version.parse()?);
        Ok(())
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.app_version = Some(version.clone());
        self.headers.insert("x-app-version", version.parse()?);
        Ok(())
    }

    fn set_host(&mut self, host: String) {
        self.host_url = host;
    }

    // no op for http so far
    fn set_tls(&mut self, _tls: bool) {
        self.reqwest = reqwest_builder();
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let http_client = self.reqwest.default_headers(self.headers).build()?;
        let libxmtp_version = self
            .libxmtp_version
            .unwrap_or(env!("CARGO_PKG_VERSION").to_string());
        let app_version = self
            .app_version
            .ok_or(HttpClientBuilderError::MissingAppVersion)?;

        Ok(XmtpHttpApiClient {
            http_client,
            host_url: self.host_url,
            app_version,
            libxmtp_version,
            stats: ApiStats::default(),
            identity_stats: IdentityStats::default(),
        })
    }
}

fn protobuf_headers() -> Result<HeaderMap, HttpClientError> {
    let mut headers = HeaderMap::new();

    headers.insert("Content-Type", "application/x-protobuf".parse()?);
    headers.insert("Accept", "application/x-protobuf".parse()?);
    Ok(headers)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsClient for XmtpHttpApiClient {
    type Error = ApiClientError<HttpClientError>;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.stats.upload_key_package.count_request();

        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::UPLOAD_KEY_PACKAGE))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::UploadKeyPackage, HttpClientError::from(e))
            })?;

        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::UploadKeyPackage, e))
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::FETCH_KEY_PACKAGES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::FetchKeyPackages, HttpClientError::from(e))
            })?;
        tracing::debug!("fetch_key_packages");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::FetchKeyPackages, e))
    }

    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.stats.send_group_messages.count_request();

        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::SEND_GROUP_MESSAGES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::SendGroupMessages, HttpClientError::from(e))
            })?;

        tracing::debug!("send_group_messages");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SendGroupMessages, e))
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        self.stats.send_welcome_messages.count_request();
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::SEND_WELCOME_MESSAGES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::SendWelcomeMessages, HttpClientError::from(e))
            })?;

        tracing::debug!("send_welcome_messages");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::SendWelcomeMessages, e))
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        self.stats.query_group_messages.count_request();
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::QUERY_GROUP_MESSAGES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::QueryGroupMessages, HttpClientError::from(e))
            })?;

        tracing::debug!("query_group_messages");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::QueryGroupMessages, e))
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        self.stats.query_welcome_messages.count_request();
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::QUERY_WELCOME_MESSAGES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::QueryWelcomeMessages, HttpClientError::from(e))
            })?;

        tracing::debug!("query_welcome_messages");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::QueryWelcomeMessages, e))
    }

    fn stats(&self) -> ApiStats {
        self.stats.clone()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsStreams for XmtpHttpApiClient {
    type Error = ApiClientError<HttpClientError>;

    // hard to avoid boxing here:
    // 1.) use `hyper` instead of `reqwest` and create our own `Stream` type
    // 2.) ise `impl Stream` in return of `XmtpMlsStreams` but that
    // breaks the `mockall::` functionality, since `mockall` does not support `impl Trait` in
    // `Trait` yet.

    #[cfg(not(target_arch = "wasm32"))]
    type GroupMessageStream<'a> = stream::BoxStream<'a, Result<GroupMessage, Self::Error>>;
    #[cfg(not(target_arch = "wasm32"))]
    type WelcomeMessageStream<'a> = stream::BoxStream<'a, Result<WelcomeMessage, Self::Error>>;

    #[cfg(target_arch = "wasm32")]
    type GroupMessageStream<'a> = stream::LocalBoxStream<'a, Result<GroupMessage, Self::Error>>;
    #[cfg(target_arch = "wasm32")]
    type WelcomeMessageStream<'a> = stream::LocalBoxStream<'a, Result<WelcomeMessage, Self::Error>>;

    #[tracing::instrument(skip_all)]
    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Self::Error> {
        Ok(create_grpc_stream::<_, GroupMessage>(
            request,
            self.endpoint(ApiEndpoints::SUBSCRIBE_GROUP_MESSAGES),
            self.http_client.clone(),
        )
        .await?)
    }

    #[tracing::instrument(skip_all)]
    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Self::Error> {
        tracing::debug!("subscribe_welcome_messages");
        Ok(create_grpc_stream::<_, WelcomeMessage>(
            request,
            self.endpoint(ApiEndpoints::SUBSCRIBE_WELCOME_MESSAGES),
            self.http_client.clone(),
        )
        .await?)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpIdentityClient for XmtpHttpApiClient {
    type Error = ApiClientError<HttpClientError>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::PUBLISH_IDENTITY_UPDATE))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::PublishIdentityUpdate, HttpClientError::from(e))
            })?;

        tracing::debug!("publish_identity_update");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::PublishIdentityUpdate, e))
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Self::Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::GET_IDENTITY_UPDATES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::GetIdentityUpdatesV2, HttpClientError::from(e))
            })?;

        tracing::debug!("get_identity_updates_v2");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::GetIdentityUpdatesV2, e))
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::GET_INBOX_IDS))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec());
        tracing::error!("{:?}", res);
        let res = res
            .send()
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::GetInboxIds, HttpClientError::from(e)))?;

        tracing::debug!("get_inbox_ids");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::GetInboxIds, e))
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::VERIFY_SMART_CONTRACT_WALLET_SIGNATURES))
            .headers(protobuf_headers()?)
            .body(request.encode_to_vec())
            .send()
            .await
            .map_err(|e| {
                ApiClientError::new(ApiEndpoint::VerifyScwSignature, HttpClientError::from(e))
            })?;

        tracing::debug!("verify_smart_contract_wallet_signatures");
        handle_error_proto(res)
            .await
            .map_err(|e| ApiClientError::new(ApiEndpoint::VerifyScwSignature, e))
    }

    fn identity_stats(&self) -> IdentityStats {
        self.identity_stats.clone()
    }
}

// tests
#[cfg(test)]
pub mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use xmtp_proto::xmtp::mls::api::v1::KeyPackageUpload;

    use crate::constants::ApiUrls;

    use super::*;

    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(test)]
    fn _setup() {
        xmtp_common::logger();
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_upload_key_package() {
        let mut client = XmtpHttpApiClient::builder();
        client.set_host(ApiUrls::LOCAL_ADDRESS.to_string());
        client.set_app_version("".into()).unwrap();
        client
            .set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        let client = client.build().await.unwrap();
        let result = client
            .upload_key_package(UploadKeyPackageRequest {
                is_inbox_id_credential: false,
                key_package: Some(KeyPackageUpload {
                    key_package_tls_serialized: vec![1, 2, 3],
                }),
            })
            .await;

        assert!(result.is_err());
        println!("{:?}", result);
        assert!(result
            .as_ref()
            .err()
            .unwrap()
            .to_string()
            .contains("invalid identity"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_get_inbox_ids() {
        use xmtp_proto::xmtp::identity::api::v1::{
            get_inbox_ids_request::Request, GetInboxIdsRequest,
        };
        use xmtp_proto::xmtp::identity::associations::IdentifierKind;
        let mut client = XmtpHttpApiClient::builder();
        client.set_host(ApiUrls::LOCAL_ADDRESS.to_string());
        client.set_app_version("".into()).unwrap();
        client
            .set_libxmtp_version(env!("CARGO_PKG_VERSION").into())
            .unwrap();
        let client = client.build().await.unwrap();
        let result = client
            .get_inbox_ids(GetInboxIdsRequest {
                requests: vec![Request {
                    identifier: "0xC2e3f813297E7b42a89e0b2FAa66f2034831984f".to_string(),
                    identifier_kind: IdentifierKind::Ethereum as i32,
                }],
            })
            .await;
        assert!(result.is_ok());
    }
}
