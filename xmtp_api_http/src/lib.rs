pub mod constants;
mod util;

use async_trait::async_trait;
use util::{create_grpc_stream, handle_error};
use xmtp_proto::api_client::{Error, ErrorKind, XmtpIdentityClient};
use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
    GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
    GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
};
use xmtp_proto::xmtp::mls::api::v1::{GroupMessage, WelcomeMessage};
use xmtp_proto::{
    api_client::{GroupMessageStream, WelcomeMessageStream, XmtpMlsClient},
    xmtp::mls::api::v1::{
        FetchKeyPackagesRequest, FetchKeyPackagesResponse, GetIdentityUpdatesRequest,
        GetIdentityUpdatesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, RegisterInstallationRequest,
        RegisterInstallationResponse, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
        SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    },
};

use crate::constants::ApiEndpoints;

#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

pub struct XmtpHttpApiClient {
    http_client: reqwest::Client,
    host_url: String,
}

impl XmtpHttpApiClient {
    pub fn new(host_url: String) -> Result<Self, HttpClientError> {
        let client = reqwest::Client::builder()
            .connection_verbose(true)
            .build()?;

        Ok(XmtpHttpApiClient {
            http_client: client,
            host_url,
        })
    }

    fn endpoint(&self, endpoint: &str) -> String {
        format!("{}{}", self.host_url, endpoint)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl XmtpMlsClient for XmtpHttpApiClient {
    async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, Error> {
        let res = self
            .http_client
            .post(self.endpoint(ApiEndpoints::REGISTER_INSTALLATION))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?
            .bytes()
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        log::debug!("register_installation");
        handle_error(&*res)
    }

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

        log::debug!("upload_key_package");
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

        log::debug!("fetch_key_packages");
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

        log::debug!("send_group_messages");
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

        log::debug!("send_welcome_messages");
        handle_error(&*res)
    }

    // deprecated
    async fn get_identity_updates(
        &self,
        _request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Error> {
        unimplemented!()
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

        log::debug!("query_group_messages");
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

        log::debug!("query_welcome_messages");
        handle_error(&*res)
    }

    async fn subscribe_group_messages(
        &self,
        request: SubscribeGroupMessagesRequest,
    ) -> Result<GroupMessageStream, Error> {
        log::debug!("subscribe_group_messages");
        create_grpc_stream::<_, GroupMessage>(
            request,
            self.endpoint(ApiEndpoints::SUBSCRIBE_GROUP_MESSAGES),
            self.http_client.clone(),
        )
        .await
    }

    async fn subscribe_welcome_messages(
        &self,
        request: SubscribeWelcomeMessagesRequest,
    ) -> Result<WelcomeMessageStream, Error> {
        log::debug!("subscribe_welcome_messages");
        create_grpc_stream::<_, WelcomeMessage>(
            request,
            self.endpoint(ApiEndpoints::SUBSCRIBE_WELCOME_MESSAGES),
            self.http_client.clone(),
        )
        .await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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

        log::debug!("publish_identity_update");
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

        log::debug!("get_identity_updates_v2");
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

        log::debug!("get_inbox_ids");
        handle_error(&*res)
    }
}

// tests
#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::mls::api::v1::KeyPackageUpload;

    use crate::constants::ApiUrls;

    use super::*;

    #[tokio::test]
    async fn test_register_installation() {
        let client = XmtpHttpApiClient::new(ApiUrls::LOCAL_ADDRESS.to_string()).unwrap();
        let result = client
            .register_installation(RegisterInstallationRequest {
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
