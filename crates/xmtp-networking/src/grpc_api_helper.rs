use hyper::{client::HttpConnector, Uri};
use hyper_rustls::HttpsConnector;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio_rustls::rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore};
use tonic::{metadata::MetadataValue, transport::Channel, Request, Streaming};
use xmtp_proto::xmtp::message_api::v1::{
    message_api_client::MessageApiClient, Envelope, PagingInfo, PublishRequest, PublishResponse,
    QueryRequest, QueryResponse, SubscribeRequest,
};

fn tls_config() -> Result<ClientConfig, tonic::Status> {
    let mut roots = RootCertStore::empty();
    // Need to convert into OwnedTrustAnchor
    roots.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    let tls = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tls)
}

fn get_tls_connector() -> Result<HttpsConnector<HttpConnector>, tonic::Status> {
    let tls =
        tls_config().map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    let connector = tower::ServiceBuilder::new()
        .layer_fn(move |s| {
            let tls = tls.clone();
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls)
                .https_or_http()
                .enable_http2()
                .wrap_connector(s)
        })
        .service(http);
    Ok(connector)
}

async fn make_non_tls_client(host: String) -> Result<MessageApiClient<Channel>, tonic::Status> {
    let error_str_host = host.clone();
    let non_tls_client = MessageApiClient::connect(host).await.map_err(|e| {
        tonic::Status::new(
            tonic::Code::Internal,
            format!("Failed to connect to {}: {}", error_str_host, e),
        )
    })?;
    Ok(non_tls_client)
}

pub struct Client {
    client: Option<MessageApiClient<Channel>>,
    tls_client: Option<MessageApiClient<Channel>>,
    tls_conn: Option<hyper::Client<HttpsConnector<HttpConnector>>>,
    channel: Option<Channel>,
}

impl Client {
    pub async fn create(host: String, is_secure: bool) -> Result<Self, tonic::Status> {
        let host = host.to_string();
        if is_secure {
            let connector = get_tls_connector().map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Internal,
                    format!("Failed to create TLS connector: {}", e),
                )
            })?;

            let tls_conn = hyper::Client::builder().build(connector);

            let uri = Uri::from_str(&host)
                .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

            let tls_client = MessageApiClient::with_origin(tls_conn, uri);

            return Ok(Self {
                client: None,
                channel: None,
                tls_client: Some(tls_client),
                tls_conn: Some(tls_conn),
            });
        } else {
            let channel = Channel::from_shared(host)
                .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?
                .connect()
                .await
                .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

            let client = MessageApiClient::new(channel.clone());

            return Ok(Self {
                client: Some(client),
                channel: Some(channel),
                tls_client: None,
                tls_conn: None,
            });
        }
    }

    pub async fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
    ) -> Result<PublishResponse, tonic::Status> {
        let auth_token_string = format!("Bearer {}", token);
        let token: MetadataValue<_> = auth_token_string
            .parse()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, format!("{}", e)))?;

        if self.channel.is_some() {
            return MessageApiClient::with_interceptor(
                self.channel.unwrap().clone(),
                move |mut req: Request<()>| {
                    req.metadata_mut().insert("authorization", token.clone());
                    Ok(req)
                },
            )
            .publish(PublishRequest { envelopes })
            .await
            .map(|r| r.into_inner());
        } else {
            return MessageApiClient::with_interceptor(
                self.tls_conn.unwrap().clone(),
                move |mut req: Request<()>| {
                    req.metadata_mut().insert("authorization", token.clone());
                    Ok(req)
                },
            )
            .publish(PublishRequest { envelopes })
            .await
            .map(|r| r.into_inner());
        }
    }

    pub async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, tonic::Status> {
        let request = SubscribeRequest {
            content_topics: topics,
        };
        let stream = self.client.subscribe(request).await.unwrap().into_inner();

        return Ok(Subscription::start(stream).await);
    }

    pub async fn query(
        &mut self,
        topic: String,
        start_time: Option<u64>,
        end_time: Option<u64>,
        paging_info: Option<PagingInfo>,
    ) -> Result<QueryResponse, tonic::Status> {
        let request = QueryRequest {
            content_topics: vec![topic],
            start_time_ns: start_time.unwrap_or(0),
            end_time_ns: end_time.unwrap_or(0),
            paging_info,
        };

        match self.client.query(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(e),
        }
    }
}

pub struct Subscription {
    pending: Arc<Mutex<Vec<Envelope>>>,
    close_sender: Option<oneshot::Sender<()>>,
}

impl Subscription {
    pub async fn start(stream: Streaming<Envelope>) -> Self {
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_clone = pending.clone();
        let (close_sender, close_receiver) = oneshot::channel::<()>();
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
        });

        Subscription {
            pending,
            close_sender: Some(close_sender),
        }
    }

    pub fn get_messages(&self) -> Vec<Envelope> {
        let mut pending = self.pending.lock().unwrap();
        let items = pending.drain(..).collect::<Vec<Envelope>>();
        items
    }

    pub fn close_stream(&mut self) {
        if let Some(close_tx) = self.close_sender.take() {
            let _ = close_tx.send(());
        }
    }
}
