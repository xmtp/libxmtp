//! The genneric gRPC Client
//! Generic over a inner "Channel".
//! The  inner channel must implement a tower service to implicitly
//! implement the gRPC Service

use crate::{
    GrpcService,
    error::{GrpcBuilderError, GrpcError},
    streams::EscapableTonicStream,
};
use futures::Stream;
use pin_project_lite::pin_project;
use prost::bytes::Bytes;
use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};
use tonic::{
    Status,
    client::Grpc,
    metadata::{self, MetadataMap, MetadataValue},
};
use xmtp_configuration::GRPC_PAYLOAD_LIMIT;
use xmtp_proto::{
    api::{ApiClientError, Client},
    api_client::ApiBuilder,
    codec::TransparentCodec,
    types::AppVersion,
};

impl From<GrpcError> for ApiClientError<GrpcError> {
    fn from(source: GrpcError) -> ApiClientError<GrpcError> {
        ApiClientError::Client { source }
    }
}

/// Private trait to convert type to an HTTP Response
trait ToHttp {
    type Body;
    fn to_http(self) -> http::Response<Self::Body>;
}

/// Convert a tonic Response to a generic HTTP response
impl<T> ToHttp for tonic::Response<T> {
    type Body = T;

    fn to_http(self) -> http::Response<Self::Body> {
        let (metadata, body, extensions) = self.into_parts();
        let mut response = http::Response::new(body);
        if cfg!(target_arch = "wasm32") {
            *response.version_mut() = http::version::Version::HTTP_11;
        } else {
            *response.version_mut() = http::version::Version::HTTP_2;
        }
        *response.headers_mut() = metadata.into_headers();
        *response.extensions_mut() = extensions;
        response
    }
}

#[derive(Clone)]
pub struct GrpcClient {
    inner: tonic::client::Grpc<crate::GrpcService>,
    app_version: MetadataValue<metadata::Ascii>,
    libxmtp_version: MetadataValue<metadata::Ascii>,
}

impl GrpcClient {
    pub fn new(
        service: crate::GrpcService,
        app_version: MetadataValue<metadata::Ascii>,
        libxmtp_version: MetadataValue<metadata::Ascii>,
    ) -> Self {
        Self {
            inner: tonic::client::Grpc::new(service),
            app_version,
            libxmtp_version,
        }
    }

    /// Builds a tonic request from a body and a generic HTTP Request
    fn build_tonic_request(
        &self,
        request: http::request::Builder,
        body: Bytes,
    ) -> Result<tonic::Request<Bytes>, Status> {
        let request = request
            .body(body)
            .map_err(|e| tonic::Status::from_error(Box::new(e)))?;
        let (parts, body) = request.into_parts();
        let mut tonic_request = tonic::Request::from_parts(
            MetadataMap::from_headers(parts.headers),
            parts.extensions,
            body,
        );
        let metadata = tonic_request.metadata_mut();
        // must be lowercase otherwise panics
        metadata.append("x-app-version", self.app_version.clone());
        metadata.append("x-libxmtp-version", self.libxmtp_version.clone());
        Ok(tonic_request)
    }

    async fn wait_for_ready(&self, client: &mut Grpc<GrpcService>) -> Result<(), Status> {
        client.ready().await.map_err(|e| {
            tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {e}"))
        })?;
        Ok(())
    }
}

pin_project! {
    /// A stream of bytes from a GRPC Network Source
    pub struct GrpcStream {
        #[pin] inner: crate::streams::NonBlocking
    }
}

impl From<crate::streams::NonBlocking> for GrpcStream {
    fn from(value: crate::streams::NonBlocking) -> GrpcStream {
        GrpcStream { inner: value }
    }
}

