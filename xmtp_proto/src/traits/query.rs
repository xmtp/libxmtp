//! Implementation of the Query trait for all Endpoints

use super::{Client, Endpoint, Query, QueryStream};
use crate::{api::XmtpStream, prelude::ApiClientError, ApiEndpoint};

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<T, C> for E
where
    E: Endpoint<Output = T> + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + 'static,
{
    async fn query(&mut self, client: &C) -> Result<T, ApiClientError<C::Error>> {
        let request = http::Request::builder();
        let endpoint = self.grpc_endpoint();
        let path = http::uri::PathAndQuery::try_from(endpoint.as_ref())?;
        let rsp = client
            .request(request, path, self.body()?)
            .await
            .map_err(|e| e.endpoint(endpoint.into_owned()))?;
        let value: T = prost::Message::decode(rsp.into_body())?;
        Ok(value)
    }
}

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> QueryStream<T, C> for E
where
    E: Endpoint<Output = T> + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + 'static,
{
    async fn stream(
        &mut self,
        client: &C,
    ) -> Result<XmtpStream<<C as Client>::Stream, T>, ApiClientError<C::Error>> {
        let request = http::Request::builder();
        let endpoint = self.grpc_endpoint();
        let path = http::uri::PathAndQuery::try_from(endpoint.as_ref())?;
        let rsp = client
            .stream(request, path, self.body()?)
            .await
            .map_err(|e| e.endpoint(endpoint.into_owned()))?;
        let stream = rsp.into_body();
        let stream = XmtpStream::new(stream, ApiEndpoint::SubscribeGroupMessages);
        Ok(stream)
    }
}
