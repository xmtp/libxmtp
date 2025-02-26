//! Compatibility layer for d14n and previous xmtp_api crate

//TODO: Remove once d14n integration complete
#![allow(unused)]

use crate::{endpoints::d14n::GetInboxIds, PublishClientEnvelopes, QueryEnvelopes};
use xmtp_api_grpc::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams};
use xmtp_proto::traits::Client;
use xmtp_proto::traits::{ApiError, Query};
use xmtp_proto::v4_utils::{
    build_group_message_topic, build_identity_topic_from_hex_encoded, build_key_package_topic,
    build_welcome_message_topic, extract_client_envelope, extract_unsigned_originator_envelope,
};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::{
    IdentityUpdateLog, Response,
};
use xmtp_proto::xmtp::identity::api::v1::{
    get_inbox_ids_response, GetIdentityUpdatesRequest, GetIdentityUpdatesResponse,
    GetInboxIdsRequest, GetInboxIdsResponse, PublishIdentityUpdateRequest,
    PublishIdentityUpdateResponse, VerifySmartContractWalletSignaturesRequest,
    VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::mls::api::v1::{
    group_message, group_message_input, welcome_message, welcome_message_input,
    FetchKeyPackagesRequest, FetchKeyPackagesResponse, GroupMessage, QueryGroupMessagesRequest,
    QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, UploadKeyPackageRequest, WelcomeMessage,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, GetInboxIdsResponse as GetInboxIdsResponseV4, QueryEnvelopesResponse,
};

const DEFAULT_PAGINATION_LIMIT: u32 = 100;

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
    ApiError<E>: From<ApiError<<P as Client>::Error>>
        + From<ApiError<<C as Client>::Error>>
        + Send
        + Sync
        + 'static,
    ApiError<E>: From<GrpcError>,
{
    type Error = ApiError<E>;

    async fn upload_key_package(
        &self,
        request: UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let envelope: ClientEnvelope = request.try_into().map_err(GrpcError::from)?;

        PublishClientEnvelopes::builder()
            .envelopes(vec![envelope])
            .build()
            .unwrap()
            .query(&self.payer_client)
            .await?;

        Ok(())
    }
    async fn fetch_key_packages(
        &self,
        request: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Self::Error> {
        let topics = request
            .installation_keys
            .iter()
            .map(|key| build_key_package_topic(key))
            .collect();

        let result: QueryEnvelopesResponse = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics,
                originator_node_ids: vec![], // todo: set later
                last_seen: None,             // todo: set later
            })
            .limit(DEFAULT_PAGINATION_LIMIT)
            .build()
            .unwrap()
            .query(&self.message_client)
            .await?;

        let key_packages = result
            .envelopes
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(FetchKeyPackagesResponse { key_packages })
    }
    async fn send_group_messages(
        &self,
        request: SendGroupMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelopes: Vec<ClientEnvelope> = request
            .messages
            .into_iter()
            .map(|message| message.try_into().map_err(GrpcError::from))
            .collect::<Result<_, _>>()?;

        PublishClientEnvelopes::builder()
            .envelopes(envelopes)
            .build()
            .unwrap()
            .query(&self.payer_client)
            .await?;

        Ok(())
    }
    async fn send_welcome_messages(
        &self,
        request: SendWelcomeMessagesRequest,
    ) -> Result<(), Self::Error> {
        let envelope: Vec<ClientEnvelope> = request
            .messages
            .into_iter()
            .map(|message| message.try_into().map_err(GrpcError::from))
            .collect::<Result<_, _>>()?;

        PublishClientEnvelopes::builder()
            .envelopes(envelope)
            .build()
            .unwrap()
            .query(&self.payer_client)
            .await?;

        Ok(())
    }
    async fn query_group_messages(
        &self,
        request: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Self::Error> {
        let query_envelopes = EnvelopesQuery {
            topics: vec![build_group_message_topic(request.group_id.as_slice())],
            originator_node_ids: Vec::new(), // todo: set later
            last_seen: None,                 // todo: set later
        };

        let response_envelopes: QueryEnvelopesResponse = QueryEnvelopes::builder()
            .envelopes(query_envelopes)
            .limit(
                request
                    .paging_info
                    .map_or(DEFAULT_PAGINATION_LIMIT, |paging| paging.limit),
            ) // Defaulting limit to 100
            .build()
            .map_err(|err| ApiError::<E>::Generic)?
            .query(&self.message_client)
            .await?;

        let messages = response_envelopes
            .envelopes
            .into_iter()
            .map(|envelope| {
                let unsigned_originator_envelope = extract_unsigned_originator_envelope(&envelope)?;
                let client_envelope = extract_client_envelope(&envelope)?;
                let payload = client_envelope.payload.ok_or(GrpcError::MissingPayload)?;

                if let Payload::GroupMessage(group_message) = payload {
                    if let Some(group_message_input::Version::V1(v1_group_message)) =
                        group_message.version
                    {
                        return Ok(GroupMessage {
                            version: Some(group_message::Version::V1(group_message::V1 {
                                id: unsigned_originator_envelope.originator_sequence_id,
                                created_ns: unsigned_originator_envelope.originator_ns as u64,
                                group_id: request.group_id.clone(),
                                data: v1_group_message.data,
                                sender_hmac: v1_group_message.sender_hmac,
                            })),
                        });
                    }
                }

                Err(GrpcError::MissingPayload)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(QueryGroupMessagesResponse {
            messages,
            paging_info: None,
        })
    }
    async fn query_welcome_messages(
        &self,
        request: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Self::Error> {
        let query = EnvelopesQuery {
            topics: vec![build_welcome_message_topic(
                request.installation_key.as_slice(),
            )],
            originator_node_ids: Vec::new(), // todo: set later
            last_seen: None,                 // todo: set later
        };

        let response_envelopes = QueryEnvelopes::builder()
            .envelopes(query)
            .limit(
                request
                    .paging_info
                    .map_or(DEFAULT_PAGINATION_LIMIT, |paging| paging.limit),
            ) // Defaulting limit to 100
            .build()
            .map_err(|err| ApiError::<E>::Generic)?
            .query(&self.message_client)
            .await?;

        let messages = response_envelopes
            .envelopes
            .into_iter()
            .filter_map(|envelope| {
                let unsigned_originator_envelope =
                    extract_unsigned_originator_envelope(&envelope).ok()?;
                let client_envelope = extract_client_envelope(&envelope).ok()?;
                let payload = client_envelope.payload?;

                if let Payload::WelcomeMessage(welcome_message) = payload {
                    if let Some(welcome_message_input::Version::V1(v1_welcome_message)) =
                        welcome_message.version
                    {
                        return Some(Ok(WelcomeMessage {
                            version: Some(welcome_message::Version::V1(welcome_message::V1 {
                                id: unsigned_originator_envelope.originator_sequence_id,
                                created_ns: unsigned_originator_envelope.originator_ns as u64,
                                installation_key: request.installation_key.clone(),
                                data: v1_welcome_message.data,
                                hpke_public_key: v1_welcome_message.hpke_public_key,
                            })),
                        }));
                    }
                }

                Some(Err(GrpcError::MissingPayload))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(QueryWelcomeMessagesResponse {
            messages,
            paging_info: None,
        })
    }
}

#[async_trait::async_trait]
impl<C, P, E> XmtpIdentityClient for D14nClient<C, P, E>
where
    E: std::error::Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client<Error = E>,
    C: Send + Sync + Client<Error = E>,
    ApiError<E>: From<ApiError<<P as Client>::Error>>
        + From<ApiError<<C as Client>::Error>>
        + Send
        + Sync
        + 'static,
    xmtp_proto::traits::ApiError<E>: From<GrpcError>,
{
    type Error = ApiError<E>;

    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Self::Error> {
        let result = PublishClientEnvelopes::builder()
            .envelopes(vec![request.try_into().map_err(GrpcError::from)?])
            .build()
            .unwrap()
            .query(&self.payer_client)
            .await?;

        Ok(PublishIdentityUpdateResponse {})
    }

    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Self::Error> {
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
            .limit(DEFAULT_PAGINATION_LIMIT)
            .build()
            .unwrap()
            .query(&self.message_client)
            .await?;

        let joined_data: Vec<_> = result
            .envelopes
            .into_iter()
            .zip(request.requests.into_iter())
            .collect();
        let responses: Vec<Response> = joined_data
            .into_iter()
            .map(|(envelopes, inner_req)| {
                let identity_update_log: IdentityUpdateLog =
                    envelopes.try_into().map_err(GrpcError::from).unwrap(); //todo: handle
                Response {
                    inbox_id: inner_req.inbox_id.clone(),
                    updates: vec![identity_update_log],
                }
            })
            .collect();

        Ok(GetIdentityUpdatesResponse { responses })
    }

    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Self::Error> {
        let res: GetInboxIdsResponseV4 = GetInboxIds::builder()
            .addresses(
                request
                    .requests
                    .iter()
                    .map(|r| r.address.clone())
                    .collect::<Vec<_>>(),
            )
            .build()
            .unwrap()
            .query(&self.message_client)
            .await?;
        Ok(GetInboxIdsResponse {
            responses: res
                .responses
                .iter()
                .map(|r| get_inbox_ids_response::Response {
                    address: r.address.clone(),
                    inbox_id: r.inbox_id.clone(),
                })
                .collect::<Vec<_>>(),
        })
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Self::Error> {
        unimplemented!()
    }
}
