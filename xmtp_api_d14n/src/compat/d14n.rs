//! Compatibility layer for d14n and previous xmtp_api crate

//TODO: Remove once d14n integration complete
#![allow(unused)]

use crate::{endpoints::d14n::GetInboxIds, PublishClientEnvelopes, QueryEnvelopes};
use xmtp_api_grpc::replication_client::convert_v4_envelope_to_identity_update;
use xmtp_api_grpc::{Error, GrpcError};
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{Envelope, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams};
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiError, Query};
use xmtp_proto::v4_utils::build_identity_topic_from_hex_encoded;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::{IdentityUpdateLog, Response};
use xmtp_proto::xmtp::identity::api::v1::{
    get_identity_updates_response, get_inbox_ids_response, GetIdentityUpdatesRequest,
    GetIdentityUpdatesResponse, GetInboxIdsRequest, GetInboxIdsResponse,
    PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::mls::api::v1::{
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, QueryGroupMessagesRequest,
    QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, UploadKeyPackageRequest,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, GetInboxIdsResponse as GetInboxIdsResponseV4, QueryEnvelopesRequest,
    QueryEnvelopesResponse,
};

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
        // let envelopes: Vec<ClientEnvelope> = request
        //     .into_iter()
        //     .map(|message| message.try_into().map_err(GrpcError::from))
        //     .collect::<Result<_, _>>()?;
        // PublishClientEnvelopes::builder()
        //     .envelopes(envelopes)
        //     .build()
        //     .unwrap()
        //     .query(&self.payer_client)
        //     .await?
        todo!()
    }
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        // let topics = request
        //     .installation_keys
        //     .iter()
        //     .map(|key| build_key_package_topic(key.as_slice()))
        //     .collect();
        // QueryEnvelopes::builder()
        //     .envelopes(topics)
        //     .limit(0u32) //todo: do we need to get it as a var in the parent function?
        //     .build()
        //     .unwrap()
        //     .query(&self.message_client)
        //     .await?;
        todo!()
    }
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        // PublishClientEnvelopes::builder()
        //     .envelopes(request.messages.try_collect()?)
        //     .build()
        //     .unwrap()
        //     .query(&self.payer_client)
        //     .await
        todo!()
    }
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        // PublishClientEnvelopes::builder()
        //     .envelopes(request.messages.try_collect()?)
        //     .build()
        //     .unwrap()
        //     .query(&self.payer_client)
        todo!()
    }
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        // let query_envelopes = EnvelopesQuery {
        //     topics: vec![build_group_message_topic(request.group_id.as_slice())],
        //     originator_node_ids: vec![], //todo: set later
        //     last_seen: None,             //todo: set later
        // };
        // QueryEnvelopes::builder()
        //     .envelopes(query_envelopes)
        //     .limit(0u32) //todo: do we need to get it as a var in the parent function?
        //     .build()
        //     .unwrap()
        //     .query(&self.message_client)
        todo!()
    }
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        // let query_envelopes = EnvelopesQuery {
        //     topics: vec![build_welcome_message_topic(
        //         request.installation_key.as_slice(),
        //     )],
        //     originator_node_ids: vec![], //todo: set later
        //     last_seen: None,             //todo: set later
        // };
        // QueryEnvelopes::builder()
        //     .envelopes(query_envelopes)
        //     .limit(0u32) //todo: do we need to get it as a var in the parent function?
        //     .build()
        //     .unwrap()
        //     .query(&self.message_client)
        todo!()
    }
}

#[async_trait::async_trait]
impl<C, P, E> XmtpIdentityClient for D14nClient<C, P, E>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client<Error = E>,
    C: Send + Sync + Client<Error = E>,
    ApiError<E>: From<ApiError<<P as Client>::Error>>,
{
    type Error = ApiError<E>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        PublishClientEnvelopes::builder()
            .envelopes(vec![request.try_into().map_err(ApiError::ProtoError)?])
            .build()
            .unwrap()
            .query(&self.payer_client)
            .await?.map_err(ApiError::from)
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Self::Error> {
        let limit = 0; // q: where we should set the limits? here or get it as the fn params?
        let topics = request
            .requests
            .iter()
            .map(|r| build_identity_topic_from_hex_encoded(&r.inbox_id.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(GrpcError::from)
            .unwrap();

        let result: QueryEnvelopesResponse = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: topics.clone(),
                originator_node_ids: vec![], //todo: set later
                last_seen: None,             //todo: set later
            })
            .limit(0u32)
            .build()
            .unwrap()
            .query(&self.message_client)
            .await?.map_err(ApiError::from);

        let joined_data: Vec<_> = result
            .envelopes
            .into_iter()
            .zip(request.requests.into_iter())
            .collect();
        let responses: Vec<Response> = joined_data
            .iter()
            .map(|(envelopes, inner_req)| {
                let identity_updates =vec![convert_v4_envelope_to_identity_update(envelopes).map_err(GrpcError::from).unwrap()];
                Response {
                    inbox_id: inner_req.inbox_id.clone(),
                    updates: identity_updates,
                }
            })
            .collect()?;

        Ok(GetIdentityUpdatesResponse { responses })
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        let res: GetInboxIdsResponseV4 = GetInboxIds::builder()
            .addresses(request.requests.iter().map(|r| r.address.clone()).collect())
            .build()
            .unwrap()
            .query(&self.message_client)
            .await
            .unwrap().map_err(Error::from)?;
        res.responses
            .iter()
            .map(|r| get_inbox_ids_response::Response {
                address: r.address.clone(),
                inbox_id: r.inbox_id.clone(),
            })
            .collect()
            .map_err(Error::from)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        todo!()
    }
}
