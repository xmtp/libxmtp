use std::marker::PhantomData;

use xmtp_common::{
    ExponentialBackoff, Retry, RetryableError, Strategy as RetryStrategy, retry_async,
};

use crate::api::{ApiClientError, Client, Endpoint, Pageable, Query, QueryRaw};

/// The concrete type of a [`crate::api::retry`] Combinators.
/// Generally using the concrete type can be avoided with type inference
/// or impl Trait.
pub struct RetryQuery<E, S = ExponentialBackoff> {
    endpoint: E,
    pub(crate) retry: Retry<S>,
}

impl<E> RetryQuery<E> {
    pub fn new(endpoint: E) -> Self {
        Self {
            endpoint,
            retry: Default::default(),
        }
    }
}

impl<E> Pageable for RetryQuery<E>
where
    E: Pageable,
{
    fn set_cursor(&mut self, cursor: u64) {
        self.endpoint.set_cursor(cursor)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C, S> Query<C> for RetryQuery<E, S>
where
    E: Query<C>,
    C: Client,
    C::Error: RetryableError,
    S: RetryStrategy + Send + Sync,
{
    type Output = E::Output;
    async fn query(&mut self, client: &C) -> Result<Self::Output, ApiClientError<C::Error>> {
        retry_async!(
            self.retry,
            (async { Query::<C>::query(&mut self.endpoint, client).await })
        )
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, C, S> QueryRaw<C> for RetryQuery<E, S>
where
    E: Endpoint,
    C: Client,
    C::Error: RetryableError,
    S: RetryStrategy + Send + Sync,
{
    async fn query_raw(&mut self, client: &C) -> Result<bytes::Bytes, ApiClientError<C::Error>> {
        retry_async!(
            self.retry,
            (async { QueryRaw::<C>::query_raw(&mut self.endpoint, client).await })
        )
    }
}

pub struct RetrySpecialized<Spec> {
    _marker: PhantomData<Spec>,
}

impl<E, Spec> Endpoint<RetrySpecialized<Spec>> for RetryQuery<E>
where
    E: Endpoint<Spec>,
    Spec: Send + Sync,
{
    type Output = <E as Endpoint<Spec>>::Output;

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.grpc_endpoint()
    }

    fn body(&self) -> Result<bytes::Bytes, crate::api::BodyError> {
        self.endpoint.body()
    }
}

/// retry with the default retry strategy (ExponentialBackoff)
pub fn retry<E>(endpoint: E) -> RetryQuery<E, ExponentialBackoff> {
    RetryQuery::<E, _> {
        endpoint,
        retry: Retry::default(),
    }
}

/// Retry the endpoint, indicating a specific strategy to retry with
pub fn retry_with_strategy<E, S>(endpoint: E, retry: Retry<S>) -> RetryQuery<E, S> {
    RetryQuery::<E, S> { endpoint, retry }
}

#[cfg(test)]
mod tests {

    use crate::api::{
        EndpointExt,
        mock::{MockError, MockNetworkClient, TestEndpoint},
    };

    use super::*;

    #[xmtp_common::test]
    async fn retries_endpoint_three_times() {
        let mut client = MockNetworkClient::new();
        client.expect_request().times(3).returning(|_, _, _| {
            tracing::info!("error");
            Err(ApiClientError::Client {
                source: MockError::ARetryableError,
            })
        });
        client
            .expect_request()
            .times(1)
            .returning(|_, _, _| Ok(http::Response::new(vec![].into())));

        let result: Result<(), _> = retry(TestEndpoint).query(&client).await;
        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    async fn does_not_retry_non_retryable() {
        let mut client = MockNetworkClient::new();
        client.expect_request().times(1).returning(|_, _, _| {
            Err(ApiClientError::Client {
                source: MockError::ANonRetryableError,
            })
        });

        let result: Result<(), _> = retry(TestEndpoint).query(&client).await;
        assert!(result.is_err());
        assert!(
            matches!(
                result,
                Err(ApiClientError::ClientWithEndpoint {
                    source: MockError::ANonRetryableError,
                    ..
                })
            ),
            "{:?}",
            result.unwrap_err()
        );
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_delegates_to_wrapped_endpoint() {
        let retry_endpoint = retry(TestEndpoint);
        assert_eq!(retry_endpoint.grpc_endpoint(), "");
    }

    #[xmtp_common::test]
    fn test_body_delegates_to_wrapped_endpoint() {
        let retry_endpoint = retry(TestEndpoint);
        let result = retry_endpoint.body();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), bytes::Bytes::from(vec![]));
    }

    #[xmtp_common::test]
    async fn retries_with_strategy() {
        let mut client = MockNetworkClient::new();
        client.expect_request().times(2).returning(|_, _, _| {
            Err(ApiClientError::Client {
                source: MockError::ARetryableError,
            })
        });
        client
            .expect_request()
            .times(1)
            .returning(|_, _, _| Ok(http::Response::new(vec![1].into())));

        let result: Result<(), _> = TestEndpoint
            .ignore_response() // ignore b/c invalid protobuf bytes
            .retry_with_strategy(Retry::builder().retries(2).build())
            .query(&client)
            .await;
        assert!(result.is_ok(), "{:?}", result.unwrap_err());
    }
}
