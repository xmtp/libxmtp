use crate::{ErrorResponse, HttpClientError, XmtpHttpApiClient};
use bytes::Bytes;
use std::pin::Pin;
use xmtp_proto::traits::{ApiClientError, Client};

impl From<HttpClientError> for ApiClientError<HttpClientError> {
    fn from(value: HttpClientError) -> Self {
        ApiClientError::Client { source: value }
    }
}

impl XmtpHttpApiClient {
    async fn request<T>(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, HttpClientError>
    where
        T: Default + prost::Message + 'static,
        Self: Sized,
    {
        let host = http::uri::Builder::from(http::uri::Uri::try_from(self.host_url.clone())?);
        let uri = host.path_and_query(path).build()?;
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
        let mut parts = http::response::Builder::default()
            .status(response.status())
            .version(Default::default());
        for (key, value) in response.headers() {
            parts = parts.header(key, value);
        }
        let response = parts.body(prost::Message::decode(response.bytes().await?)?);

        Ok(response?)
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
        uri: http::uri::PathAndQuery,
        body: Vec<u8>,
    ) -> Result<http::Response<T>, ApiClientError<Self::Error>>
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
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        // same as unary but server_streaming method
        todo!()
    }
}
