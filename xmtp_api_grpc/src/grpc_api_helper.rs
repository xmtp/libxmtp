use futures::stream::BoxStream;
use futures_util::StreamExt;
use http_body::combinators::UnsyncBoxBody;
use hyper::{client::HttpConnector, Uri};
use hyper_rustls::HttpsConnector;
use std::str::FromStr;
use tokio_rustls::rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore};
use tonic::async_trait;
use tonic::Status;
use tonic::{metadata::MetadataValue, transport::Channel, Request};
use xmtp_proto::api_client::{Error, ErrorKind, XmtpApiClient};
use xmtp_proto::xmtp::message_api::v1::{
    message_api_client::MessageApiClient, BatchQueryRequest, BatchQueryResponse, Envelope,
    PublishRequest, PublishResponse, QueryRequest, QueryResponse, SubscribeRequest,
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

pub enum InnerApiClient {
    Plain(MessageApiClient<Channel>),
    Tls(
        MessageApiClient<
            hyper::Client<HttpsConnector<HttpConnector>, UnsyncBoxBody<hyper::body::Bytes, Status>>,
        >,
    ),
}

pub struct XmtpGrpcClient {
    client: InnerApiClient,
    app_version: MetadataValue<tonic::metadata::Ascii>,
}

impl XmtpGrpcClient {
    pub async fn create(host: String, is_secure: bool) -> Result<Self, Error> {
        let host = host.to_string();
        if is_secure {
            let connector = get_tls_connector();

            let tls_conn = hyper::Client::builder().build(connector);

            let uri =
                Uri::from_str(&host).map_err(|e| Error::new(ErrorKind::SetupError).with(e))?;

            let tls_client = MessageApiClient::with_origin(tls_conn, uri);

            Ok(Self {
                client: InnerApiClient::Tls(tls_client),
                app_version: MetadataValue::try_from(&String::from("0.0.0")).unwrap(),
            })
        } else {
            let channel = Channel::from_shared(host)
                .map_err(|e| Error::new(ErrorKind::SetupError).with(e))?
                .connect()
                .await
                .map_err(|e| Error::new(ErrorKind::SetupError).with(e))?;

            let client = MessageApiClient::new(channel);

            Ok(Self {
                client: InnerApiClient::Plain(client),
                app_version: MetadataValue::try_from(&String::from("0.0.0")).unwrap(),
            })
        }
    }
}

impl Default for XmtpGrpcClient {
    fn default() -> Self {
        //TODO: Remove once Default constraint lifted from clientBuilder
        unimplemented!()
    }
}

#[async_trait]
impl XmtpApiClient for XmtpGrpcClient {
    type Subscription = BoxStream<'static, Envelope>;

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

    async fn subscribe(&self, request: SubscribeRequest) -> Result<Self::Subscription, Error> {
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
        }
        // Discard any messages that we can't parse.
        // TODO: consider surfacing these in a log somewhere
        .filter_map(|r| async move { r.ok() })
        .boxed();
        Ok(stream)
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
