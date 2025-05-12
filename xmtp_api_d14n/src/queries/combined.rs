use xmtp_common::RetryableError;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::traits::{ApiClientError, Client, Query};
use xmtp_proto::{mls_v1, prelude::XmtpMlsClient};

use crate::v3::{
    FetchKeyPackages, QueryGroupMessages, QueryWelcomeMessages, SendGroupMessages,
    SendWelcomeMessages, UploadKeyPackage,
};

#[derive(Clone)]
pub struct CombinedD14nClient<C, D> {
    pub(crate) v3_client: C,
    pub(crate) xmtpd_client: D,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, D, E> XmtpMlsClient for CombinedD14nClient<C, D>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    C: Send + Sync + Client<Error = E>,
    D: Send + Sync + Client<Error = E>,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>> + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;

    async fn upload_key_package(
        &self,
        request: mls_v1::UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        UploadKeyPackage::builder()
            .key_package(request.key_package)
            .is_inbox_id_credential(request.is_inbox_id_credential)
            .build()?
            .query(&self.v3_client)
            .await
    }

    async fn fetch_key_packages(
        &self,
        request: mls_v1::FetchKeyPackagesRequest,
    ) -> Result<mls_v1::FetchKeyPackagesResponse, Self::Error> {
        FetchKeyPackages::builder()
            .installation_keys(request.installation_keys)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }

    async fn send_group_messages(
        &self,
        request: mls_v1::SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendGroupMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.v3_client)
            .await
    }

    async fn send_welcome_messages(
        &self,
        request: mls_v1::SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendWelcomeMessages::builder()
            .messages(request.messages)
            .build()?
            .query(&self.v3_client)
            .await
    }
    async fn query_group_messages(
        &self,
        request: mls_v1::QueryGroupMessagesRequest,
    ) -> Result<mls_v1::QueryGroupMessagesResponse, Self::Error> {
        QueryGroupMessages::builder()
            .group_id(request.group_id)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }
    async fn query_welcome_messages(
        &self,
        request: mls_v1::QueryWelcomeMessagesRequest,
    ) -> Result<mls_v1::QueryWelcomeMessagesResponse, Self::Error> {
        QueryWelcomeMessages::builder()
            .installation_key(request.installation_key)
            .paging_info(request.paging_info)
            .build()?
            .query(&self.xmtpd_client)
            .await
    }

    fn stats(&self) -> ApiStats {
        Default::default()
    }
}
