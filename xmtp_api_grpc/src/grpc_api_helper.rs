use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex}; // TODO switch to async mutexes
use std::time::Duration;

use futures::stream::{AbortHandle, Abortable};
use futures::{SinkExt, Stream, StreamExt, TryStreamExt};
use tokio::sync::oneshot;
use tonic::transport::ClientTlsConfig;
use tonic::{async_trait, metadata::MetadataValue, transport::Channel, Request, Streaming};

use xmtp_proto::{
    api_client::{
        Error, ErrorKind, GroupMessageStream, MutableApiSubscription, WelcomeMessageStream,
        XmtpApiClient, XmtpApiSubscription, XmtpMlsClient,
    },
    xmtp::identity::api::v1::identity_api_client::IdentityApiClient as ProtoIdentityApiClient,
    xmtp::message_api::v1::{
        message_api_client::MessageApiClient, BatchQueryRequest, BatchQueryResponse, Envelope,
        PublishRequest, PublishResponse, QueryRequest, QueryResponse, SubscribeRequest,
    },
    xmtp::mls::api::v1::{
        mls_api_client::MlsApiClient as ProtoMlsApiClient, FetchKeyPackagesRequest,
        FetchKeyPackagesResponse, GetIdentityUpdatesRequest, GetIdentityUpdatesResponse,
        QueryGroupMessagesRequest, QueryGroupMessagesResponse, QueryWelcomeMessagesRequest,
        QueryWelcomeMessagesResponse, RegisterInstallationRequest, RegisterInstallationResponse,
        SendGroupMessagesRequest, SendWelcomeMessagesRequest, SubscribeGroupMessagesRequest,
        SubscribeWelcomeMessagesRequest, UploadKeyPackageRequest,
    },
};

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
        .tls_config(ClientTlsConfig::new())
        .map_err(|e| Error::new(ErrorKind::SetupTLSConfigError).with(e))?
        .connect()
        .await
        .map_err(|e| Error::new(ErrorKind::SetupConnectionError).with(e))?;

    Ok(channel)
}

#[derive(Debug)]
pub struct Client {
    pub(crate) client: MessageApiClient<Channel>,
    pub(crate) mls_client: ProtoMlsApiClient<Channel>,
    pub(crate) identity_client: ProtoIdentityApiClient<Channel>,
    pub(crate) app_version: MetadataValue<tonic::metadata::Ascii>,
}

impl Client {
    pub async fn create(host: String, is_secure: bool) -> Result<Self, Error> {
        let host = host.to_string();
        let app_version = MetadataValue::try_from(&String::from("0.0.0")).unwrap();
        if is_secure {
            let channel = create_tls_channel(host).await?;

            let client = MessageApiClient::new(channel.clone());
            let mls_client = ProtoMlsApiClient::new(channel.clone());
            let identity_client = ProtoIdentityApiClient::new(channel);

            Ok(Self {
                client,
                mls_client,
                app_version,
                identity_client,
            })
        } else {
            let channel = Channel::from_shared(host)
                .map_err(|e| Error::new(ErrorKind::SetupCreateChannelError).with(e))?
                .connect()
                .await
                .map_err(|e| Error::new(ErrorKind::SetupConnectionError).with(e))?;

            let client = MessageApiClient::new(channel.clone());
            let mls_client = ProtoMlsApiClient::new(channel.clone());
            let identity_client = ProtoIdentityApiClient::new(channel);

            Ok(Self {
                client,
                mls_client,
                identity_client,
                app_version,
            })
        }
    }
}

#[async_trait]
impl XmtpApiClient for Client {
    type Subscription = Subscription;
    type MutableSubscription = GrpcMutableSubscription;

    fn set_app_version(&mut self, version: String) {
        self.app_version = MetadataValue::try_from(&version).unwrap();
    }

