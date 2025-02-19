use crate::endpoints::v3::{
    GetIdentityUpdatesV2, GetInboxIds, PublishIdentityUpdate, VerifySmartContractWalletSignatures,
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
impl<C> XmtpMlsClient for V3Client<C> {
    type Error = ApiError<Box<dyn XmtpApiError>>;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        todo!()
    }
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        todo!()
    }
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        todo!()
    }
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        todo!()
    }
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        todo!()
    }
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        todo!()
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
            .requests(request.requests)
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
            .query(&self.client)
    }
}
