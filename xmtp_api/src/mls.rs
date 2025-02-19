use std::collections::HashMap;

use super::ApiClientWrapper;
use crate::{Result, XmtpApi};
use xmtp_common::retry_async;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::xmtp::mls::api::v1::{
    subscribe_group_messages_request::Filter as GroupFilterProto,
    subscribe_welcome_messages_request::Filter as WelcomeFilterProto, FetchKeyPackagesRequest,
    GroupMessage, GroupMessageInput, KeyPackageUpload, PagingInfo, QueryGroupMessagesRequest,
    QueryWelcomeMessagesRequest, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
    SortDirection, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
    UploadKeyPackageRequest, WelcomeMessage, WelcomeMessageInput,
};
use xmtp_proto::ApiError;
// the max page size for queries
const MAX_PAGE_SIZE: u32 = 100;

/// A filter for querying group messages
#[derive(Clone)]
pub struct GroupFilter {
    pub group_id: Vec<u8>,
    pub id_cursor: Option<u64>,
}

impl GroupFilter {
    pub fn new(group_id: Vec<u8>, id_cursor: Option<u64>) -> Self {
        Self {
            group_id,
            id_cursor,
        }
    }
}

impl From<GroupFilter> for GroupFilterProto {
    fn from(filter: GroupFilter) -> Self {
        Self {
            group_id: filter.group_id,
            id_cursor: filter.id_cursor.unwrap_or(0),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct NewInstallation {
    pub installation_key: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    pub timestamp_ns: u64,
}

#[derive(Debug, PartialEq)]
pub struct RevokeInstallation {
    pub installation_key: Vec<u8>, // TODO: Add proof of revocation
    pub timestamp_ns: u64,
}

#[derive(Debug, PartialEq)]
pub enum IdentityUpdate {
    NewInstallation(NewInstallation),
    RevokeInstallation(RevokeInstallation),
    Invalid,
}

type KeyPackageMap = HashMap<Vec<u8>, Vec<u8>>;

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpApi,
{
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn query_group_messages(
        &self,
        group_id: Vec<u8>,
        id_cursor: Option<u64>,
    ) -> Result<Vec<GroupMessage>> {
        tracing::debug!(
            group_id = hex::encode(&group_id),
            id_cursor,
            inbox_id = self.inbox_id,
            "query group messages"
        );
        let mut out: Vec<GroupMessage> = vec![];
        let mut id_cursor = id_cursor;
        loop {
            let mut result = retry_async!(
                self.retry_strategy,
                (async {
                    self.api_client
                        .query_group_messages(QueryGroupMessagesRequest {
                            group_id: group_id.clone(),
                            paging_info: Some(PagingInfo {
                                id_cursor: id_cursor.unwrap_or(0),
                                limit: MAX_PAGE_SIZE,
                                direction: SortDirection::Ascending as i32,
                            }),
                        })
                        .await
                })
            )
            .map_err(ApiError::from)?;
            let num_messages = result.messages.len();
            out.append(&mut result.messages);

            if num_messages < MAX_PAGE_SIZE as usize || result.paging_info.is_none() {
                break;
            }

            let paging_info = result.paging_info.expect("Empty paging info");
            if paging_info.id_cursor == 0 {
                break;
            }

            id_cursor = Some(paging_info.id_cursor);
        }
        Ok(out)
    }

    /// Query for the latest message on a group
    pub async fn query_latest_group_message<Id: AsRef<[u8]> + Copy>(
        &self,
        group_id: Id,
    ) -> Result<Option<GroupMessage>> {
        tracing::debug!(
            group_id = hex::encode(group_id),
            inbox_id = self.inbox_id,
            "query latest group message"
        );
        let result = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .query_group_messages(QueryGroupMessagesRequest {
                        group_id: group_id.as_ref().to_vec(),
                        paging_info: Some(PagingInfo {
                            id_cursor: 0,
                            limit: 1,
                            direction: SortDirection::Descending as i32,
                        }),
                    })
                    .await
            })
        )
        .map_err(ApiError::from)?;

        Ok(result.messages.into_iter().next())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn query_welcome_messages<Id: AsRef<[u8]> + Copy>(
        &self,
        installation_id: Id,
        id_cursor: Option<u64>,
    ) -> Result<Vec<WelcomeMessage>> {
        tracing::debug!(
            installation_id = hex::encode(installation_id),
            cursor = id_cursor,
            inbox_id = self.inbox_id,
            "query welcomes"
        );
        let mut out: Vec<WelcomeMessage> = vec![];
        let page_size = 100;
        let mut id_cursor = id_cursor;
        loop {
            let mut result = retry_async!(
                self.retry_strategy,
                (async {
                    self.api_client
                        .query_welcome_messages(QueryWelcomeMessagesRequest {
                            installation_key: installation_id.as_ref().to_vec(),
                            paging_info: Some(PagingInfo {
                                id_cursor: id_cursor.unwrap_or(0),
                                limit: page_size,
                                direction: SortDirection::Ascending as i32,
                            }),
                        })
                        .await
                })
            )
            .map_err(ApiError::from)?;

            let num_messages = result.messages.len();
            out.append(&mut result.messages);

            if num_messages < page_size as usize || result.paging_info.is_none() {
                break;
            }

            let paging_info = result.paging_info.expect("Empty paging info");
            if paging_info.id_cursor == 0 {
                break;
            }

            id_cursor = Some(paging_info.id_cursor);
        }

        Ok(out)
    }

    /// Upload a KeyPackage to the network
    /// New InboxID clients should set `is_inbox_id_credential` to true.
    /// V3 clients should have `is_inbox_id_credential` to `false`.
    /// Not indicating your client version will result in validation failure.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn upload_key_package(
        &self,
        key_package: Vec<u8>,
        is_inbox_id_credential: bool,
    ) -> Result<()> {
        tracing::debug!(inbox_id = self.inbox_id, "upload key packages");
        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .upload_key_package(UploadKeyPackageRequest {
                        key_package: Some(KeyPackageUpload {
                            key_package_tls_serialized: key_package.clone(),
                        }),
                        is_inbox_id_credential,
                    })
                    .await
            })
        )
        .map_err(ApiError::from)?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn fetch_key_packages(
        &self,
        installation_keys: Vec<Vec<u8>>,
    ) -> Result<KeyPackageMap> {
        tracing::debug!(inbox_id = self.inbox_id, "fetch key packages");
        let res = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .fetch_key_packages(FetchKeyPackagesRequest {
                        installation_keys: installation_keys.clone(),
                    })
                    .await
            })
        )
        .map_err(ApiError::from)?;

        if res.key_packages.len() != installation_keys.len() {
            return Err(crate::Error::MismatchedKeyPackages {
                key_packages: res.key_packages.len(),
                installation_keys: installation_keys.len(),
            });
        }

        let mapping: KeyPackageMap = res
            .key_packages
            .into_iter()
            .enumerate()
            .map(|(idx, key_package)| {
                (
                    installation_keys[idx].to_vec(),
                    key_package.key_package_tls_serialized,
                )
            })
            .collect();

        Ok(mapping)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn send_welcome_messages(&self, messages: &[WelcomeMessageInput]) -> Result<()> {
        tracing::debug!(inbox_id = self.inbox_id, "send welcome messages");
        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .send_welcome_messages(SendWelcomeMessagesRequest {
                        messages: messages.to_vec(),
                    })
                    .await
            })
        )
        .map_err(ApiError::from)?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn send_group_messages(&self, group_messages: Vec<GroupMessageInput>) -> Result<()> {
        tracing::debug!(
            inbox_id = self.inbox_id,
            "sending [{}] group messages",
            group_messages.len()
        );

        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .send_group_messages(SendGroupMessagesRequest {
                        messages: group_messages.clone(),
                    })
                    .await
            })
        )
        .map_err(ApiError::from)?;

        Ok(())
    }

    pub async fn subscribe_group_messages(
        &self,
        filters: Vec<GroupFilter>,
    ) -> Result<<ApiClient as XmtpMlsStreams>::GroupMessageStream<'_>>
    where
        ApiClient: XmtpMlsStreams,
    {
        tracing::debug!(inbox_id = self.inbox_id, "subscribing to group messages");
        self.api_client
            .subscribe_group_messages(SubscribeGroupMessagesRequest {
                filters: filters.into_iter().map(|f| f.into()).collect(),
            })
            .await
            .map_err(ApiError::from)
            .map_err(crate::Error::from)
    }

    pub async fn subscribe_welcome_messages(
        &self,
        installation_key: &[u8],
        id_cursor: Option<u64>,
    ) -> Result<<ApiClient as XmtpMlsStreams>::WelcomeMessageStream<'_>>
    where
        ApiClient: XmtpMlsStreams,
    {
        tracing::debug!(inbox_id = self.inbox_id, "subscribing to welcome messages");
        // _NOTE_:
        // Default ID Cursor should be one
        // else we miss welcome messages
        self.api_client
            .subscribe_welcome_messages(SubscribeWelcomeMessagesRequest {
                filters: vec![WelcomeFilterProto {
                    installation_key: installation_key.to_vec(),
                    id_cursor: id_cursor.unwrap_or(1),
                }],
            })
            .await
            .map_err(ApiError::from)
            .map_err(crate::Error::from)
    }
}

