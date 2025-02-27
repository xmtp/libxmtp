use crate::endpoints::v3::{
    GetIdentityUpdatesV2, GetInboxIds, PublishIdentityUpdate, VerifySmartContractWalletSignatures,
};
use crate::{
    FetchKeyPackages, QueryGroupMessages, QueryWelcomeMessages, SendGroupMessages,
    SendWelcomeMessages, UploadKeyPackage,
};
use xmtp_proto::api_client::{XmtpApiClient, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams};
use xmtp_proto::traits::{ApiError, Query};
use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, GetInboxIdsRequest, GetInboxIdsResponse,
    PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::mls::api::v1::{
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, QueryGroupMessagesRequest,
    QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, UploadKeyPackageRequest,
};
use xmtp_proto::XmtpApiError;

pub struct V3Client<C> {
    client: C,
}

#[async_trait::async_trait]
impl<C: xmtp_proto::traits::Client> XmtpMlsClient for V3Client<C> {
    type Error = ApiError<Box<dyn XmtpApiError>>;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        UploadKeyPackage::builder()
            .key_package(request.key_package.unwrap())
            .is_inbox_id_credential(request.is_inbox_id_credential)
            .build()
            .unwrap()
            .query(&self.client).await?
    }
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        FetchKeyPackages::builder()
            .installation_keys(request.installation_keys)
            .build()
            .unwrap()
            .query(&self.client)?
    }
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendGroupMessages::builder()
            .messages(request.messages)
            .build()
            .unwrap()
            .query(&self.client)?
    }
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        SendWelcomeMessages::builder()
            .messages(request.messages)
            .build()
            .unwrap()
            .query(&self.client)?
    }
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        QueryGroupMessages::builder()
            .group_id(request.group_id)
            .build()
            .unwrap()
            .query(&self.client).await
    }
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        QueryWelcomeMessages::builder()
            .installation_key(request.installation_key)
            .paging_info(request.paging_info.unwrap())
            .build()
            .unwrap()
            .query(&self.client)
    }
}

#[async_trait::async_trait]
impl<C> XmtpIdentityClient for V3Client<C> {
    type Error = ApiError<Box<dyn XmtpApiError>>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        PublishIdentityUpdate::builder()
            //todo: handle error or tryFrom
            .identity_update(request.identity_update.unwrap())
            .build()
            .unwrap()
            .query(&self.client)
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Self::Error> {
        GetIdentityUpdatesV2::builder()
            .requests(request.requests)
            .build()
            .unwrap()
            .query(&self.client)
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        GetInboxIds::builder()
            .addresses(request.requests.iter().map(|r| r.address.to_string()))
            .build()
            .unwrap()
            .query(&self.client)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        VerifySmartContractWalletSignatures::builder()
            .signatures(request.signatures)
            .build()
            .unwrap()
            .query(&self.client).await
    }
}
