//! The generic gRPC Client
//! Generic over a inner "Channel".
//! The  inner channel must implement a tower service to implicitly
//! implement the gRPC Service

use crate::{
    error::{GrpcBuilderError, GrpcError},
    streams::{EscapableTonicStream, FakeEmptyStream, NonBlockingWebStream},
};
use futures::Stream;
use http::{StatusCode, request, uri::PathAndQuery};
use http_body_util::StreamBody;
use pin_project::pin_project;
use prost::bytes::Bytes;
use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};
use tonic::{
    Response, Status, Streaming,
    client::Grpc,
    codec::Codec,
    metadata::{self, MetadataMap, MetadataValue},
};
use xmtp_common::Retry;
use xmtp_configuration::GRPC_PAYLOAD_LIMIT;
use xmtp_proto::{
    api::{ApiClientError, Client, IsConnectedCheck},
    api_client::{ApiBuilder, NetConnectConfig},
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

    async fn wait_for_ready(&self, client: &mut Grpc<crate::GrpcService>) -> Result<(), Status> {
        client.ready().await.map_err(|e| {
            tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {e}"))
        })?;
        Ok(())
    }
}

#[pin_project]
/// A stream of bytes from a GRPC Network Source
pub struct GrpcStream {
    #[pin]
    inner: crate::streams::NonBlocking,
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

#[xmtp_common::async_trait]
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
        request: request::Builder,
        path: PathAndQuery,
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
        let req = crate::streams::NonBlockingStreamRequest::new(
            Box::pin(response) as crate::streams::ResponseFuture
        );
        let response = crate::streams::send(req).await.map_err(GrpcError::from)?;
        let response = response.map(|body| GrpcStream {
            inner: EscapableTonicStream::new(body),
        });
        Ok(response.to_http().map(Into::into))
    }

    // just need to ensure the type is the same as `stream`
    fn fake_stream(&self) -> http::Response<Self::Stream> {
        let mut codec = TransparentCodec::default();
        let s = StreamBody::new(FakeEmptyStream::<Status>::new());
        let response = Streaming::new_response(codec.decoder(), s, StatusCode::OK, None, None);
        let response = Response::new(response);
        let response = response.map(|body| GrpcStream {
            inner: EscapableTonicStream::new(NonBlockingWebStream::started(body)),
        });
        response.to_http()
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for GrpcClient {
    async fn is_connected(&self) -> bool {
        self.inner.clone().ready().await.is_ok()
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
    /// retry strategy for this client
    pub retry: Option<Retry>,
}

impl NetConnectConfig for ClientBuilder {
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

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        self.retry = Some(retry);
    }
}

impl ApiBuilder for ClientBuilder {
    type Output = crate::GrpcClient;
    type Error = GrpcBuilderError;

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
}

impl GrpcClient {
    pub fn create(host: &str, is_secure: bool) -> Result<Self, GrpcBuilderError> {
        let mut builder = Self::builder();
        builder.set_host(host.to_string());
        builder.set_tls(is_secure);
        builder.build()
    }

    /// Create a grpc client with `app_version` attached
    pub fn create_with_version(
        host: &str,
        is_secure: bool,
        app_version: AppVersion,
    ) -> Result<Self, GrpcBuilderError> {
        let mut builder = Self::builder();
        builder.set_host(host.to_string());
        builder.set_tls(is_secure);
        builder.set_app_version(app_version)?;
        builder.build()
    }
}

#[cfg(test)]
pub mod tests {
    use crate::grpc_client::test::DevNodeGoClient;
    use prost::Message;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::prelude::{NetConnectConfig, XmtpTestClient};
    use xmtp_proto::types::AppVersion;
    use xmtp_proto::xmtp::message_api::v1::PublishRequest;

    #[xmtp_common::test]
    async fn metadata_test() {
        let mut client = DevNodeGoClient::create();
        let app_version = AppVersion::from("test/1.0.0");
        let libxmtp_version = "0.0.1".to_string();
        client.set_app_version(app_version.clone()).unwrap();
        client.set_libxmtp_version(libxmtp_version.clone()).unwrap();
        let client = client.build().unwrap();
        let request = client
            .build_tonic_request(
                Default::default(),
                PublishRequest { envelopes: vec![] }.encode_to_vec().into(),
            )
            .unwrap();

        assert_eq!(
            request
                .metadata()
                .get("x-app-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            app_version
        );
        assert_eq!(
            request
                .metadata()
                .get("x-libxmtp-version")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            libxmtp_version
        );
    }
}