#[cfg(test)]
pub mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::test_utils::*;
    use crate::*;

    use crate::test_utils::MockError;
    use xmtp_common::StreamHandle;
    use xmtp_proto::xmtp::mls::api::v1::{
        fetch_key_packages_response::KeyPackage, FetchKeyPackagesResponse, PagingInfo,
        QueryGroupMessagesResponse,
    };

    use std::sync::{Arc, Mutex};
    use tokio::sync::Barrier;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "multi_thread"))]
    async fn test_upload_key_package() {
        tracing::debug!("test_upload_key_package");
        let mut mock_api = MockApiClient::new();
        let key_package = vec![1, 2, 3];
        // key_package gets moved below but needs to be used for assertions later
        let key_package_clone = key_package.clone();
        mock_api
            .expect_upload_key_package()
            .withf(move |req| {
                req.key_package
                    .as_ref()
                    .unwrap()
                    .key_package_tls_serialized
                    .eq(&key_package)
            })
            .returning(move |_| Ok(()));
        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());
        let result = wrapper.upload_key_package(key_package_clone, false).await;
        assert!(result.is_ok());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_fetch_key_packages() {
        tracing::debug!("test_fetch_key_packages");
        let mut mock_api = MockApiClient::new();
        let installation_keys: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6]];
        mock_api.expect_fetch_key_packages().returning(move |_| {
            Ok(FetchKeyPackagesResponse {
                key_packages: vec![
                    KeyPackage {
                        key_package_tls_serialized: vec![7, 8, 9],
                    },
                    KeyPackage {
                        key_package_tls_serialized: vec![10, 11, 12],
                    },
                ],
            })
        });
        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());
        let result = wrapper
            .fetch_key_packages(installation_keys.clone())
            .await
            .unwrap();
        assert_eq!(result.len(), 2);

        for (k, v) in result {
            if k.eq(&installation_keys[0]) {
                assert_eq!(v, vec![7, 8, 9]);
            } else {
                assert_eq!(v, vec![10, 11, 12]);
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_read_group_messages_single_page() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3, 4];
        let group_id_clone = group_id.clone();
        // Set expectation for first request with no cursor
        mock_api
            .expect_query_group_messages()
            .returning(move |req| {
                assert_eq!(req.group_id, group_id.clone());

                Ok(QueryGroupMessagesResponse {
                    paging_info: Some(PagingInfo {
                        id_cursor: 0,
                        limit: 100,
                        direction: 0,
                    }),
                    messages: build_group_messages(10, group_id.clone()),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_read_group_messages_single_page_exactly_100_results() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3, 4];
        let group_id_clone = group_id.clone();
        // Set expectation for first request with no cursor
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |req| {
                assert_eq!(req.group_id, group_id.clone());

                Ok(QueryGroupMessagesResponse {
                    paging_info: Some(PagingInfo {
                        direction: 0,
                        limit: 100,
                        id_cursor: 0,
                    }),
                    messages: build_group_messages(100, group_id.clone()),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 100);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_read_topic_multi_page() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3, 4];
        let group_id_clone = group_id.clone();
        let group_id_clone2 = group_id.clone();
        // Set expectation for first request with no cursor
        mock_api
            .expect_query_group_messages()
            .withf(move |req| match req.paging_info {
                Some(paging_info) => paging_info.id_cursor == 0,
                None => true,
            })
            .returning(move |req| {
                assert_eq!(req.group_id, group_id.clone());

                Ok(QueryGroupMessagesResponse {
                    paging_info: Some(PagingInfo {
                        id_cursor: 10,
                        limit: 100,
                        direction: 0,
                    }),
                    messages: build_group_messages(100, group_id.clone()),
                })
            });
        // Set expectation for requests with a cursor
        mock_api
            .expect_query_group_messages()
            .withf(|req| match req.paging_info {
                Some(paging_info) => paging_info.id_cursor > 0,
                None => false,
            })
            .returning(move |req| {
                assert_eq!(req.group_id, group_id_clone.clone());

                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(100, group_id_clone.clone()),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());

        let result = wrapper
            .query_group_messages(group_id_clone2, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 200);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn it_retries_twice_then_succeeds() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3];
        let group_id_clone = group_id.clone();

        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| Err(MockError::MockQuery));
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| Err(MockError::MockQuery));
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(50, group_id.clone()),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api.into(), exponential().build());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 50);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn it_cools_down_then_succeeds() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3];
        let group_id_clone = group_id.clone();
        let time_failure = Arc::new(Mutex::new(None));
        let time_success = Arc::new(Mutex::new(None));

        let f = time_failure.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = f.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Err(MockError::RateLimit)
            });

        let s = time_success.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = s.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(50, group_id.clone()),
                })
            });
        let strategy = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_millis(10))
            .build();
        let cooldown = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_secs(1))
            .build();
        let wrapper = ApiClientWrapper::new(
            mock_api.into(),
            Retry::builder()
                .with_strategy(strategy)
                .with_cooldown(cooldown)
                .build(),
        );

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        let failed = time_failure.lock().unwrap().unwrap();
        let success = time_success.lock().unwrap().unwrap();
        assert!((failed.elapsed() - success.elapsed()) > std::time::Duration::from_secs(1));
        assert_eq!(result.len(), 50);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn it_exponentially_increases_the_cooldown() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3];
        let group_id_clone = group_id.clone();
        let time_failure = Arc::new(Mutex::new(None));
        let time_failure2 = Arc::new(Mutex::new(None));
        let time_failure3 = Arc::new(Mutex::new(None));
        let time_success = Arc::new(Mutex::new(None));

        let f = time_failure.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = f.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Err(MockError::RateLimit)
            });

        let f = time_failure2.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = f.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Err(MockError::RateLimit)
            });

        let f = time_failure3.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = f.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Err(MockError::RateLimit)
            });

        let s = time_success.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                let mut set = s.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(50, group_id.clone()),
                })
            });

        let strategy = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_millis(10))
            .build();
        let cooldown = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_secs(1))
            .multiplier(2)
            .build();
        let wrapper = ApiClientWrapper::new(
            mock_api.into(),
            Retry::builder()
                .with_strategy(strategy)
                .with_cooldown(cooldown)
                .build(),
        );

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        let failed = time_failure.lock().unwrap().unwrap();
        let failed2 = time_failure2.lock().unwrap().unwrap();
        let failed3 = time_failure3.lock().unwrap().unwrap();
        let success = time_success.lock().unwrap().unwrap();
        assert!((failed.elapsed() - success.elapsed()) > std::time::Duration::from_secs(1));
        assert!((failed2.elapsed() - success.elapsed()) > std::time::Duration::from_secs(2));
        tracing::info!("failed3.elapsed={:?}", failed3.elapsed());
        assert!((failed3.elapsed() - success.elapsed()) > std::time::Duration::from_secs(4));

        assert_eq!(result.len(), 50);
    }

    // this test fails on wasm, but seems to be functioning as expected
    // It fails because of mockall expects which should be investigated
    // but occurs in an unrelated library (mockall).
    #[cfg_attr(target_arch = "wasm32", ignore)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn cooldowns_apply_to_concurrent_fns() {
        xmtp_common::logger();
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3];

        let time_failure = Arc::new(Mutex::new(None));
        let time_success = Arc::new(Mutex::new(Vec::new()));
        // first task gets rate limited
        let f = time_failure.clone();
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                tracing::info!("called query_message rate limit");
                let mut set = f.lock().unwrap();
                *set = Some(xmtp_common::time::Instant::now());
                Err(MockError::RateLimit)
            });

        let s = time_success.clone();
        mock_api.expect_query_group_messages().times(3).returning({
            let group_id = group_id.clone();
            move |_| {
                tracing::info!("Called non rate limit");
                let set = s.lock();
                set.unwrap().push(xmtp_common::time::Instant::now());
                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(50, group_id.clone()),
                })
            }
        });

        let strategy = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_millis(10))
            .build();
        let cooldown = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_secs(2))
            .multiplier(2)
            .build();

        let wrapper = ApiClientWrapper::new(
            mock_api.into(),
            Retry::builder()
                .with_strategy(strategy)
                .with_cooldown(cooldown)
                .build(),
        );

        let barrier = Arc::new(Barrier::new(4));
        let c = barrier.clone();
        let api = wrapper.clone();
        let g = group_id.clone();
        let rate_limits = xmtp_common::spawn(None, async move {
            c.wait().await;
            tracing::info!("Waited query_msg RATE LIMIT");
            let _ = api.query_group_messages(g.clone(), None).await.unwrap();
        })
        .join();

        let c = barrier.clone();
        let g = group_id.clone();
        let api = wrapper.clone();
        let h1 = xmtp_common::spawn(None, async move {
            c.wait().await;
            tracing::info!("Waited query_msg 2 sleeping 5ms");
            // ensure 5ms to fire after rate limit
            xmtp_common::time::sleep(std::time::Duration::from_millis(5)).await;
            let _ = api.query_group_messages(g, None).await;
        })
        .join();

        let c = barrier.clone();
        let g = group_id.clone();
        let api = wrapper.clone();
        let h2 = xmtp_common::spawn(None, async move {
            c.wait().await;
            tracing::info!("Waited query_msg 3 sleeping 5ms");
            // ensure 5ms to fire after rate limit
            xmtp_common::time::sleep(std::time::Duration::from_millis(5)).await;
            let _ = api.query_group_messages(g, None).await;
        })
        .join();

        let c = barrier.clone();
        let g = group_id.clone();
        let api = wrapper.clone();
        let h3 = xmtp_common::spawn(None, async move {
            c.wait().await;
            tracing::info!("Waited query_msg 3 sleeping 1000ms");
            // Firing this 1s later, should still start within duration of backoff
            xmtp_common::time::sleep(std::time::Duration::from_millis(1_000)).await;
            let _ = api.query_group_messages(g, None).await;
            tracing::info!("long query finished");
        })
        .join();

        futures::future::join_all(vec![rate_limits, h1, h2, h3]).await;
        let fail = time_failure.lock().unwrap().unwrap();
        let success = time_success.lock().unwrap();
        let min_expected_cooldown = std::time::Duration::from_secs(2);
        let max_expected_cooldown = std::time::Duration::from_millis(2050);

        for s in success.iter() {
            tracing::info!("Duration since rate limit={:?}", s.duration_since(fail));
            assert!(s.duration_since(fail) > min_expected_cooldown);
            assert!(s.duration_since(fail) < max_expected_cooldown);
        }
    }
}
