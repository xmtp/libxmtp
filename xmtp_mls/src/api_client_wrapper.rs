use std::collections::HashMap;

use crate::{retry::Retry, retry_async};
use xmtp_proto::{
    api_client::{
        Envelope, Error as ApiError, ErrorKind, PagingInfo, QueryRequest, SubscribeRequest,
        XmtpApiClient, XmtpMlsClient,
    },
    xmtp::{
        message_api::v1::{Cursor, SortDirection},
        mls::api::v1::{
            get_identity_updates_response::update::Kind as UpdateKind,
            publish_welcomes_request::WelcomeMessageRequest, FetchKeyPackagesRequest,
            GetIdentityUpdatesRequest, KeyPackageUpload, PublishToGroupRequest,
            PublishWelcomesRequest, RegisterInstallationRequest, UploadKeyPackageRequest,
        },
        mls::message_contents::{
            group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
            welcome_message::{Version as WelcomeMessageVersion, V1 as WelcomeMessageV1},
            GroupMessage, WelcomeMessage as WelcomeMessageProto,
        },
    },
};

#[derive(Debug)]
pub struct ApiClientWrapper<ApiClient> {
    api_client: ApiClient,
    retry_strategy: Retry,
}

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpApiClient,
{
    pub fn new(api_client: ApiClient, retry_strategy: Retry) -> Self {
        Self {
            api_client,
            retry_strategy,
        }
    }

    pub async fn read_topic(
        &self,
        topic: &str,
        start_time_ns: u64,
    ) -> Result<Vec<Envelope>, ApiError> {
        let mut cursor: Option<Cursor> = None;
        let mut out: Vec<Envelope> = vec![];
        let page_size = 100;
        loop {
            let mut result = retry_async!(
                self.retry_strategy,
                (async {
                    self.api_client
                        .query(QueryRequest {
                            content_topics: vec![topic.to_string()],
                            start_time_ns,
                            end_time_ns: 0,
                            paging_info: Some(PagingInfo {
                                cursor: cursor.clone(),
                                limit: page_size,
                                direction: SortDirection::Ascending as i32,
                            }),
                        })
                        .await
                })
            )?;

            let num_envelopes = result.envelopes.len();
            out.append(&mut result.envelopes);

            if num_envelopes < page_size as usize || result.paging_info.is_none() {
                break;
            }

            cursor = result.paging_info.expect("Empty paging info").cursor;

            if cursor.is_none() {
                break;
            }
        }

        Ok(out)
    }

    pub async fn register_installation(&self, key_package: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        let res = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .register_installation(RegisterInstallationRequest {
                        key_package: Some(KeyPackageUpload {
                            key_package_tls_serialized: key_package.to_vec(),
                        }),
                    })
                    .await
            })
        )?;

        Ok(res.installation_id)
    }

    pub async fn upload_key_package(&self, key_package: Vec<u8>) -> Result<(), ApiError> {
        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .upload_key_package(UploadKeyPackageRequest {
                        key_package: Some(KeyPackageUpload {
                            key_package_tls_serialized: key_package.clone(),
                        }),
                    })
                    .await
            })
        )?;

        Ok(())
    }

    pub async fn fetch_key_packages(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<KeyPackageMap, ApiError> {
        let res = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .fetch_key_packages(FetchKeyPackagesRequest {
                        installation_ids: installation_ids.clone(),
                    })
                    .await
            })
        )?;

        if res.key_packages.len() != installation_ids.len() {
            println!("mismatched number of results");
            return Err(ApiError::new(ErrorKind::MlsError));
        }

        let mapping: KeyPackageMap = res
            .key_packages
            .into_iter()
            .enumerate()
            .map(|(idx, key_package)| {
                (
                    installation_ids[idx].to_vec(),
                    key_package.key_package_tls_serialized,
                )
            })
            .collect();

        Ok(mapping)
    }

    pub async fn publish_welcomes(
        &self,
        welcome_messages: Vec<WelcomeMessage>,
    ) -> Result<(), ApiError> {
        let welcome_requests: Vec<WelcomeMessageRequest> = welcome_messages
            .into_iter()
            .map(|msg| WelcomeMessageRequest {
                installation_id: msg.installation_id,
                welcome_message: Some(WelcomeMessageProto {
                    version: Some(WelcomeMessageVersion::V1(WelcomeMessageV1 {
                        welcome_message_tls_serialized: msg.ciphertext,
                    })),
                }),
            })
            .collect();

        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .publish_welcomes(PublishWelcomesRequest {
                        welcome_messages: welcome_requests.clone(),
                    })
                    .await
            })
        )?;

        Ok(())
    }

    pub async fn get_identity_updates(
        &self,
        start_time_ns: u64,
        account_addresses: Vec<String>,
    ) -> Result<IdentityUpdatesMap, ApiError> {
        let result = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .get_identity_updates(GetIdentityUpdatesRequest {
                        start_time_ns,
                        account_addresses: account_addresses.clone(),
                    })
                    .await
            })
        )?;

        if result.updates.len() != account_addresses.len() {
            println!("mismatched number of results");
            return Err(ApiError::new(ErrorKind::MlsError));
        }

        let mapping: IdentityUpdatesMap = result
            .updates
            .into_iter()
            .zip(account_addresses.into_iter())
            .map(|(update, account_address)| {
                (
                    account_address,
                    update
                        .updates
                        .into_iter()
                        .map(|update| match update.kind {
                            Some(UpdateKind::NewInstallation(new_installation)) => {
                                IdentityUpdate::NewInstallation(NewInstallation {
                                    timestamp_ns: update.timestamp_ns,
                                    installation_id: new_installation.installation_id,
                                    credential_bytes: new_installation.credential_identity,
                                })
                            }
                            Some(UpdateKind::RevokedInstallation(revoke_installation)) => {
                                IdentityUpdate::RevokeInstallation(RevokeInstallation {
                                    timestamp_ns: update.timestamp_ns,
                                    installation_id: revoke_installation.installation_id,
                                })
                            }
                            None => {
                                println!("no update kind");
                                IdentityUpdate::Invalid
                            }
                        })
                        .collect(),
                )
            })
            .collect();

        Ok(mapping)
    }

    pub async fn publish_to_group(&self, group_messages: Vec<&[u8]>) -> Result<(), ApiError> {
        let to_send: Vec<GroupMessage> = group_messages
            .iter()
            .map(|msg| GroupMessage {
                version: Some(GroupMessageVersion::V1(GroupMessageV1 {
                    mls_message_tls_serialized: msg.to_vec(),
                })),
            })
            .collect();

        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .publish_to_group(PublishToGroupRequest {
                        messages: to_send.clone(),
                    })
                    .await
            })
        )?;

        Ok(())
    }

    pub async fn subscribe(
        &self,
        content_topics: Vec<String>,
    ) -> Result<ApiClient::MutableSubscription, ApiError> {
        self.api_client
            .subscribe2(SubscribeRequest { content_topics })
            .await
    }
}

