//! Implementation of the Query trait for all Endpoints

use bytes::Bytes;

use super::{Client, Endpoint, Query, QueryStream};
use crate::{
    ApiEndpoint,
    api::{QueryRaw, XmtpBufferedStream, XmtpStream},
    prelude::ApiClientError,
};

pub(super) async fn request<C: Client + Send + Sync>(
    client: &C,
    endpoint: &mut impl Endpoint,
) -> Result<http::Response<Bytes>, ApiClientError<C::Error>> {
    let request = http::Request::builder();
    let endpoint_url = endpoint.grpc_endpoint();
    let path = http::uri::PathAndQuery::try_from(endpoint_url.as_ref())?;
    client
        .request(request, path, endpoint.body()?)
        .await
        .map_err(|e| e.endpoint(endpoint_url.into_owned()))
}

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<Q, C> Query<C> for Q
where
    Q: QueryRaw<C> + Endpoint + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    <Q as Endpoint>::Output: Default + prost::Message + 'static,
{
    type Output = <Q as Endpoint>::Output;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>> {
        let rsp = request(client, self).await?;
        let value = prost::Message::decode(rsp.into_body())?;
        Ok(value)
    }
}

// blanket QueryRaw implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C> QueryRaw<C> for E
where
    E: Endpoint + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
{
    async fn query_raw(&mut self, client: &C) -> Result<bytes::Bytes, ApiClientError<C::Error>> {
        let rsp = request(client, self).await?;
        Ok(rsp.into_body())
    }
}

// blanket Query implementation for a bare Endpoint
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> QueryStream<T, C> for E
where
    E: Endpoint + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + 'static,
    <C as Client>::Stream: 'static,
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

    async fn buffered_stream(
        &mut self,
        client: &C,
    ) -> Result<XmtpBufferedStream<<C as Client>::Stream, T>, ApiClientError<C::Error>> {
        let stream = self.stream(client).await?;
        let buffered_stream = XmtpBufferedStream::new(stream);
        Ok(buffered_stream)
    }
}
