use futures::Stream;
use pin_project_lite::pin_project;
use prost::bytes::Bytes;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};
use tonic::{
    metadata::{self, MetadataMap, MetadataValue},
    transport::Channel,
};
use xmtp_proto::{
    api_client::ApiBuilder,
    codec::TransparentCodec,
    traits::{ApiClientError, Client},
};

use crate::{create_tls_channel, GrpcBuilderError, GrpcError};
use xmtp_configuration::GRPC_PAYLOAD_LIMIT;

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
        *response.version_mut() = http::version::Version::HTTP_2;
        *response.headers_mut() = metadata.into_headers();
        *response.extensions_mut() = extensions;
        response
    }
}

#[derive(Clone)]
pub struct GrpcClient {
    inner: tonic::client::Grpc<Channel>,
    app_version: MetadataValue<metadata::Ascii>,
    libxmtp_version: MetadataValue<metadata::Ascii>,
}

impl GrpcClient {
    /// Builds a tonic request from a body and a generic HTTP Request
    fn build_tonic_request(
        &self,
        request: http::request::Builder,
        body: Bytes,
    ) -> Result<tonic::Request<Bytes>, ApiClientError<crate::GrpcError>> {
        let request = request.body(body)?;
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

    async fn wait_for_ready(&self) -> Result<(), ApiClientError<crate::GrpcError>> {
        let client = &mut self.inner.clone();
        client
            .ready()
            .await
            .map_err(|e| {
                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {e}"))
            })
            .map_err(GrpcError::from)?;
        Ok(())
    }
}

pin_project! {
    /// A stream of bytes from a GRPC Network Source
    pub struct GrpcStream {
        #[pin] inner: tonic::Streaming<Bytes>
    }
}

impl From<tonic::Streaming<Bytes>> for GrpcStream {
    fn from(value: tonic::Streaming<Bytes>) -> GrpcStream {
        GrpcStream { inner: value }
    }
}

// just a more convenient way to map the stream type to
// something more customized to the trait, without playing around with getting the
// generics right on nested futures combinators.
impl Stream for GrpcStream {
    type Item = Result<Bytes, crate::GrpcError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let item = ready!(this.inner.poll_next(cx));
        Poll::Ready(item.map(|i| i.map_err(crate::GrpcError::from)))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for GrpcClient {
    type Error = crate::GrpcError;
    type Stream = GrpcStream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        self.wait_for_ready().await?;
        let request = self.build_tonic_request(request, body)?;
        let client = &mut self.inner.clone();

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
        self.wait_for_ready().await?;
        let request = self.build_tonic_request(request, body)?;
        let client = &mut self.inner.clone();

        let codec = TransparentCodec::default();
        let response = client
            .server_streaming(request, path, codec)
            .await
            .map_err(GrpcError::from)?;
        Ok(response.to_http().map(Into::into))
    }
}

impl GrpcClient {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    host: Option<String>,
    /// version of the app
    app_version: Option<MetadataValue<metadata::Ascii>>,
    /// Version of the libxmtp core library
    libxmtp_version: Option<MetadataValue<metadata::Ascii>>,
    /// Whether or not the channel should use TLS
    tls_channel: bool,
    /// Rate per minute
    limit: Option<u64>,
}

impl ApiBuilder for ClientBuilder {
    type Output = GrpcClient;
    type Error = GrpcBuilderError;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        self.libxmtp_version = Some(MetadataValue::try_from(&version)?);
        Ok(())
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
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

    async fn build(self) -> Result<Self::Output, Self::Error> {
        let host = self.host.ok_or(GrpcBuilderError::MissingHostUrl)?;
        tracing::info!("building GrpcClient with host {}", &host);
        let channel = match self.tls_channel {
            true => create_tls_channel(host, self.limit.unwrap_or(5000)).await?,
            false => {
                Channel::from_shared(host)?
                    .rate_limit(self.limit.unwrap_or(5000), Duration::from_secs(60))
                    .connect()
                    .await?
            }
        };

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

    fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use super::*;
    use xmtp_configuration::GrpcUrls;
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{api_client::XmtpTestClient, TestApiBuilder, ToxicProxies};

    impl XmtpTestClient for GrpcClient {
        type Builder = ClientBuilder;

        fn create_local() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host(GrpcUrls::NODE.into());
            client.set_tls(false);
            client
        }

        fn create_local_d14n() -> Self::Builder {
            let mut client = GrpcClient::builder();
            client.set_host(GrpcUrls::XMTPD.into());
            client.set_tls(false);
            client
        }

        fn create_local_payer() -> Self::Builder {
            let mut payer = GrpcClient::builder();
            payer.set_host(GrpcUrls::PAYER.into());
            payer.set_tls(false);
            payer
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
