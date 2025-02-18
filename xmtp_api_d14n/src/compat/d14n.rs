//! Compatibility layer for d14n and previous xmtp_api crate

//TODO: Remove once d14n integration complete
#![allow(unused)]
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams};
use xmtp_proto::traits::ApiError;
use xmtp_proto::traits::Client;

use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, GetInboxIdsRequest, GetInboxIdsResponse,
    PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::XmtpApiError;
use xmtp_proto::{
    xmtp::identity::api::v1::identity_api_client::IdentityApiClient as ProtoIdentityApiClient,
    xmtp::message_api::v1::{
        message_api_client::MessageApiClient, BatchQueryRequest, BatchQueryResponse, Envelope,
        PublishRequest, PublishResponse, QueryRequest, QueryResponse, SubscribeRequest,
    },
    xmtp::mls::api::v1::{
        mls_api_client::MlsApiClient as ProtoMlsApiClient, FetchKeyPackagesRequest,
        FetchKeyPackagesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, SendGroupMessagesRequest,
        SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
        UploadKeyPackageRequest,
    },
};

use crate::endpoints::{GetInboxIds, PublishClientEnvelopes, QueryEnvelopes};
pub struct D14nClient<C, P, E> {
    message_client: C,
    payer_client: P,
    _marker: E,
}

#[async_trait::async_trait]
impl<C, P, E> XmtpMlsClient for D14nClient<C, P, E>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client,
    C: Send + Sync + Client,
{
    type Error = ApiError<E>;
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
impl<C, P, E> XmtpIdentityClient for D14nClient<C, P, E>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client,
    C: Send + Sync + Client,
{
    type Error = ApiError<E>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        todo!()
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Self::Error> {
        todo!()
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        todo!()
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        todo!()
    }
}
