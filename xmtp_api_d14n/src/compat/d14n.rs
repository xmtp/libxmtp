//! Compatibility layer for d14n and previous xmtp_api crate

//TODO: Remove once d14n integration complete
#![allow(unused)]

use crate::endpoints::d14n::GetInboxIds;
use crate::{PublishClientEnvelopes, QueryEnvelopes};
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams};
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiError, Query};
use xmtp_proto::v4_utils::{
    build_group_message_topic, build_key_package_topic, build_welcome_message_topic,
};
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
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;

pub struct D14nClient<C, P, E> {
    message_client: C,
    payer_client: P,
    _marker: E,
}

trait TryCollect: IntoIterator {
    fn try_collect<U, E>(self) -> Result<Vec<U>, E>
    where
        Self: Sized,
        Self::Item: TryInto<U, Error = E>,
        E: From<E>,
    {
        self.into_iter()
            .map(|item| item.try_into().map_err(E::from))
            .collect()
    }
}

// Implement the trait for all iterators
impl<T> TryCollect for T where T: IntoIterator {}

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
        PublishClientEnvelopes::builder()
            .envelopes(request.try_collect()?)
            .build()
            .unwrap()
            .query(&self.payer_client)
    }
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        let topics = request
            .installation_keys
            .iter()
            .map(|key| build_key_package_topic(key.as_slice()))
            .collect();
        QueryEnvelopes::builder()
            .envelopes(topics)
            .limit(0u32) //todo: do we need to get it as a var in the parent function?
            .build()
            .unwrap()
            .query(&self.message_client)
    }
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        PublishClientEnvelopes::builder()
            .envelopes(request.messages.try_collect()?)
            .build()
            .unwrap()
            .query(&self.payer_client)
    }
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        PublishClientEnvelopes::builder()
            .envelopes(request.messages.try_collect()?)
            .build()
            .unwrap()
            .query(&self.payer_client)
    }
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        let query_envelopes = EnvelopesQuery {
            topics: vec![build_group_message_topic(request.group_id.as_slice())],
            originator_node_ids: vec![], //todo: set later
            last_seen: None,             //todo: set later
        };
        QueryEnvelopes::builder()
            .envelopes(query_envelopes)
            .limit(0u32) //todo: do we need to get it as a var in the parent function?
            .build()
            .unwrap()
            .query(&self.message_client)
    }
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        let query_envelopes = EnvelopesQuery {
            topics: vec![build_welcome_message_topic(
                request.installation_key.as_slice(),
            )],
            originator_node_ids: vec![], //todo: set later
            last_seen: None,             //todo: set later
        };
        QueryEnvelopes::builder()
            .envelopes(query_envelopes)
            .limit(0u32) //todo: do we need to get it as a var in the parent function?
            .build()
            .unwrap()
            .query(&self.message_client)
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
        PublishClientEnvelopes::builder()
            .envelopes(request.identity_update.try_collect()?)
            .build()
            .unwrap()
            .query(&self.payer_client)
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
        GetInboxIds::builder()
            .addresses(request.requests.try_collect()?)
            .build()
            .unwrap()
            .query(&self.message_client)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        todo!()
    }
}
