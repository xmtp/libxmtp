use crate::{HttpClientError, XmtpHttpApiClient};
use bytes::Bytes;
use http::Method;
use std::pin::Pin;
use xmtp_proto::traits::{ApiError, Client};

impl From<HttpClientError> for ApiError<HttpClientError> {
    fn from(value: HttpClientError) -> Self {
        ApiError::Client { source: value }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Client for XmtpHttpApiClient {
    type Error = HttpClientError;
    type Stream = Pin<Box<dyn futures::Stream<Item = Result<Bytes, HttpClientError>> + Send>>;
    async fn request(
        &self,
        request: http::request::Builder,
        body: Vec<u8>,
    ) -> Result<http::Response<Bytes>, ApiError<Self::Error>> {
        let request = request.body(body.clone())?;
        println!("## {:?}", request);
        let (parts, _) = request.into_parts();
        println!("## {:?}", parts);
        println!("## body {:?}", body);

        let url = format!("{}{}", self.host_url, parts.uri);
        let mut req = self.http_client.request(Method::POST, url);
        println!("## req {:?}", req);

        for (key, value) in parts.headers.iter() {
            req = req.header(key, value);
        }
        println!("## req {:?}", req);
        let response = req
            .body(body)
            .send()
            .await
            .map_err(HttpClientError::from)?
            .error_for_status()
            .map_err(HttpClientError::from)?;
        println!("## response {:?}", response);

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(HttpClientError::from)?;

        let mut http_response = http::Response::new(body);
        *http_response.status_mut() = status;
        *http_response.headers_mut() = headers;

        Ok(http_response)
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

#[cfg(test)]
pub mod tests {}
