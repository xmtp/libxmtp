use std::pin::Pin;
use std::sync::{Arc, Mutex}; // TODO switch to async mutexes
use std::{
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use futures::stream::{AbortHandle, Abortable};
use futures::{SinkExt, Stream, StreamExt, TryStreamExt};
use http_body::combinators::UnsyncBoxBody;
use hyper::{client::HttpConnector, Uri};
use hyper_rustls::HttpsConnector;
use tokio::sync::oneshot;
use tokio_rustls::rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore};
use tonic::{async_trait, metadata::MetadataValue, transport::Channel, Request, Status, Streaming};
use xmtp_proto::{
    api_client::{
        Error, ErrorKind, MutableApiSubscription, XmtpApiClient, XmtpApiSubscription, XmtpMlsClient,
    },
    xmtp::message_api::{
        v1::{
            message_api_client::MessageApiClient, BatchQueryRequest, BatchQueryResponse, Envelope,
            PublishRequest, PublishResponse, QueryRequest, QueryResponse, SubscribeRequest,
        },
        v3::{
            mls_api_client::MlsApiClient as ProtoMlsApiClient, ConsumeKeyPackagesRequest,
            ConsumeKeyPackagesResponse, GetIdentityUpdatesRequest, GetIdentityUpdatesResponse,
            PublishToGroupRequest, PublishWelcomesRequest, RegisterInstallationRequest,
            RegisterInstallationResponse, UploadKeyPackagesRequest,
        },
    },
};

fn tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    // Need to convert into OwnedTrustAnchor
    roots.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn get_tls_connector() -> HttpsConnector<HttpConnector> {
    let tls = tls_config();

    let mut http = HttpConnector::new();
    http.enforce_http(false);
    tower::ServiceBuilder::new()
        .layer_fn(move |s| {
            let tls = tls.clone();
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls)
                .https_or_http()
                .enable_http2()
                .wrap_connector(s)
        })
        .service(http)
}

#[derive(Debug)]
pub enum InnerApiClient {
    Plain(MessageApiClient<Channel>),
    Tls(
        MessageApiClient<
            hyper::Client<HttpsConnector<HttpConnector>, UnsyncBoxBody<hyper::body::Bytes, Status>>,
        >,
    ),
}

#[derive(Debug)]
pub enum InnerMlsClient {
    Plain(ProtoMlsApiClient<Channel>),
    Tls(
        ProtoMlsApiClient<
            hyper::Client<HttpsConnector<HttpConnector>, UnsyncBoxBody<hyper::body::Bytes, Status>>,
        >,
    ),
}

#[derive(Debug)]
pub struct Client {
    client: InnerApiClient,
    mls_client: InnerMlsClient,
    app_version: MetadataValue<tonic::metadata::Ascii>,
}

impl Client {
    pub async fn create(host: String, is_secure: bool) -> Result<Self, Error> {
        let host = host.to_string();
        let app_version = MetadataValue::try_from(&String::from("0.0.0")).unwrap();
        if is_secure {
            let connector = get_tls_connector();

            let tls_conn = hyper::Client::builder().build(connector);

            let uri =
                Uri::from_str(&host).map_err(|e| Error::new(ErrorKind::SetupError).with(e))?;

            let tls_client = MessageApiClient::with_origin(tls_conn.clone(), uri.clone());
            let mls_client = ProtoMlsApiClient::with_origin(tls_conn, uri);

            Ok(Self {
                client: InnerApiClient::Tls(tls_client),
                mls_client: InnerMlsClient::Tls(mls_client),
                app_version,
            })
        } else {
            let channel = Channel::from_shared(host)
                .map_err(|e| Error::new(ErrorKind::SetupError).with(e))?
                .connect()
                .await
                .map_err(|e| Error::new(ErrorKind::SetupError).with(e))?;

            let client = MessageApiClient::new(channel.clone());
            let mls_client = ProtoMlsApiClient::new(channel);

            Ok(Self {
                client: InnerApiClient::Plain(client),
                mls_client: InnerMlsClient::Plain(mls_client),
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

        match &self.client {
            InnerApiClient::Plain(c) => c
                .clone()
                .publish(tonic_request)
                .await
                .map(|r| r.into_inner())
                .map_err(|e| Error::new(ErrorKind::PublishError).with(e)),
            InnerApiClient::Tls(c) => c
                .clone()
                .publish(tonic_request)
                .await
                .map(|r| r.into_inner())
                .map_err(|e| Error::new(ErrorKind::PublishError).with(e)),
        }
    }

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Subscription, Error> {
        let mut tonic_request = Request::new(request);
        tonic_request
            .metadata_mut()
            .insert("x-app-version", self.app_version.clone());

        let stream = match &self.client {
            InnerApiClient::Plain(c) => c
                .clone()
                .subscribe(tonic_request)
                .await
                .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
                .into_inner(),
            InnerApiClient::Tls(c) => c
                .clone()
                .subscribe(tonic_request)
                .await
                .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
                .into_inner(),
        };

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

        let stream = match &self.client {
            InnerApiClient::Plain(c) => c
                .clone()
                .subscribe2(tonic_request)
                .await
                .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
                .into_inner(),
            InnerApiClient::Tls(c) => c
                .clone()
                .subscribe2(tonic_request)
                .await
                .map_err(|e| Error::new(ErrorKind::SubscribeError).with(e))?
                .into_inner(),
        };
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

        let res = match &self.client {
            InnerApiClient::Plain(c) => c.clone().query(tonic_request).await,
            InnerApiClient::Tls(c) => c.clone().query(tonic_request).await,
        };
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

        let res = match &self.client {
            InnerApiClient::Plain(c) => c.clone().batch_query(tonic_request).await,
            InnerApiClient::Tls(c) => c.clone().batch_query(tonic_request).await,
        };
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
    fn new(
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
    type Subscription = GrpcMutableSubscription;

    async fn register_installation(
        &self,
        req: RegisterInstallationRequest,
    ) -> Result<RegisterInstallationResponse, Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().register_installation(req).await,
            InnerMlsClient::Tls(c) => c.clone().register_installation(req).await,
        };
        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn upload_key_packages(&self, req: UploadKeyPackagesRequest) -> Result<(), Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().upload_key_packages(req).await,
            InnerMlsClient::Tls(c) => c.clone().upload_key_packages(req).await,
        };
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn consume_key_packages(
        &self,
        req: ConsumeKeyPackagesRequest,
    ) -> Result<ConsumeKeyPackagesResponse, Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().consume_key_packages(req).await,
            InnerMlsClient::Tls(c) => c.clone().consume_key_packages(req).await,
        };

        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn publish_to_group(&self, req: PublishToGroupRequest) -> Result<(), Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().publish_to_group(req).await,
            InnerMlsClient::Tls(c) => c.clone().publish_to_group(req).await,
        };
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn publish_welcomes(&self, req: PublishWelcomesRequest) -> Result<(), Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().publish_welcomes(req).await,
            InnerMlsClient::Tls(c) => c.clone().publish_welcomes(req).await,
        };
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }

    async fn get_identity_updates(
        &self,
        req: GetIdentityUpdatesRequest,
    ) -> Result<GetIdentityUpdatesResponse, Error> {
        let res = match &self.mls_client {
            InnerMlsClient::Plain(c) => c.clone().get_identity_updates(req).await,
            InnerMlsClient::Tls(c) => c.clone().get_identity_updates(req).await,
        };
        match res {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(Error::new(ErrorKind::MlsError).with(e)),
        }
    }
}
