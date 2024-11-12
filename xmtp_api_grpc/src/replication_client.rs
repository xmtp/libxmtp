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
    api_client::{
        Error, ErrorKind, MutableApiSubscription, XmtpApiClient, XmtpApiSubscription, XmtpMlsClient,
    },
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
};
use xmtp_proto::v4_utils::{build_group_message_topic, build_identity_topic_from_hex_encoded, build_identity_update_topic, build_key_package_topic, build_welcome_message_topic, extract_client_envelope, extract_unsigned_originator_envelope};

async fn create_tls_channel(address: String) -> Result<Channel, Error> {
    let channel = Channel::from_shared(address)
        .map_err(|e| Error::new(ErrorKind::SetupCreateChannelError).with(e))?
        // Purpose: This setting controls the size of the initial connection-level flow control window for HTTP/2, which is the underlying protocol for gRPC.
        // Functionality: Flow control in HTTP/2 manages how much data can be in flight on the network. Setting the initial connection window size to (1 << 31) - 1 (the maximum possible value for a 32-bit integer, which is 2,147,483,647 bytes) essentially allows the client to receive a very large amount of data from the server before needing to acknowledge receipt and permit more data to be sent. This can be particularly useful in high-latency networks or when transferring large amounts of data.
        // Impact: Increasing the window size can improve throughput by allowing more data to be in transit at a time, but it may also increase memory usage and can potentially lead to inefficient use of bandwidth if the network is unreliable.
        .initial_connection_window_size(Some((1 << 31) - 1))
        // Purpose: Configures whether the client should send keep-alive pings to the server when the connection is idle.
        // Functionality: When set to true, this option ensures that periodic pings are sent on an idle connection to keep it alive and detect if the server is still responsive.
        // Impact: This helps maintain active connections, particularly through NATs, load balancers, and other middleboxes that might drop idle connections. It helps ensure that the connection is promptly usable when new requests need to be sent.
        .keep_alive_while_idle(true)
        // Purpose: Sets the maximum amount of time the client will wait for a connection to be established.
        // Functionality: If a connection cannot be established within the specified duration, the attempt is aborted and an error is returned.
        // Impact: This setting prevents the client from waiting indefinitely for a connection to be established, which is crucial in scenarios where rapid failure detection is necessary to maintain responsiveness or to quickly fallback to alternative services or retry logic.
        .connect_timeout(Duration::from_secs(10))
        // Purpose: Configures the TCP keep-alive interval for the socket connection.
        // Functionality: This setting tells the operating system to send TCP keep-alive probes periodically when no data has been transferred over the connection within the specified interval.
        // Impact: Similar to the gRPC-level keep-alive, this helps keep the connection alive at the TCP layer and detect broken connections. It's particularly useful for detecting half-open connections and ensuring that resources are not wasted on unresponsive peers.
        .tcp_keepalive(Some(Duration::from_secs(15)))
        // Purpose: Sets a maximum duration for the client to wait for a response to a request.
        // Functionality: If a response is not received within the specified timeout, the request is canceled and an error is returned.
        // Impact: This is critical for bounding the wait time for operations, which can enhance the predictability and reliability of client interactions by avoiding indefinitely hanging requests.
        .timeout(Duration::from_secs(120))
        // Purpose: Specifies how long the client will wait for a response to a keep-alive ping before considering the connection dead.
        // Functionality: If a ping response is not received within this duration, the connection is presumed to be lost and is closed.
        // Impact: This setting is crucial for quickly detecting unresponsive connections and freeing up resources associated with them. It ensures that the client has up-to-date information on the status of connections and can react accordingly.
        .keep_alive_timeout(Duration::from_secs(25))
        .tls_config(ClientTlsConfig::new().with_enabled_roots())
        .map_err(|e| Error::new(ErrorKind::SetupTLSConfigError).with(e))?
        .connect()
        .await
        .map_err(|e| Error::new(ErrorKind::SetupConnectionError).with(e))?;

    Ok(channel)
}

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

