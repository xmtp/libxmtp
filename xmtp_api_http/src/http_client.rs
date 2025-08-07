use crate::{ErrorResponse, HttpClientError, XmtpHttpApiClient};
use bytes::Bytes;
use futures::TryStreamExt;
use xmtp_common::StreamWrapper;
use std::future::Future;
use xmtp_proto::traits::{ApiClientError, Client};

impl From<HttpClientError> for ApiClientError<HttpClientError> {
    fn from(value: HttpClientError) -> Self {
        ApiClientError::Client { source: value }
    }
}

impl XmtpHttpApiClient {
    fn build_request(&self, request: http::request::Builder, path: http::uri::PathAndQuery, body: Bytes) -> Result<impl Future<Output = Result<reqwest::Response, reqwest::Error>>, HttpClientError> {
        let host = http::uri::Builder::from(http::uri::Uri::try_from(self.host_url.clone())?);
        let uri = host.path_and_query(path).build()?;
        trace!("uri={uri}");
        let request = request.method("POST").uri(uri).body(body)?;
        trace!("request={:?}", request);
        Ok(self.http_client.execute(request.try_into()?))
    }

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, HttpClientError> {
        self.wait_for_ready().await;
        let response = self.build_request(request, path, body)?.await?;

        if !response.status().is_success() {
            return Err(HttpClientError::Grpc(ErrorResponse {
                code: response.status().as_u16() as usize,
                message: response.text().await.map_err(HttpClientError::from)?,
                details: vec![],
            }));
        }
        let mut parts = http::response::Builder::default()
            .status(response.status())
            .version(Default::default());
        for (key, value) in response.headers() {
            parts = parts.header(key, value);
        }
        let response = parts.body(response.bytes().await?);

        Ok(response?)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for XmtpHttpApiClient {
    type Error = HttpClientError;
    type Stream = StreamWrapper<'static, Result<Bytes, HttpClientError>>;
    async fn request(
        &self,
        request: http::request::Builder,
        uri: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        Ok(self.request(request, uri, body).await?)
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let response = self.build_request(request, path, body)?.await.map_err(HttpClientError::from)?;
        // normally we would be able to convert this response to an HTTP response and
        // then wrap the body in a stream.
        // Unfortunately 'wrap_stream' is missing from reqwest when compilied to webassembly.
        // Further, the support for ReadableStream in response bodies is lacking for browsers.
        // So, we craft the http response ourselves after cloning the response values of interest.
        // https://github.com/seanmonstar/reqwest/issues/2248
        if !response.status().is_success() {
            return Err(HttpClientError::Grpc(ErrorResponse {
                code: response.status().as_u16() as usize,
                message: response.text().await.map_err(HttpClientError::from)?,
                details: vec![],
            })).map_err(ApiClientError::from);
        }
        let mut parts = http::response::Builder::default()
            .status(response.status())
            .version(Default::default());
        for (key, value) in response.headers() {
            parts = parts.header(key, value);
        }
        let stream = response.bytes_stream().map_err(HttpClientError::from);
        let stream = StreamWrapper::new(stream);
        let response = parts.body(stream)?;
        Ok(response)
    }
}
