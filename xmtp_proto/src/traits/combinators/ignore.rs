use std::marker::PhantomData;

use crate::api::{ApiClientError, Client, Endpoint, Query, QueryRaw};

/// Concrete type of the [`ignore`] combinator
pub struct Ignore<E> {
    endpoint: E,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C> Query<C> for Ignore<E>
where
    E: QueryRaw<C>,
    C: Client,
{
    type Output = ();
    async fn query(&mut self, client: &C) -> Result<(), ApiClientError<C::Error>> {
        let _ = QueryRaw::<C>::query_raw(&mut self.endpoint, client).await?;
        // ignore response value
        Ok(())
    }
}

pub struct IgnoreSpecialized<S> {
    _marker: PhantomData<S>,
}

impl<S, E: Endpoint<S>> Endpoint<IgnoreSpecialized<S>> for Ignore<E> {
    type Output = <E as Endpoint<S>>::Output;

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.grpc_endpoint()
    }

    fn body(&self) -> Result<bytes::Bytes, crate::api::BodyError> {
        self.endpoint.body()
    }
}

//TODO: figure out how to skip deserialization of body
//would require a query that doesn't deserialize?
/// Ignore/drop the response data for this endpoint
/// does not ignore any errors that might have occurred as a result of
/// making a network request.
/// the response body still must be valid protobuf
pub fn ignore<E>(endpoint: E) -> Ignore<E> {
    Ignore { endpoint }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;
    use crate::api::{
        self, EndpointExt,
        mock::{MockError, MockNetworkClient, TestEndpoint},
    };
    use rstest::*;

    #[derive(prost::Message)]
    struct TestProto {
        #[prost(int64, tag = "1")]
        inner: i64,
    }

    #[fixture]
    fn client() -> MockNetworkClient {
        let mut client = MockNetworkClient::new();
        let bytes = TestProto { inner: 900 }.encode_to_vec();
        client.expect_request().times(3).returning(|_, _, _| {
            tracing::info!("error");
            Err(ApiClientError::Client {
                source: MockError::ARetryableError,
            })
        });
        client
            .expect_request()
            .times(1)
            .returning(move |_, _, _| Ok(http::Response::new(bytes.clone().into())));
        client
    }

    #[xmtp_common::test]
    async fn ignores_payloads() {
        let mut client = MockNetworkClient::new();
        let bytes = vec![0, 1, 2];
        client
            .expect_request()
            .times(1)
            .returning(move |_, _, _| Ok(http::Response::new(bytes.clone().into())));
        let result: Result<(), _> = TestEndpoint.ignore_response().query(&client).await;
        assert!(result.is_ok(), "{}", result.unwrap_err().to_string());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn ignore_is_retryable(client: MockNetworkClient) {
        let result: Result<(), _> = api::ignore(api::retry(TestEndpoint)).query(&client).await;
        assert!(result.is_ok(), "{}", result.unwrap_err().to_string());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn ignore_is_orthogonal(client: MockNetworkClient) {
        let result: Result<(), _> = api::retry(api::ignore(TestEndpoint)).query(&client).await;
        assert!(result.is_ok(), "{}", result.unwrap_err().to_string());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn endpoint_chains_work(client: MockNetworkClient) {
        let result: Result<(), _> = TestEndpoint.ignore_response().retry().query(&client).await;
        assert!(result.is_ok(), "{}", result.unwrap_err().to_string());
    }

    #[rstest]
    #[xmtp_common::test]
    async fn endpoint_chains_orthogonal(client: MockNetworkClient) {
        let result: Result<(), _> = TestEndpoint.retry().ignore_response().query(&client).await;
        assert!(result.is_ok(), "{}", result.unwrap_err().to_string());
    }

    #[xmtp_common::test]
    fn test_body_delegates_to_wrapped_endpoint() {
        let ignore_endpoint = ignore(TestEndpoint);
        let result = ignore_endpoint.body();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), bytes::Bytes::from(vec![]));
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_delegates_to_wrapped_endpoint() {
        let ignore_endpoint = ignore(TestEndpoint);
        assert_eq!(ignore_endpoint.grpc_endpoint(), "");
    }
}