pub struct Subscription {
    pending: Arc<Mutex<Vec<Envelope>>>,
    close_sender: Option<oneshot::Sender<()>>,
    closed: Arc<AtomicBool>,
}

impl Subscription {
    pub async fn start(stream: Streaming<Envelope>) -> Self {
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_clone = pending.clone();
        let (close_sender, close_receiver) = oneshot::channel::<()>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_clone = closed.clone();
        tokio::spawn(async move {
            let mut stream = Box::pin(stream);
            let mut close_receiver = Box::pin(close_receiver);

            loop {
                tokio::select! {
                    item = stream.message() => {
                        match item {
                            Ok(Some(envelope)) => {
                                let mut pending = pending_clone.lock().unwrap();
                                pending.push(envelope);
                            }
                            _ => break,
                        }
                    },
                    _ = &mut close_receiver => {
                        break;
                    }
                }
            }

            closed_clone.store(true, Ordering::SeqCst);
        });

        Subscription {
            pending,
            closed,
            close_sender: Some(close_sender),
        }
    }
}

impl XmtpApiSubscription for Subscription {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn get_messages(&self) -> Vec<Envelope> {
        let mut pending = self.pending.lock().unwrap();
        let items = pending.drain(..).collect::<Vec<Envelope>>();
        items
    }

    fn close_stream(&mut self) {
        // Set this value here, even if it will be eventually set again when the loop exits
        // This makes the `closed` status immediately correct
        self.closed.store(true, Ordering::SeqCst);
        if let Some(close_tx) = self.close_sender.take() {
            let _ = close_tx.send(());
        }
    }
}

type EnvelopeStream = Pin<Box<dyn Stream<Item = Result<Envelope, Error>> + Send>>;

pub struct GrpcMutableSubscription {
    envelope_stream: Abortable<EnvelopeStream>,
    update_channel: futures::channel::mpsc::UnboundedSender<SubscribeRequest>,
    abort_handle: AbortHandle,
}

impl GrpcMutableSubscription {
    pub fn new(
        envelope_stream: EnvelopeStream,
        update_channel: futures::channel::mpsc::UnboundedSender<SubscribeRequest>,
    ) -> Self {
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        Self {
            envelope_stream: Abortable::new(envelope_stream, abort_registration),
            update_channel,
            abort_handle,
        }
    }
}

impl Stream for GrpcMutableSubscription {
    type Item = Result<Envelope, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.envelope_stream.poll_next_unpin(cx)
    }
}

#[async_trait::async_trait]
impl MutableApiSubscription for GrpcMutableSubscription {
    async fn update(&mut self, req: SubscribeRequest) -> Result<(), Error> {
        self.update_channel
            .send(req)
            .await
            .map_err(|_| Error::new(ErrorKind::SubscriptionUpdateError))?;

        Ok(())
    }

    fn close(&self) {
        self.abort_handle.abort();
        self.update_channel.close_channel();
    }
}

#[async_trait::async_trait]
impl XmtpMlsClient for ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(&self, req: UploadKeyPackageRequest) -> Result<(), Error> {
        let client = &mut self.payer_client.clone();
        let res = client
            .publish_client_envelopes(PublishClientEnvelopesRequest::try_from(req)?)
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
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
        let key_packages = envelopes
            .iter()
            .map(|envelopes| {
                // The last envelope should be the newest key package upload
                let unsigned = envelopes.last().unwrap();
                let client_env = extract_client_envelope(unsigned);
                if let Some(Payload::UploadKeyPackage(upload_key_package)) = client_env.payload {
                    fetch_key_packages_response::KeyPackage {
                        key_package_tls_serialized: upload_key_package
                            .key_package
                            .unwrap()
                            .key_package_tls_serialized,
                    }
                } else {
                    panic!("Payload is not a key package");
                }
            })
            .collect();

