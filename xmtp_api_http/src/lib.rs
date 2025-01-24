#![warn(clippy::unwrap_used)]

pub mod constants;
mod http_stream;
mod util;

use futures::stream;
use http_stream::create_grpc_stream;
use reqwest::header;
use util::handle_error;
use xmtp_proto::api_client::{ClientWithMetadata, XmtpIdentityClient};
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
    Error, ErrorKind,
};

use crate::constants::ApiEndpoints;

#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

#[cfg(target_arch = "wasm32")]
fn reqwest_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
}

#[cfg(not(target_arch = "wasm32"))]
fn reqwest_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().connection_verbose(true)
}

#[derive(Clone)]
pub struct XmtpHttpApiClient {
    http_client: reqwest::Client,
    host_url: String,
    app_version: Option<String>,
    libxmtp_version: Option<String>,
}

impl XmtpHttpApiClient {
    pub fn new(host_url: String) -> Result<Self, HttpClientError> {
        let client = reqwest_builder().build()?;

        Ok(XmtpHttpApiClient {
            http_client: client,
            host_url,
            app_version: None,
            libxmtp_version: None,
        })
    }

    fn endpoint(&self, endpoint: &str) -> String {
        format!("{}{}", self.host_url, endpoint)
    }
}

fn metadata_err<E>(e: E) -> Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    Error::new(ErrorKind::MetadataError).with(e)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ClientWithMetadata for XmtpHttpApiClient {
    fn set_app_version(&mut self, version: String) -> Result<(), Error> {
        self.app_version = Some(version);

        let mut headers = header::HeaderMap::new();
        if let Some(app_version) = &self.app_version {
            headers.insert("x-app-version", app_version.parse().map_err(metadata_err)?);
        }
        if let Some(libxmtp_version) = &self.libxmtp_version {
            headers.insert(
                "x-libxmtp-version",
                libxmtp_version.parse().map_err(metadata_err)?,
            );
        }
        self.http_client = reqwest_builder()
            .default_headers(headers)
            .build()
            .map_err(metadata_err)?;
        Ok(())
    }
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error> {
        self.libxmtp_version = Some(version);

        let mut headers = header::HeaderMap::new();
        if let Some(app_version) = &self.app_version {
            headers.insert(
                "x-app-version",
                app_version
                    .parse()
                    .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?,
            );
        }
        if let Some(libxmtp_version) = &self.libxmtp_version {
            headers.insert(
                "x-libxmtp-version",
                libxmtp_version
                    .parse()
                    .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?,
            );
        }
        self.http_client = reqwest_builder()
            .default_headers(headers)
            .build()
            .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?;
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsClient for XmtpHttpApiClient {
    async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::UPLOAD_KEY_PACKAGE))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("upload_key_package");
        handle_error(&*res)
    }

    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::FETCH_KEY_PACKAGES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("fetch_key_packages");
        handle_error(&*res)
    }

    async fn send_group_messages(&self, request: SendGroupMessagesRequest) -> Result<(), Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::SEND_GROUP_MESSAGES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("send_group_messages");
        handle_error(&*res)
    }

    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::SEND_WELCOME_MESSAGES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("send_welcome_messages");
        handle_error(&*res)
    }

    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::QUERY_GROUP_MESSAGES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("query_group_messages");
        handle_error(&*res)
    }

    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::QUERY_WELCOME_MESSAGES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        tracing::debug!("query_welcome_messages");
        handle_error(&*res)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl XmtpMlsStreams for XmtpHttpApiClient {
    // hard to avoid boxing here:
    // 1.) use `hyper` instead of `reqwest` and create our own `Stream` type
    // 2.) ise `impl Stream` in return of `XmtpMlsStreams` but that
    // breaks the `mockall::` functionality, since `mockall` does not support `impl Trait` in
    // `Trait` yet.

    #[cfg(not(target_arch = "wasm32"))]
    type GroupMessageStream<'a> = stream::BoxStream<'a, Result<GroupMessage, Error>>;
    #[cfg(not(target_arch = "wasm32"))]
    type WelcomeMessageStream<'a> = stream::BoxStream<'a, Result<WelcomeMessage, Error>>;

    #[cfg(target_arch = "wasm32")]
    type GroupMessageStream<'a> = stream::LocalBoxStream<'a, Result<GroupMessage, Error>>;
    #[cfg(target_arch = "wasm32")]
    type WelcomeMessageStream<'a> = stream::LocalBoxStream<'a, Result<WelcomeMessage, Error>>;

    #[tracing::instrument(skip_all)]
    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Error> {
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
    ) -> Result<Self::WelcomeMessageStream<'_>, Error> {
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
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::PUBLISH_IDENTITY_UPDATE))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

        tracing::debug!("publish_identity_update");
        handle_error(&*res)
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::GET_IDENTITY_UPDATES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

        tracing::debug!("get_identity_updates_v2");
        handle_error(&*res)
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::GET_INBOX_IDS))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

        tracing::debug!("get_inbox_ids");
        handle_error(&*res)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::VERIFY_SMART_CONTRACT_WALLET_SIGNATURES))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

        tracing::debug!("verify_smart_contract_wallet_signatures");
        handle_error(&*res)
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_upload_key_package() {
        let client = XmtpHttpApiClient::new(ApiUrls::LOCAL_ADDRESS.to_string()).unwrap();
        let result = client
            .upload_key_package(UploadKeyPackageRequest {
                is_inbox_id_credential: false,
                key_package: Some(KeyPackageUpload {
                    key_package_tls_serialized: vec![1, 2, 3],
                }),
            })
            .await;

        assert!(result.is_err());
        assert!(result
            .as_ref()
            .err()
            .unwrap()
            .to_string()
            .contains("invalid identity"));
    }
}
