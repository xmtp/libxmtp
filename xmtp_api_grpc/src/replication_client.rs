use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
// TODO switch to async mutexes
use std::time::Duration;

use futures::stream::{AbortHandle, Abortable};
use futures::{SinkExt, Stream, StreamExt, TryStreamExt};
use tokio::sync::oneshot;
use tonic::transport::ClientTlsConfig;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Streaming};

use xmtp_proto::api_client::{ClientWithMetadata, XmtpIdentityClient, XmtpMlsStreams};
use xmtp_proto::xmtp::mls::api::v1::{GroupMessage, WelcomeMessage};
use xmtp_proto::xmtp::xmtpv4::message_api::replication_api_client::ReplicationApiClient;
use xmtp_proto::{
    api_client::{
        Error, ErrorKind, MutableApiSubscription, XmtpApiClient, XmtpApiSubscription, XmtpMlsClient,
    }
    ,
    xmtp::message_api::v1::{
        BatchQueryRequest, BatchQueryResponse, Envelope,
        PublishRequest, PublishResponse, QueryRequest, QueryResponse, SubscribeRequest,
    },
    xmtp::mls::api::v1::{
        FetchKeyPackagesRequest,
        FetchKeyPackagesResponse, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, SendGroupMessagesRequest,
        SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
        UploadKeyPackageRequest,
    },
    xmtp::identity::api::v1::{
        GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request,
        GetIdentityUpdatesResponse as GetIdentityUpdatesV2Response, GetInboxIdsRequest,
        GetInboxIdsResponse, PublishIdentityUpdateRequest, PublishIdentityUpdateResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
        get_inbox_ids_response,
    },
    xmtp::xmtpv4::message_api::{
        GetInboxIdsRequest as GetInboxIdsRequestV4,
        get_inbox_ids_request,
    }
};
use crate::Client;

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
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
    pub(crate) libxmtp_version: MetadataValue<tonic::metadata::Ascii>,
}

impl ClientV4 {
    pub async fn  create(host: String, is_secure: bool) -> Result<Self, Error> {
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

        let client = ReplicationApiClient::new(channel.clone());

        Ok(Self {
            client,
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
impl XmtpMlsClient for ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn upload_key_package(&self, req: UploadKeyPackageRequest) -> Result<(), Error> {
        unimplemented!();
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn fetch_key_packages(
        &self,
        req: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error> {
        unimplemented!();
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_group_messages(&self, req: SendGroupMessagesRequest) -> Result<(), Error> {
        unimplemented!();
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn send_welcome_messages(&self, req: SendWelcomeMessagesRequest) -> Result<(), Error> {
        unimplemented!();
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_group_messages(
        &self,
        req: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error> {
        unimplemented!();
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn query_welcome_messages(
        &self,
        req: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error> {
        unimplemented!();
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

impl XmtpIdentityClient for ClientV4 {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn publish_identity_update(
        &self,
        request: PublishIdentityUpdateRequest,
    ) -> Result<PublishIdentityUpdateResponse, Error> {
        unimplemented!()
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
                .map(
                    |r| get_inbox_ids_request::Request {
                        address: r.address,
                    },
                )
                .collect(),
        };

        let res = client.get_inbox_ids(self.build_request(req)).await;

        res.map(|response| response.into_inner())
            .map(|response| GetInboxIdsResponse {
                responses: response
                    .responses
                    .into_iter()
                    .map(|r| {
                        get_inbox_ids_response::Response {
                            address: r.address,
                            inbox_id: r.inbox_id,
                        }
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
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse, Error> {
        unimplemented!()
    }
}
