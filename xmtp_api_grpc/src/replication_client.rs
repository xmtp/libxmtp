#![allow(unused)]
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
// TODO switch to async mutexes
use std::time::Duration;

use futures::stream::{AbortHandle, Abortable};
use futures::{SinkExt, Stream, StreamExt, TryStreamExt};
use prost::Message;
use tokio::sync::oneshot;
use tonic::transport::ClientTlsConfig;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Streaming};

#[cfg(any(feature = "test-utils", test))]
use xmtp_proto::api_client::XmtpTestClient;
use xmtp_proto::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsStreams};

use crate::grpc_api_helper::{create_tls_channel, GrpcMutableSubscription, Subscription};
use crate::{GroupMessageStream, WelcomeMessageStream};
use xmtp_proto::v4_utils::{
    build_group_message_topic, build_identity_topic_from_hex_encoded, build_identity_update_topic,
    build_key_package_topic, build_welcome_message_topic, extract_client_envelope,
    extract_unsigned_originator_envelope,
};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::xmtp::mls::api::v1::{
    fetch_key_packages_response, group_message, group_message_input, welcome_message,
    welcome_message_input, GroupMessage, WelcomeMessage,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::replication_api_client::ReplicationApiClient;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, PublishPayerEnvelopesRequest, QueryEnvelopesRequest,
};
use xmtp_proto::xmtp::xmtpv4::payer_api::payer_api_client::PayerApiClient;
use xmtp_proto::xmtp::xmtpv4::payer_api::PublishClientEnvelopesRequest;
use xmtp_proto::{
    api_client::{MutableApiSubscription, XmtpApiClient, XmtpApiSubscription, XmtpMlsClient},
    xmtp::identity::api::v1::{
        get_inbox_ids_response, GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
    xmtp::message_api::v1::{
        BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse,
        QueryRequest, QueryResponse, SubscribeRequest,
    },
    xmtp::mls::api::v1::{
        FetchKeyPackagesRequest, FetchKeyPackagesResponse, QueryGroupMessagesRequest,
        QueryGroupMessagesResponse, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
        SendGroupMessagesRequest, SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
        SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    },
    xmtp::xmtpv4::message_api::{
        get_inbox_ids_request, GetInboxIdsRequest as GetInboxIdsRequestV4,
    },
    Error, ErrorKind, InternalError,
};

#[derive(Debug, Clone)]
pub struct ClientV4 {
    pub(crate) client: ReplicationApiClient<Channel>,
    pub(crate) payer_client: PayerApiClient<Channel>,
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) libxmtp_version: MetadataValue<tonic::metadata::Ascii>,
}

impl ClientV4 {
    pub async fn create(host: String, is_secure: bool) -> Result<Self, Error> {
        let host = host.to_string();
        let app_version = MetadataValue::try_from(&String::from("0.0.0"))
            .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?;
        let libxmtp_version = MetadataValue::try_from(&String::from("0.0.0"))
            .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?;

        let channel = match is_secure {
            true => create_tls_channel(host).await?,
            false => Channel::from_shared(host)
                .map_err(|e| Error::new(ErrorKind::SetupCreateChannelError).with(e))?
                .connect()
                .await
                .map_err(|e| Error::new(ErrorKind::SetupConnectionError).with(e))?,
        };

        // GroupMessageInputTODO(mkysel) for now we assume both payer and replication are on the same host
        let client = ReplicationApiClient::new(channel.clone());
        let payer_client = PayerApiClient::new(channel.clone());

        Ok(Self {
            client,
            payer_client,
            app_version,
            libxmtp_version,
        })
    }

    pub fn build_request<RequestType>(&self, request: RequestType) -> Request<RequestType> {
        let mut req = Request::new(request);
        req.metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        req.metadata_mut()
            .insert("x-libxmtp-version", self.libxmtp_version.clone());

        req
    }
}