#[derive(Debug, PartialEq)]
pub struct WelcomeMessage {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) installation_id: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct NewInstallation {
    pub installation_id: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    pub timestamp_ns: u64,
}

#[derive(Debug, PartialEq)]
pub struct RevokeInstallation {
    pub installation_id: Vec<u8>, // TODO: Add proof of revocation
    pub timestamp_ns: u64,
}

#[derive(Debug, PartialEq)]
pub enum IdentityUpdate {
    NewInstallation(NewInstallation),
    RevokeInstallation(RevokeInstallation),
    Invalid,
}

type KeyPackageMap = HashMap<Vec<u8>, Vec<u8>>;

type IdentityUpdatesMap = HashMap<String, Vec<IdentityUpdate>>;

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use futures::Stream;
    use mockall::mock;
    use xmtp_proto::{
        api_client::{
            Error, ErrorKind, MutableApiSubscription, PagingInfo, XmtpApiClient,
            XmtpApiSubscription, XmtpMlsClient,
        },
        xmtp::message_api::v1::{
            cursor::Cursor as InnerCursor, BatchQueryRequest, BatchQueryResponse, Cursor, Envelope,
            IndexCursor, PublishRequest, PublishResponse, QueryRequest, QueryResponse,
            SubscribeRequest,
        },
        xmtp::mls::api::v1::{
            fetch_key_packages_response::KeyPackage,
            get_identity_updates_response::{
                update::Kind as UpdateKind, NewInstallationUpdate, Update, WalletUpdates,
            },
            FetchKeyPackagesRequest, FetchKeyPackagesResponse, GetIdentityUpdatesRequest,
            GetIdentityUpdatesResponse, PublishToGroupRequest, PublishWelcomesRequest,
            RegisterInstallationRequest, RegisterInstallationResponse, UploadKeyPackageRequest,
        },
    };

    use super::ApiClientWrapper;
    use crate::retry::Retry;

    fn build_envelopes(num_envelopes: usize, topic: &str) -> Vec<Envelope> {
        let mut out: Vec<Envelope> = vec![];
        for i in 0..num_envelopes {
            out.push(Envelope {
                content_topic: topic.to_string(),
                message: vec![i as u8],
                timestamp_ns: i as u64,
            })
        }
        out
    }

    mock! {
        pub Subscription {}

        impl XmtpApiSubscription for Subscription {
            fn is_closed(&self) -> bool;
            fn get_messages(&self) -> Vec<Envelope>;
            fn close_stream(&mut self);
        }
    }

    mock! {
        pub MutableSubscription {}

        #[async_trait]
        impl MutableApiSubscription for MutableSubscription {
            async fn update(&mut self, req: SubscribeRequest) -> Result<(), Error>;
            fn close(&self);
        }

        impl Stream for MutableSubscription {
            type Item = Result<Envelope, Error>;

            fn poll_next<'a>(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'a>,
            ) -> std::task::Poll<Option<<Self as Stream>::Item>>;
        }
    }

    // Create a mock XmtpClient for testing the client wrapper
    mock! {
        pub ApiClient {}

        #[async_trait]
        impl XmtpMlsClient for ApiClient {
            type Subscription = MockMutableSubscription;
            async fn register_installation(
                &self,
                request: RegisterInstallationRequest,
            ) -> Result<RegisterInstallationResponse, Error>;
            async fn upload_key_package(&self, request: UploadKeyPackageRequest) -> Result<(), Error>;
            async fn fetch_key_packages(
                &self,
                request: FetchKeyPackagesRequest,
            ) -> Result<FetchKeyPackagesResponse, Error>;
            async fn publish_to_group(&self, request: PublishToGroupRequest) -> Result<(), Error>;
            async fn publish_welcomes(&self, request: PublishWelcomesRequest) -> Result<(), Error>;
            async fn get_identity_updates(
                &self,
                request: GetIdentityUpdatesRequest,
            ) -> Result<GetIdentityUpdatesResponse, Error>;
        }

        #[async_trait]
        impl XmtpApiClient for ApiClient {
            // Need to set an associated type and don't currently need streaming
            // Can figure out a mocked stream type later
            type Subscription = MockSubscription;
            type MutableSubscription = MockMutableSubscription;

            fn set_app_version(&mut self, version: String);

            async fn publish(
                &self,
                token: String,
                request: PublishRequest,
            ) -> Result<PublishResponse, Error>;

            async fn subscribe(&self, request: SubscribeRequest) -> Result<<Self as XmtpApiClient>::Subscription, Error>;

            async fn subscribe2(&self, request: SubscribeRequest) -> Result<<Self as XmtpApiClient>::MutableSubscription, Error>;

            async fn query(&self, request: QueryRequest) -> Result<QueryResponse, Error>;

            async fn batch_query(&self, request: BatchQueryRequest) -> Result<BatchQueryResponse, Error>;
        }


    }

    #[tokio::test]
    async fn test_register_installation() {
        let mut mock_api = MockApiClient::new();
        mock_api.expect_register_installation().returning(move |_| {
            Ok(RegisterInstallationResponse {
                installation_id: vec![1, 2, 3],
            })
        });
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper.register_installation(vec![2, 3, 4]).await.unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_upload_key_package() {
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
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper.upload_key_package(key_package_clone).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_key_packages() {
        let mut mock_api = MockApiClient::new();
        let installation_ids: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6]];
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
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper
            .fetch_key_packages(installation_ids.clone())
            .await
            .unwrap();
        assert_eq!(result.len(), 2);

        for (k, v) in result {
            if k.eq(&installation_ids[0]) {
                assert_eq!(v, vec![7, 8, 9]);
            } else {
                assert_eq!(v, vec![10, 11, 12]);
            }
        }
    }

    #[tokio::test]
    async fn test_get_identity_updates() {
        let mut mock_api = MockApiClient::new();
        let start_time_ns = 12;
        let account_addresses = vec!["wallet1".to_string(), "wallet2".to_string()];
        // account_addresses gets moved below but needs to be used for assertions later
        let account_addresses_clone = account_addresses.clone();
        mock_api
            .expect_get_identity_updates()
            .withf(move |req| {
                req.start_time_ns.eq(&start_time_ns) && req.account_addresses.eq(&account_addresses)
            })
            .returning(move |_| {
                Ok(GetIdentityUpdatesResponse {
                    updates: {
                        vec![
                            WalletUpdates {
                                updates: vec![Update {
                                    timestamp_ns: 1,
                                    kind: Some(UpdateKind::NewInstallation(
                                        NewInstallationUpdate {
                                            installation_id: vec![1, 2, 3],
                                            credential_identity: vec![4, 5, 6],
                                        },
                                    )),
                                }],
                            },
                            WalletUpdates {
                                updates: vec![Update {
                                    timestamp_ns: 2,
                                    kind: Some(UpdateKind::NewInstallation(
                                        NewInstallationUpdate {
                                            installation_id: vec![7, 8, 9],
                                            credential_identity: vec![10, 11, 12],
                                        },
                                    )),
                                }],
                            },
                        ]
                    },
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper
            .get_identity_updates(start_time_ns, account_addresses_clone.clone())
            .await
            .unwrap();
        assert_eq!(result.len(), 2);

        for (k, v) in result {
            if k.eq(&account_addresses_clone[0]) {
                assert_eq!(v.len(), 1);
                assert_eq!(
                    v[0],
                    super::IdentityUpdate::NewInstallation(super::NewInstallation {
                        installation_id: vec![1, 2, 3],
                        credential_bytes: vec![4, 5, 6],
                        timestamp_ns: 1,
                    })
                );
            } else {
                assert_eq!(v.len(), 1);
                assert_eq!(
                    v[0],
                    super::IdentityUpdate::NewInstallation(super::NewInstallation {
                        installation_id: vec![7, 8, 9],
                        credential_bytes: vec![10, 11, 12],
                        timestamp_ns: 2,
                    })
                );
            }
        }
    }

    #[tokio::test]
    async fn test_read_topic_single_page() {
        let mut mock_api = MockApiClient::new();
        let topic = "topic";
        let start_time_ns = 10;
        // Set expectation for first request with no cursor
        mock_api.expect_query().returning(move |req| {
            assert_eq!(req.content_topics[0], topic);

            Ok(QueryResponse {
                paging_info: Some(PagingInfo {
                    cursor: None,
                    limit: 100,
                    direction: 0,
                }),
                envelopes: build_envelopes(10, topic),
            })
        });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper.read_topic(topic, start_time_ns).await.unwrap();
        assert_eq!(result.len(), 10);
    }

    #[tokio::test]
    async fn test_read_topic_single_page_exactly_100_results() {
        let mut mock_api = MockApiClient::new();
        let topic = "topic";
        let start_time_ns = 10;
        // Set expectation for first request with no cursor
        mock_api.expect_query().times(1).returning(move |req| {
            assert_eq!(req.content_topics[0], topic);

            Ok(QueryResponse {
                paging_info: Some(PagingInfo {
                    cursor: None,
                    limit: 100,
                    direction: 0,
                }),
                envelopes: build_envelopes(100, topic),
            })
        });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper.read_topic(topic, start_time_ns).await.unwrap();
        assert_eq!(result.len(), 100);
    }

    #[tokio::test]
    async fn test_read_topic_multi_page() {
        let mut mock_api = MockApiClient::new();
        let topic = "topic";
        let start_time_ns = 10;
        // Set expectation for first request with no cursor
        mock_api
            .expect_query()
            .withf(move |req| match req.paging_info.clone() {
                Some(paging_info) => paging_info.cursor.is_none(),
                None => true,
            } && req.start_time_ns == 10)
            .returning(move |req| {
                assert_eq!(req.content_topics[0], topic);

                Ok(QueryResponse {
                    paging_info: Some(PagingInfo {
                        cursor: Some(Cursor {
                            cursor: Some(InnerCursor::Index(IndexCursor {
                                digest: vec![],
                                sender_time_ns: 0,
                            })),
                        }),
                        limit: 100,
                        direction: 0,
                    }),
                    envelopes: build_envelopes(100, topic),
                })
            });
        // Set expectation for requests with a cursor
        mock_api
            .expect_query()
            .withf(|req| match req.paging_info.clone() {
                Some(paging_info) => paging_info.cursor.is_some(),
                None => false,
            })
            .returning(move |req| {
                assert_eq!(req.content_topics[0], topic);

                Ok(QueryResponse {
                    paging_info: Some(PagingInfo {
                        cursor: None,
                        limit: 100,
                        direction: 0,
                    }),
                    envelopes: build_envelopes(100, topic),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper.read_topic(topic, start_time_ns).await.unwrap();
        assert_eq!(result.len(), 200);
    }

    #[tokio::test]
    async fn it_retries_twice_then_succeeds() {
        let mut mock_api = MockApiClient::new();
        let topic = "topic";
        let start_time_ns = 10;

        mock_api
            .expect_query()
            .times(1)
            .returning(move |_| Err(Error::new(ErrorKind::QueryError)));
        mock_api
            .expect_query()
            .times(1)
            .returning(move |_| Err(Error::new(ErrorKind::QueryError)));
        mock_api.expect_query().times(1).returning(move |_| {
            Ok(QueryResponse {
                paging_info: Some(PagingInfo {
                    cursor: None,
                    limit: 100,
                    direction: 0,
                }),
                envelopes: build_envelopes(10, topic),
            })
        });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper.read_topic(topic, start_time_ns).await.unwrap();
        assert_eq!(result.len(), 10);
    }
}