    async fn publish(
        &self,
        token: String,
        request: PublishRequest,
    ) -> Result<PublishResponse, Error> {
        let auth_token_string = format!("Bearer {}", token);
        let token: MetadataValue<_> = auth_token_string
            .parse()
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))?;

        let mut tonic_request = Request::new(request);
        tonic_request.metadata_mut().insert("authorization", token);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        let client = &mut self.client.clone();

        client
            .publish(tonic_request)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| Error::new(ErrorKind::PublishError).with(e))
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Subscription, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());

        let client = &mut self.client.clone();
        let stream = client
            .subscribe(tonic_request)
            .await
            .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
            .into_inner();

        Ok(Subscription::start(stream).await)
    }

    async fn subscribe2(
        &self,
        request: SubscribeRequest,
    ) -> Result<GrpcMutableSubscription, Error> {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<SubscribeRequest>();

        let input_stream = async_stream::stream! {
            yield request;
            // Wait for the receiver to send a new request.
            // This happens in the update method of the Subscription
            while let Some(result) = receiver.next().await {
                yield result;
            }
        };

        let mut tonic_request = Request::new(input_stream);

        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());

        let client = &mut self.client.clone();

        let stream = client
            .subscribe2(tonic_request)
            .await
            .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
            .into_inner();

        Ok(GrpcMutableSubscription::new(
            Box::pin(stream.map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))),
            sender,
        ))
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());
        let client = &mut self.client.clone();

        let res = client.query(tonic_request).await;

        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::QueryError).with(e)),
        }
    }

    async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());

        let client = &mut self.client.clone();
        let res = client.batch_query(tonic_request).await;

        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::QueryError).with(e)),
        }
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

#[async_trait]
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

#[async_trait]
impl XmtpMlsClient for Client {
    async fn register_installation(
        &self,
        req: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, Error> {
        let client = &mut self.mls_client.clone();
        let res = client.register_installation(req).await;
        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn upload_key_package(&self, req: UploadKeyPackageRequest) -> Result<(), Error> {
        let client = &mut self.mls_client.clone();
        let res = client.upload_key_package(req).await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn fetch_key_packages(
        &self,
        req: FetchKeyPackagesRequest,
    ) -> Result<FetchKeyPackagesResponse, Error> {
        let client = &mut self.mls_client.clone();
        let res = client.fetch_key_packages(req).await;

        res.map(|r| r.into_inner())
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
    }

    async fn send_group_messages(&self, req: SendGroupMessagesRequest) -> Result<(), Error> {
        let client = &mut self.mls_client.clone();
        let res = client.send_group_messages(req).await;

        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn send_welcome_messages(&self, req: SendWelcomeMessagesRequest) -> Result<(), Error> {
        let client = &mut self.mls_client.clone();
        let res = client.send_welcome_messages(req).await;

        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn query_group_messages(
        &self,
        req: QueryGroupMessagesRequest,
    ) -> Result<QueryGroupMessagesResponse, Error> {
        let client = &mut self.mls_client.clone();
        let res = client.query_group_messages(req).await;

        res.map(|r| r.into_inner())
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
    }

    async fn query_welcome_messages(
        &self,
        req: QueryWelcomeMessagesRequest,
    ) -> Result<QueryWelcomeMessagesResponse, Error> {
        let client = &mut self.mls_client.clone();
        let res = client.query_welcome_messages(req).await;

        res.map(|r| r.into_inner())
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
    }

    async fn get_identity_updates(
        &self,
        req: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Error> {
        let client = &mut self.mls_client.clone();
        let res = client.get_identity_updates(req).await;

        res.map(|r| r.into_inner())
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))
    }

    async fn subscribe_group_messages(
        &self,
        req: SubscribeGroupMessagesRequest,
    ) -> Result<GroupMessageStream, Error> {
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_group_messages(req)
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        let stream = res.into_inner();

        let new_stream = stream.map_err(|e| Error::new(ErrorKind::SubscribeError).with(e));

        Ok(Box::pin(new_stream))
    }

    async fn subscribe_welcome_messages(
        &self,
        req: SubscribeWelcomeMessagesRequest,
    ) -> Result<WelcomeMessageStream, Error> {
        let client = &mut self.mls_client.clone();
        let res = client
            .subscribe_welcome_messages(req)
            .await
            .map_err(|e| Error::new(ErrorKind::MlsError).with(e))?;

        let stream = res.into_inner();

        let new_stream = stream.map_err(|e| Error::new(ErrorKind::SubscribeError).with(e));

        Ok(Box::pin(new_stream))
    }
}