impl ClientWithMetadata for ClientV4 {
    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Error> {
        self.libxmtp_version = MetadataValue::try_from(&version)
            .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?;

        Ok(())
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Error> {
        self.app_version = MetadataValue::try_from(&version)
            .map_err(|e| Error::new(ErrorKind::MetadataError).with(e))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl XmtpApiClient for ClientV4 {
    type Subscription = Subscription;
    type MutableSubscription = GrpcMutableSubscription;

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        unimplemented!();
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Subscription, Error> {
        unimplemented!();
    }

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<GrpcMutableSubscription, Error> {
        unimplemented!();
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        unimplemented!();
    }

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        unimplemented!();
    }
}

#[async_trait::async_trait]
impl XmtpMlsClient for ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(&self, req: UploadKeyPackageRequest) -> Result<(), Error> {
        self.send_messages_to_payer(vec![req]).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn fetch_key_packages(
        &self,
        req: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error> {
        let topics = req
            .installation_keys
            .iter()
            .map(|key| build_key_package_topic(key.as_slice()))
            .collect();

        let envelopes = self.query_v4_envelopes(topics).await?;
        let key_packages: Result<Vec<_>, Error> = envelopes
            .iter()
            .map(|envelopes| {
                // The last envelope should be the newest key package upload
                let unsigned = envelopes.last().ok_or_else(|| {
                    Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                        .with("No envelopes found")
                })?;

                let client_env = extract_client_envelope(unsigned)?;

                if let Some(Payload::UploadKeyPackage(upload_key_package)) = client_env.payload {
                    let key_package = upload_key_package.key_package.ok_or_else(|| {
                        Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                            .with("Missing key package")
                    })?;

                    Ok(fetch_key_packages_response::KeyPackage {
                        key_package_tls_serialized: key_package.key_package_tls_serialized,
                    })
                } else {
                    Err(
                        Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                            .with("Payload is not a key package"),
                    )
                }
            })
            .collect();

        Ok(FetchKeyPackagesResponse {
            key_packages: key_packages?,
        })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(&self, req: SendGroupMessagesRequest) -> Result<(), Error> {
        self.send_messages_to_payer(req.messages).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(&self, req: SendWelcomeMessagesRequest) -> Result<(), Error> {
        self.send_messages_to_payer(req.messages).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        req: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error> {
        let client = &mut self.client.clone();
        let res = client
            .query_envelopes(QueryEnvelopesRequest {
                query: Some(EnvelopesQuery {
                    topics: vec![build_group_message_topic(req.group_id.as_slice())],
                    originator_node_ids: vec![],
                    last_seen: None,
                }),
                limit: 100,
            })
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        let envelopes = res.into_inner().envelopes;
        let response = QueryGroupMessagesResponse {
            messages: envelopes
                .iter()
                .map(|envelope| {
                    let unsigned_originator_envelope =
                        extract_unsigned_originator_envelope(envelope)?;
                    let client_envelope = extract_client_envelope(envelope)?;
                    let payload = client_envelope.payload.ok_or_else(|| {
                        Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                    })?;
                    let Payload::GroupMessage(group_message) = payload else {
                        return Err(Error::new(ErrorKind::InternalError(
                            InternalError::MissingPayloadError,
                        )));
                    };

                    let group_message_input::Version::V1(v1_group_message) =
                        group_message.version.ok_or_else(|| {
                            Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                        })?;

                    Ok(GroupMessage {
                        version: Some(group_message::Version::V1(group_message::V1 {
                            id: unsigned_originator_envelope.originator_sequence_id,
                            created_ns: unsigned_originator_envelope.originator_ns as u64,
                            group_id: req.group_id.clone(),
                            data: v1_group_message.data,
                            sender_hmac: v1_group_message.sender_hmac,
                        })),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
            paging_info: None,
        };
        Ok(response)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_welcome_messages(
        &self,
        req: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error> {
        let client = &mut self.client.clone();
        let res = client
            .query_envelopes(QueryEnvelopesRequest {
                query: Some(EnvelopesQuery {
                    topics: vec![build_welcome_message_topic(req.installation_key.as_slice())],
                    originator_node_ids: vec![],
                    last_seen: None,
                }),
                limit: 100,
            })
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        let envelopes = res.into_inner().envelopes;
        let response = QueryWelcomeMessagesResponse {
            messages: envelopes
                .iter()
                .map(|envelope| {
                    let unsigned_originator_envelope =
                        extract_unsigned_originator_envelope(envelope)?;
                    let client_envelope = extract_client_envelope(envelope)?;
                    let payload = client_envelope.payload.ok_or_else(|| {
                        Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                    })?;
                    let Payload::WelcomeMessage(welcome_message) = payload else {
                        return Err(Error::new(ErrorKind::InternalError(
                            InternalError::MissingPayloadError,
                        )));
                    };
                    let welcome_message_input::Version::V1(v1_welcome_message) =
                        welcome_message.version.ok_or_else(|| {
                            Error::new(ErrorKind::InternalError(InternalError::MissingPayloadError))
                        })?;

                    Ok(WelcomeMessage {
                        version: Some(welcome_message::Version::V1(welcome_message::V1 {
                            id: unsigned_originator_envelope.originator_sequence_id,
                            created_ns: unsigned_originator_envelope.originator_ns as u64,
                            installation_key: req.installation_key.clone(),
                            data: v1_welcome_message.data,
                            hpke_public_key: v1_welcome_message.hpke_public_key,
                        })),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
            paging_info: None,
        };
        Ok(response)
    }
}

#[async_trait::async_trait]
impl XmtpMlsStreams for ClientV4 {
    type GroupMessageStream<'a> = GroupMessageStream;
    type WelcomeMessageStream<'a> = WelcomeMessageStream;

    async fn subscribe_group_messages(
        &self,
        req: SubscribeGroupMessagesRequest,
    ) -> Result<Self::GroupMessageStream<'_>, Error> {
        unimplemented!();
    }

    async fn subscribe_welcome_messages(
        &self,
        req: SubscribeWelcomeMessagesRequest,
    ) -> Result<Self::WelcomeMessageStream<'_>, Error> {
        unimplemented!();
    }
}

#[async_trait::async_trait]
impl XmtpIdentityClient for ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        self.send_messages_to_payer(vec![request]).await?;
        Ok(PublishIdentityUpdateResponse {})
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_inbox_ids(
        &self,
        request: GetInboxIdsRequest,
    ) -> Result<GetInboxIdsResponse, Error> {
        let client = &mut self.client.clone();
        let req = GetInboxIdsRequestV4 {
            requests: request
                .requests
                .into_iter()
                .map(|r| get_inbox_ids_request::Request { address: r.address })
                .collect(),
        };

        let res = client.get_inbox_ids(self.build_request(req)).await;

        res.map(|response| response.into_inner())
            .map(|response| GetInboxIdsResponse {
                responses: response
                    .responses
                    .into_iter()
                    .map(|r| get_inbox_ids_response::Response {
                        address: r.address,
                        inbox_id: r.inbox_id,
                    })
                    .collect(),
            })
            .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_identity_updates_v2(
        &self,
        request: GetIdentityUpdatesV2Request,
    ) -> Result<GetIdentityUpdatesV2Response, Error> {
        let topics: Result<Vec<_>, Error> = request
            .requests
            .iter()
            .map(|r| build_identity_topic_from_hex_encoded(&r.inbox_id.clone()))
            .collect();
        let v4_envelopes = self.query_v4_envelopes(topics?).await?;
        let joined_data = v4_envelopes
            .into_iter()
            .zip(request.requests.into_iter())
            .collect::<Vec<_>>();
        let responses = joined_data
            .iter()
            .map(|(envelopes, inner_req)| {
                let identity_updates = envelopes
                    .iter()
                    .map(convert_v4_envelope_to_identity_update)
                    .collect::<Result<Vec<IdentityUpdateLog>, Error>>()?;

                Ok(get_identity_updates_response::Response {
                    inbox_id: inner_req.inbox_id.clone(),
                    updates: identity_updates,
                })
            })
            .collect::<Result<Vec<get_identity_updates_response::Response>, Error>>()?;

        Ok(GetIdentityUpdatesV2Response { responses })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error> {
        unimplemented!()
    }
}

#[cfg(any(feature = "test-utils", test))]
#[async_trait::async_trait]
impl XmtpTestClient for ClientV4 {
    async fn create_local() -> Self {
        todo!()
    }

    async fn create_dev() -> Self {
        todo!()
    }
}
impl ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_v4_envelopes(
        &self,
        topics: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<OriginatorEnvelope>>, Error> {
        let requests = topics.iter().map(|topic| async {
            let client = &mut self.client.clone();
            let v4_envelopes = client
                .query_envelopes(QueryEnvelopesRequest {
                    query: Some(EnvelopesQuery {
                        topics: vec![topic.clone()],
                        originator_node_ids: vec![],
                        last_seen: None,
                    }),
                    limit: 100,
                })
                .await
                .map_err(|err| Error::new(ErrorKind::IdentityError).with(err))?;

            Ok(v4_envelopes.into_inner().envelopes)
        });

        futures::future::try_join_all(requests).await
    }

    async fn send_messages_to_payer<T>(&self, messages: Vec<T>) -> Result<(), Error>
    where
        T: TryInto<ClientEnvelope>,
        <T as TryInto<ClientEnvelope>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let client = &mut self.payer_client.clone();

        let envelopes: Vec<ClientEnvelope> = messages
            .into_iter()
            .map(|message| {
                message
                    .try_into()
                    .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
            })
            .collect::<Result<_, _>>()?;

        client
            .publish_client_envelopes(PublishClientEnvelopesRequest { envelopes })
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        Ok(())
    }
}

fn convert_v4_envelope_to_identity_update(
    envelope: &OriginatorEnvelope,
) -> Result<IdentityUpdateLog, Error> {
    let mut unsigned_originator_envelope = envelope.unsigned_originator_envelope.as_slice();
    let originator_envelope = UnsignedOriginatorEnvelope::decode(&mut unsigned_originator_envelope)
        .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;

    let payer_envelope = originator_envelope
        .payer_envelope
        .ok_or(Error::new(ErrorKind::IdentityError).with("Payer envelope is None"))?;

    // TODO: validate payer signatures
    let mut unsigned_client_envelope = payer_envelope.unsigned_client_envelope.as_slice();

    let client_envelope = ClientEnvelope::decode(&mut unsigned_client_envelope)
        .map_err(|e| Error::new(ErrorKind::IdentityError).with(e))?;
    let payload = client_envelope
        .payload
        .ok_or(Error::new(ErrorKind::IdentityError).with("Payload is None"))?;

    let identity_update = match payload {
        Payload::IdentityUpdate(update) => update,
        _ => {
            return Err(
                Error::new(ErrorKind::IdentityError).with("Payload is not an identity update")
            )
        }
    };

    Ok(IdentityUpdateLog {
        sequence_id: originator_envelope.originator_sequence_id,
        server_timestamp_ns: originator_envelope.originator_ns as u64,
        update: Some(identity_update),
    })
}
