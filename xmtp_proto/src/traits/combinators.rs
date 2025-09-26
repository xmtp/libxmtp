//! Query Combinators
use xmtp_common::{
    retry_async, ExponentialBackoff, Retry, RetryableError, Strategy as RetryStrategy,
};
use xmtp_configuration::MAX_PAGE_SIZE;

use crate::{
    api::{ApiClientError, Client, Endpoint, Pageable, Query},
    api_client::Paged,
};

/// Endpoint that is paged with [`PagingInfo`]
pub struct V3Paged<E> {
    endpoint: E,
    id_cursor: Option<u64>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<Vec<<T as Paged>::Message>, C> for V3Paged<E>
where
    E: Endpoint<Output = T> + Pageable + Send + Sync,
    C: Client + Sync + Send,
    C::Error: std::error::Error,
    T: Default + prost::Message + Paged + Send + 'static,
    <T as Paged>::Message: Send,
{
    async fn query(
        &mut self,
        client: &C,
    ) -> Result<Vec<<T as Paged>::Message>, ApiClientError<C::Error>> {
        let mut out: Vec<<T as Paged>::Message> = vec![];
        self.endpoint.set_cursor(self.id_cursor.unwrap_or(0));
        loop {
            let result = self.endpoint.query(client).await?;
            let info = *result.info();
            let mut messages = result.messages();
            let num_messages = messages.len();
            out.append(&mut messages);

            if num_messages < MAX_PAGE_SIZE as usize || info.is_none() {
                break;
            }

            let paging_info = info.expect("Empty paging info");
            if paging_info.id_cursor == 0 {
                break;
            }

            self.endpoint.set_cursor(paging_info.id_cursor);
        }
        Ok(out)
    }
}

/// Set an endpoint to be paged with v3 paging info
pub fn v3_paged<E>(endpoint: E, id_cursor: Option<u64>) -> V3Paged<E>
where
    E: Endpoint + Pageable + Send + Sync,
{
    V3Paged {
        endpoint,
        id_cursor,
    }
}

pub struct RetryQuery<E, S = ExponentialBackoff> {
    endpoint: E,
    retry: Retry<S>,
}

impl<E> Pageable for RetryQuery<E>
where
    E: Pageable,
{
    fn set_cursor(&mut self, cursor: u64) {
        self.endpoint.set_cursor(cursor)
    }
}

struct RetrySpecialized;
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C, S> Query<T, C, RetrySpecialized> for RetryQuery<E, S>
where
    E: Endpoint<Output = T> + Send + Sync,
    C: Client + Sync + Send,
    C::Error: RetryableError,
    T: Default + prost::Message + Send + 'static,
    S: RetryStrategy + Send + Sync,
{
    async fn query(&mut self, client: &C) -> Result<T, ApiClientError<C::Error>> {
        retry_async!(self.retry, (async { self.endpoint.query(client).await }))
    }
}

impl<E, S> Endpoint for RetryQuery<E, S>
where
    E: Endpoint,
{
    type Output = <E as Endpoint>::Output;

    fn http_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.http_endpoint()
    }

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.grpc_endpoint()
    }

    fn body(&self) -> Result<bytes::Bytes, super::BodyError> {
        self.endpoint.body()
    }
}

// retry with the default retry strategy (ExponentialBackoff)
pub fn retry<E: Endpoint>(endpoint: E) -> Passthrough<RetryQuery<E, ExponentialBackoff>> {
    Passthrough::new(RetryQuery::<E, _> {
        endpoint,
        retry: Retry::default(),
    })
}

pub fn retry_with_strategy<E, S>(endpoint: E, retry: Retry<S>) -> RetryQuery<E, S> {
    RetryQuery::<E, S> { endpoint, retry }
}

/// passthrough struct delegates to a single `Query` implementation
/// this avoid using FQS in api functions (i.e specifying the private Specialization type).
/// used in the return type for Specialized combinator Query implementations (ex: Retry)
/// Passthorugh can be erased by returning impl Trait (impl Endpoint) or Box<dyn Endpoint> instead of the concrete
/// type.
pub struct Passthrough<E> {
    endpoint: E,
}

impl<E> Passthrough<E> {
    fn new(endpoint: E) -> Self {
        Self { endpoint }
    }
}

impl<E: Endpoint> Endpoint for Passthrough<E> {
    type Output = <E as Endpoint>::Output;

    fn http_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.http_endpoint()
    }

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.grpc_endpoint()
    }

    fn body(&self) -> Result<bytes::Bytes, super::BodyError> {
        self.endpoint.body()
    }
}

impl<E> Pageable for Passthrough<E>
where
    E: Pageable,
{
    fn set_cursor(&mut self, cursor: u64) {
        self.endpoint.set_cursor(cursor);
    }
}
