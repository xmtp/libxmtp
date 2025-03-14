use crate::{ErrorResponse, HttpClientError, XmtpHttpApiClient};
use bytes::Bytes;
use http::Response;
use reqwest::Body;
use std::pin::Pin;
use xmtp_proto::traits::{ApiError, Client};

impl From<HttpClientError> for ApiError<HttpClientError> {
    fn from(value: HttpClientError) -> Self {
        ApiError::Client { source: value }
    }
}

impl XmtpHttpApiClient {
    async fn request<T>(
        &self,
        request: http::request::Builder,
        uri: http::uri::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, HttpClientError>
    where
        T: Default + prost::Message + 'static,
        Self: Sized,
    {
        let parts = http::uri::Uri::try_from(&self.host_url)?.into_parts();
        let uri = uri
            .scheme(parts.scheme.unwrap_or("http".try_into()?))
            .authority(parts.authority.unwrap_or("localhost".try_into()?))
            .build()?;
        trace!("uri={uri}");
        let request = request.method("POST").uri(uri).body(body)?;
        trace!("request={:?}", request);
        let response = self.http_client.execute(request.try_into()?).await?;

        if !response.status().is_success() {
            return Err(HttpClientError::Grpc(ErrorResponse {
                code: response.status().as_u16() as usize,
                message: response.text().await.map_err(HttpClientError::from)?,
                details: vec![],
            }));
        }
        let response: Response<Body> = response.into();
        let (parts, body) = response.into_parts();
        let body = http_body_util::BodyExt::collect(body)
            .await
            .map(|buf| T::decode(buf.to_bytes()))??;
        Ok(http::Response::from_parts(parts, body))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for XmtpHttpApiClient {
    type Error = HttpClientError;
    type Stream = Pin<Box<dyn futures::Stream<Item = Result<Bytes, HttpClientError>> + Send>>;
    async fn request<T>(
        &self,
        request: http::request::Builder,
        uri: http::uri::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, ApiError<Self::Error>>
    where
        T: Default + prost::Message + 'static,
        Self: Sized,
    {
        Ok(self.request(request, uri, body).await?)
    }

    async fn stream(
        &self,
        _request: http::request::Builder,
        _body: Vec<u8>,
    ) -> Result<http::Response<Self::Stream>, ApiError<Self::Error>> {
        // same as unary but server_streaming method
        todo!()
    }
}