// just a more convenient way to map the stream type to
// something more customized to the trait, without playing around with getting the
// generics right on nested futures combinators.
impl Stream for GrpcStream {
    type Item = Result<Bytes, GrpcError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let item = ready!(this.inner.poll_next(cx));
        Poll::Ready(item.map(|i| i.map_err(GrpcError::from)))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for GrpcClient {
    type Error = GrpcError;
    type Stream = GrpcStream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        let client = &mut self.inner.clone();
        self.wait_for_ready(client).await.map_err(GrpcError::from)?;
        let request = self
            .build_tonic_request(request, body)
            .map_err(GrpcError::from)?;
        let codec = TransparentCodec::default();
        let response = client
            .unary(request, path, codec)
            .await
            .map_err(GrpcError::from)?;

        Ok(response.to_http())
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let this = self.clone();
        // client requires to be moved so it lives long enough for streaming response future.
        let response = async move {
            let mut client = this.inner.clone();
            this.wait_for_ready(&mut client).await?;
            let request = this.build_tonic_request(request, body)?;
            let codec = TransparentCodec::default();
            client.server_streaming(request, path, codec).await
        };
        let req = crate::streams::NonBlockingStreamRequest::new(Box::pin(response) as Pin<Box<_>>);
        let response = crate::streams::send(req).await.map_err(GrpcError::from)?;
        let response = response.map(|body| GrpcStream {
            inner: EscapableTonicStream::new(body),
        });
        Ok(response.to_http().map(Into::into))
    }
}

impl GrpcClient {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

#[derive(Default, Clone)]
pub struct ClientBuilder {
    pub host: Option<String>,
    /// version of the app
    pub app_version: Option<MetadataValue<metadata::Ascii>>,
    /// Version of the libxmtp core library
    pub libxmtp_version: Option<MetadataValue<metadata::Ascii>>,
    /// Whether or not the channel should use TLS
    pub tls_channel: bool,
    /// Rate per minute
    pub limit: Option<u64>,
}

impl ApiBuilder for ClientBuilder {
    type Output = GrpcClient;
    type Error = GrpcBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.libxmtp_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        self.app_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_host(&mut self, host: String) {
        self.host = Some(host);
    }

    fn set_tls(&mut self, tls: bool) {
        self.tls_channel = tls;
    }

    fn rate_per_minute(&mut self, limit: u32) {
        self.limit = Some(limit.into());
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        if let Some(h) = &self.host {
            let u = url::Url::parse(h)?;
            Ok(u.port().map(|u| u.to_string()))
        } else {
            Err(GrpcBuilderError::MissingHostUrl)
        }
    }

    fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.host.ok_or(GrpcBuilderError::MissingHostUrl)?;
        let channel = crate::GrpcService::new(host, self.limit, self.tls_channel)?;
        Ok(GrpcClient {
            inner: tonic::client::Grpc::new(channel)
                .max_decoding_message_size(GRPC_PAYLOAD_LIMIT)
                .max_encoding_message_size(GRPC_PAYLOAD_LIMIT),
            app_version: self
                .app_version
                .unwrap_or(MetadataValue::try_from("0.0.0")?),
            libxmtp_version: self.libxmtp_version.unwrap_or(MetadataValue::try_from(
                env!("CARGO_PKG_VERSION").to_string(),
            )?),
        })
    }

    // this client does not do retries
    fn set_retry(&mut self, _retry: xmtp_common::Retry) {}
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use super::*;
    use xmtp_configuration::GrpcUrls;
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{TestApiBuilder, ToxicProxies, api_client::XmtpTestClient};

    impl XmtpTestClient for GrpcClient {
        type Builder = ClientBuilder;

        fn create_local() -> Self::Builder {
            let mut client = GrpcClient::builder();
            let url = url::Url::parse(GrpcUrls::NODE).unwrap();
            match url.scheme() {
                "https" => client.set_tls(true),
                _ => client.set_tls(false),
            }
            client.set_host(GrpcUrls::NODE.into());
            client
        }

        fn create_d14n() -> Self::Builder {
            let mut client = GrpcClient::builder();
            let url = url::Url::parse(GrpcUrls::XMTPD).unwrap();
            match url.scheme() {
                "https" => client.set_tls(true),
                _ => client.set_tls(false),
            }
            client.set_host(GrpcUrls::XMTPD.into());
            client
        }

        fn create_gateway() -> Self::Builder {
            let mut gateway = GrpcClient::builder();
            let url = url::Url::parse(GrpcUrls::GATEWAY).unwrap();
            match url.scheme() {
                "https" => gateway.set_tls(true),
                _ => gateway.set_tls(false),
            }
            gateway.set_host(GrpcUrls::GATEWAY.into());
            gateway
        }

        fn create_dev() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host(GrpcUrls::NODE_DEV.into());
            client.set_tls(true);
            client
        }
    }

    impl TestApiBuilder for ClientBuilder {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            let proxy = xmtp_proto::init_toxi(&[self.host().unwrap()]).await;
            self.set_host(format!("{LOCALHOST}:{}", proxy.port(0)));
            proxy
        }
    }
}