        Ok(FetchKeyPackagesResponse { key_packages })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(&self, req: SendGroupMessagesRequest) -> Result<(), Error> {
        let client = &mut self.payer_client.clone();
        for message in req.messages {
            let res = client
                .publish_client_envelopes(PublishClientEnvelopesRequest::try_from(message)?)
                .await;
            if let Err(e) = res {
                return Err(Error::new(ErrorKind::MlsError).with(e));
            }
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(&self, req: SendWelcomeMessagesRequest) -> Result<(), Error> {
        let client = &mut self.payer_client.clone();
        for message in req.messages {
            let res = client
                .publish_client_envelopes(PublishClientEnvelopesRequest::try_from(message)?)
                .await;
            if let Err(e) = res {
                return Err(Error::new(ErrorKind::MlsError).with(e));
            }
        }
        Ok(())
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
                        extract_unsigned_originator_envelope(envelope);
                    let client_envelope = extract_client_envelope(envelope);
                    let Payload::GroupMessage(group_message) = client_envelope.payload.unwrap()
                    else {
                        panic!("Payload is not a group message");
                    };

                    let group_message_input::Version::V1(v1_group_message) =
                        group_message.version.unwrap();

                    GroupMessage {
                        version: Some(group_message::Version::V1(group_message::V1 {
                            id: unsigned_originator_envelope.originator_sequence_id,
                            created_ns: unsigned_originator_envelope.originator_ns as u64,
                            group_id: req.group_id.clone(),
                            data: v1_group_message.data,
                            sender_hmac: v1_group_message.sender_hmac,
                        })),
                    }
                })
                .collect(),
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
                        extract_unsigned_originator_envelope(envelope);
                    let client_envelope = extract_client_envelope(envelope);
                    let Payload::WelcomeMessage(welcome_message) = client_envelope.payload.unwrap()
                    else {
                        panic!("Payload is not a group message");
                    };
                    let welcome_message_input::Version::V1(v1_welcome_message) =
                        welcome_message.version.unwrap();

                    WelcomeMessage {
                        version: Some(welcome_message::Version::V1(welcome_message::V1 {
                            id: unsigned_originator_envelope.originator_sequence_id,
                            created_ns: unsigned_originator_envelope.originator_ns as u64,
                            installation_key: req.installation_key.clone(),
                            data: v1_welcome_message.data,
                            hpke_public_key: v1_welcome_message.hpke_public_key,
                        })),
                    }
                })
                .collect(),
            paging_info: None,
        };
        Ok(response)
    }
}

pub struct GroupMessageStream {
    inner: tonic::codec::Streaming<GroupMessage>,
}

impl From<tonic::codec::Streaming<GroupMessage>> for GroupMessageStream {
    fn from(inner: tonic::codec::Streaming<GroupMessage>) -> Self {
        GroupMessageStream { inner }
    }
}

impl Stream for GroupMessageStream {
    type Item = Result<GroupMessage, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner
            .poll_next_unpin(cx)
            .map(|data| data.map(|v| v.map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))))
    }
}

pub struct WelcomeMessageStream {
    inner: tonic::codec::Streaming<WelcomeMessage>,
}

impl From<tonic::codec::Streaming<WelcomeMessage>> for WelcomeMessageStream {
    fn from(inner: tonic::codec::Streaming<WelcomeMessage>) -> Self {
        WelcomeMessageStream { inner }
    }
}

impl Stream for WelcomeMessageStream {
    type Item = Result<WelcomeMessage, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner
            .poll_next_unpin(cx)
            .map(|data| data.map(|v| v.map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))))
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
        let client = &mut self.payer_client.clone();
        let res = client
            .publish_client_envelopes(PublishClientEnvelopesRequest::try_from(request)?)
            .await;
        match res {
            Ok(_) => Ok(PublishIdentityUpdateResponse {}),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
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
                        topics: topics.clone(),
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
