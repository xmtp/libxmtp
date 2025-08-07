//! Implementation of the Query trait for all Endpoints

use super::{Client, Endpoint, Query};
use crate::prelude::ApiClientError;
use futures::{Stream, StreamExt, TryStreamExt};

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint<Output = T> + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + 'static,
{
    async fn query(&self, client: &C) -> Result<T, ApiClientError<C::Error>> {
        let mut request = http::Request::builder();
        let endpoint = if cfg!(any(feature = "http-api", target_arch = "wasm32")) {
            request = request.header("Content-Type", "application/x-protobuf");
            request = request.header("Accept", "application/x-protobuf");
            self.http_endpoint()
        } else {
            self.grpc_endpoint()
        };
        let path = http::uri::PathAndQuery::try_from(endpoint.as_ref())?;
        let rsp = client
            .request(request, path, self.body()?)
            .await
            .map_err(|e| e.endpoint(endpoint.into_owned()))?;
        let value: T = prost::Message::decode(rsp.into_body())?;
        Ok(value)
    }

    async fn stream(
        &self,
        client: &C,
    ) -> Result<impl Stream<Item = Result<T, ApiClientError<C::Error>>>, ApiClientError<C::Error>>
    {
        let mut request = http::Request::builder();
        let endpoint = if cfg!(any(feature = "http-api", target_arch = "wasm32")) {
            request = request.header("Content-Type", "application/x-protobuf");
            request = request.header("Accept", "application/x-protobuf");
            self.http_endpoint()
        } else {
            self.grpc_endpoint()
        };
        let path = http::uri::PathAndQuery::try_from(endpoint.as_ref())?;
        let rsp = client
            .stream(request, path, self.body()?)
            .await
            .map_err(|e| e.endpoint(endpoint.into_owned()))?;
        let stream = rsp.into_body();
        let stream = stream
            .map_err(|e| ApiClientError::Client { source: e })
            .map(|i| {
                let value: T = prost::Message::decode(i?)?;
                Ok(value)
            });
        Ok(stream)
    }
}
