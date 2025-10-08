use std::marker::PhantomData;

use xmtp_configuration::MAX_PAGE_SIZE;

use crate::{
    api::{ApiClientError, Client, Endpoint, Pageable, Query},
    api_client::Paged,
};

/// Endpoint that is paged with [`PagingInfo`]
/// implements the v3 backend paging algorithm for endpoints
/// which implement the [`Pageable`] trait
pub struct V3Paged<E, T> {
    endpoint: E,
    id_cursor: Option<u64>,
    _marker: PhantomData<T>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E, T, C> Query<C> for V3Paged<E, T>
where
    E: Query<C, Output = T> + Pageable,
    C: Client,
    C::Error: std::error::Error,
    T: Default + prost::Message + Paged + 'static,
    <T as Paged>::Message: Send + Sync,
{
    type Output = Vec<<T as Paged>::Message>;
    async fn query(
        &mut self,
        client: &C,
    ) -> Result<Vec<<T as Paged>::Message>, ApiClientError<C::Error>> {
        let mut out: Vec<<T as Paged>::Message> = vec![];
        self.endpoint.set_cursor(self.id_cursor.unwrap_or(0));
        loop {
            let result: T = self.endpoint.query(client).await?;
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

pub struct V3PagedSpecialized<S> {
    _marker: PhantomData<S>,
}

impl<S, E: Endpoint<S>, T: Send + Sync> Endpoint<V3PagedSpecialized<S>> for V3Paged<E, T> {
    type Output = <E as Endpoint<S>>::Output;

    fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
        self.endpoint.grpc_endpoint()
    }

    fn body(&self) -> Result<bytes::Bytes, crate::api::BodyError> {
        self.endpoint.body()
    }
}

/// Set an endpoint to be paged with v3 paging info
pub fn v3_paged<E, T>(endpoint: E, id_cursor: Option<u64>) -> V3Paged<E, T> {
    V3Paged {
        endpoint,
        id_cursor,
        _marker: PhantomData,
    }
}

#[cfg(test)]
mod tests {

    use std::borrow::Cow;

    use prost::Message;

    use crate::{
        api::{self, Endpoint, EndpointExt, mock::MockNetworkClient},
        mls_v1::{PagingInfo, SortDirection},
    };

    use super::*;
    use rstest::*;

    #[derive(prost::Message)]
    struct TestV3Pageable {
        #[prost(message, optional, tag = "1")]
        info: Option<PagingInfo>,
        #[prost(int32, repeated, tag = "2")]
        msgs: Vec<i32>,
    }

    impl Paged for TestV3Pageable {
        type Message = i32;

        fn info(&self) -> &Option<PagingInfo> {
            &self.info
        }

        fn messages(self) -> Vec<Self::Message> {
            self.msgs
        }
    }

    #[derive(Default)]
    struct PageableTestEndpoint {
        inner: TestV3Pageable,
    }

    impl Endpoint for PageableTestEndpoint {
        type Output = TestV3Pageable;

        fn grpc_endpoint(&self) -> std::borrow::Cow<'static, str> {
            Cow::Borrowed("")
        }

        fn body(&self) -> Result<bytes::Bytes, api::BodyError> {
            Ok(self.inner.encode_to_vec().into())
        }
    }

    impl Pageable for PageableTestEndpoint {
        fn set_cursor(&mut self, cursor: u64) {
            if let Some(ref mut info) = self.inner.info {
                info.id_cursor = cursor;
            }
        }
    }

    #[fixture]
    fn client() -> MockNetworkClient {
        let mut client = MockNetworkClient::new();
        client.expect_request().times(1).returning(|_, _, b| {
            let body = TestV3Pageable::decode(b.clone()).unwrap();
            assert_eq!(
                body.info.unwrap().id_cursor,
                1,
                "expected 1 got {}",
                body.info.unwrap().id_cursor
            );
            Ok(http::Response::new(
                TestV3Pageable {
                    info: Some(PagingInfo {
                        direction: SortDirection::Ascending as i32,
                        limit: 100,
                        id_cursor: 4,
                    }),
                    msgs: vec![0; MAX_PAGE_SIZE as usize],
                }
                .encode_to_vec()
                .into(),
            ))
        });
        client.expect_request().times(1).returning(|_, _, b| {
            let body = TestV3Pageable::decode(b.clone()).unwrap();
            assert_eq!(
                body.info.unwrap().id_cursor,
                4,
                "expected 4 got {}",
                body.info.unwrap().id_cursor
            );
            Ok(http::Response::new(
                TestV3Pageable {
                    info: Some(PagingInfo {
                        direction: SortDirection::Ascending as i32,
                        limit: 100,
                        id_cursor: 6,
                    }),
                    msgs: vec![1; MAX_PAGE_SIZE as usize],
                }
                .encode_to_vec()
                .into(),
            ))
        });
        client.expect_request().times(1).returning(|_, _, b| {
            let body = TestV3Pageable::decode(b.clone()).unwrap();
            assert_eq!(
                body.info.unwrap().id_cursor,
                6,
                "expected 6 got {}",
                body.info.unwrap().id_cursor
            );
            Ok(http::Response::new(
                TestV3Pageable {
                    info: None,
                    msgs: vec![7],
                }
                .encode_to_vec()
                .into(),
            ))
        });
        client
    }

    #[rstest]
    #[xmtp_common::test]
    async fn pages_endpoint(client: MockNetworkClient) {
        let endpoint = PageableTestEndpoint {
            inner: TestV3Pageable {
                info: Some(PagingInfo {
                    direction: SortDirection::Ascending as i32,
                    limit: 100,
                    id_cursor: 2,
                }),
                msgs: vec![],
            },
        };
        // let result = api::v3_paged(endpoint, Some(1)).query(&client).await;
        let result = endpoint.v3_paged(Some(1)).query(&client).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        let msgs = std::iter::repeat_n(0, MAX_PAGE_SIZE as usize)
            .chain(std::iter::repeat_n(1, MAX_PAGE_SIZE as usize))
            .chain(vec![7])
            .collect::<Vec<_>>();
        assert_eq!(result, msgs, "{:?}", result);
    }

    #[rstest]
    #[xmtp_common::test]
    async fn pages_endpoint_can_be_retried(client: MockNetworkClient) {
        let endpoint = PageableTestEndpoint {
            inner: TestV3Pageable {
                info: Some(PagingInfo {
                    direction: SortDirection::Ascending as i32,
                    limit: 100,
                    id_cursor: 2,
                }),
                msgs: vec![],
            },
        };
        let result = api::v3_paged(api::retry(endpoint), Some(1))
            .query(&client)
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        let msgs = std::iter::repeat_n(0, MAX_PAGE_SIZE as usize)
            .chain(std::iter::repeat_n(1, MAX_PAGE_SIZE as usize))
            .chain(vec![7])
            .collect::<Vec<_>>();
        assert_eq!(result, msgs, "{:?}", result);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_delegates_to_wrapped_endpoint() {
        let base_endpoint = PageableTestEndpoint::default();
        let paged_endpoint: V3Paged<PageableTestEndpoint, TestV3Pageable> =
            v3_paged(base_endpoint, Some(0));
        assert_eq!(paged_endpoint.grpc_endpoint(), "");
    }

    #[xmtp_common::test]
    fn test_body_delegates_to_wrapped_endpoint() {
        let base_endpoint = PageableTestEndpoint::default();
        let paged_endpoint: V3Paged<PageableTestEndpoint, TestV3Pageable> =
            v3_paged(base_endpoint, Some(0));
        let result = paged_endpoint.body();
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            bytes::Bytes::from(TestV3Pageable::default().encode_to_vec())
        );
    }

    #[xmtp_common::test]
    fn test_pageable_test_endpoint_body_encodes_protobuf_message() {
        let endpoint = PageableTestEndpoint {
            inner: TestV3Pageable {
                info: Some(PagingInfo {
                    direction: SortDirection::Ascending as i32,
                    limit: 100,
                    id_cursor: 42,
                }),
                msgs: vec![1, 2, 3],
            },
        };
        let result = endpoint.body();
        assert!(result.is_ok());
        let expected_bytes = endpoint.inner.encode_to_vec();
        assert_eq!(result.unwrap(), bytes::Bytes::from(expected_bytes));
    }

    // this test here to ensure it compiles
    #[xmtp_common::test]
    async fn endpoints_can_be_chained() {
        let client = MockNetworkClient::new();
        std::mem::drop(
            PageableTestEndpoint::default()
                .v3_paged(Some(0))
                .retry()
                .query(&client),
        );
    }
}
