use bytes::Bytes;
use crate::GrpcError;
use futures::Stream;
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};
use tonic::metadata::{self, MetadataMap, MetadataValue};
use tonic_web_wasm_client::Client as WasmClient;
use xmtp_proto::client_traits::{ApiClientError, Client};
use xmtp_proto::codec::TransparentCodec;


pub struct GrpcWebClient {
    pub(super) inner: tonic::client::Grpc<WasmClient>,
    pub(super) app_version: MetadataValue<metadata::Ascii>,
    pub(super) libxmtp_version: MetadataValue<metadata::Ascii>,
}

/// this code is the same for web and grpc, and can be put into the xmtp_proto crate
/// for now a PoC of GrpcWebClient

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

impl GrpcWebClient {
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
    /*
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
    */
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
        Poll::Ready(item.map(|i| Ok(i.unwrap())))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for GrpcWebClient {
    type Error = GrpcError;
    type Stream = GrpcStream;
    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        // self.wait_for_ready().await?;
        let request = self.build_tonic_request(request, body)?;
        let client = &mut self.inner.clone();

        let codec = TransparentCodec::default();
        let response = client.unary(request, path, codec).await.unwrap();

        Ok(response.to_http())
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        // self.wait_for_ready().await?;
        let request = self.build_tonic_request(request, body)?;
        let client = &mut self.inner.clone();

        let codec = TransparentCodec::default();
        let response = client.server_streaming(request, path, codec).await.unwrap();
        Ok(response.to_http().map(Into::into))
    }
}
