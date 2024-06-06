use std::collections::HashMap;

use super::ApiClientWrapper;
use crate::{retry_async, XmtpApi};
use xmtp_proto::api_client::{
    Error as ApiError, ErrorKind, GroupMessageStream, WelcomeMessageStream,
};
use xmtp_proto::xmtp::mls::api::v1::{
    get_identity_updates_response::update::Kind as UpdateKind,
    group_message_input::{Version as GroupMessageInputVersion, V1 as GroupMessageInputV1},
    subscribe_group_messages_request::Filter as GroupFilterProto,
    subscribe_welcome_messages_request::Filter as WelcomeFilterProto,
    FetchKeyPackagesRequest, GetIdentityUpdatesRequest, GroupMessage, GroupMessageInput,
    KeyPackageUpload, PagingInfo, QueryGroupMessagesRequest, QueryWelcomeMessagesRequest,
    RegisterInstallationRequest, SendGroupMessagesRequest, SendWelcomeMessagesRequest,
    SortDirection, SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest,
    UploadKeyPackageRequest, WelcomeMessage, WelcomeMessageInput,
};

/// A filter for querying group messages
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

type IdentityUpdatesMap = HashMap<String, Vec<IdentityUpdate>>;

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpApi,
{
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn query_group_messages(
        &self,
        group_id: Vec<u8>,
        id_cursor: Option<u64>,
    ) -> Result<Vec<GroupMessage>, ApiError> {
        let mut out: Vec<GroupMessage> = vec![];
        let page_size = 100;
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
                                limit: page_size,
                                direction: SortDirection::Ascending as i32,
                            }),
                        })
                        .await
                })
            )?;

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

    pub async fn query_welcome_messages(
        &self,
        installation_id: Vec<u8>,
        id_cursor: Option<u64>,
    ) -> Result<Vec<WelcomeMessage>, ApiError> {
        let mut out: Vec<WelcomeMessage> = vec![];
        let page_size = 100;
        let mut id_cursor = id_cursor;
        loop {
            let mut result = retry_async!(
                self.retry_strategy,
                (async {
                    self.api_client
                        .query_welcome_messages(QueryWelcomeMessagesRequest {
                            installation_key: installation_id.clone(),
                            paging_info: Some(PagingInfo {
                                id_cursor: id_cursor.unwrap_or(0),
                                limit: page_size,
                                direction: SortDirection::Ascending as i32,
                            }),
                        })
                        .await
                })
            )?;

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

    /// Register an XMTP KeyPackage with the network.
    /// New InboxID clients should set `is_inbox_id_credential` to true.
    /// V3 clients should have `is_inbox_id_credential` to `false`.
    /// Not indicating your client version will result in validation failure.
    pub async fn register_installation(
        &self,
        key_package: Vec<u8>,
        is_inbox_id_credential: bool,
    ) -> Result<Vec<u8>, ApiError> {
        let res = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .register_installation(RegisterInstallationRequest {
                        key_package: Some(KeyPackageUpload {
                            key_package_tls_serialized: key_package.to_vec(),
                        }),
                        is_inbox_id_credential,
                    })
                    .await
            })
        )?;

        Ok(res.installation_key)
    }

    /// Upload a KeyPackage to the network
    /// New InboxID clients should set `is_inbox_id_credential` to true.
    /// V3 clients should have `is_inbox_id_credential` to `false`.
    /// Not indicating your client version will result in validation failure.
    pub async fn upload_key_package(
        &self,
        key_package: Vec<u8>,
        is_inbox_id_credential: bool,
    ) -> Result<(), ApiError> {
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
        )?;

        Ok(())
    }

    pub async fn fetch_key_packages(
        &self,
        installation_keys: Vec<Vec<u8>>,
    ) -> Result<KeyPackageMap, ApiError> {
        let res = retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .fetch_key_packages(FetchKeyPackagesRequest {
                        installation_keys: installation_keys.clone(),
                    })
                    .await
            })
        )?;

        if res.key_packages.len() != installation_keys.len() {
            println!("mismatched number of results");
            return Err(ApiError::new(ErrorKind::MlsError));
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

    pub async fn send_welcome_messages(
        &self,
        messages: &[WelcomeMessageInput],
    ) -> Result<(), ApiError> {
        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .send_welcome_messages(SendWelcomeMessagesRequest {
                        messages: messages.to_vec(),
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
                                    installation_key: new_installation.installation_key,
                                    credential_bytes: new_installation.credential_identity,
                                })
                            }
                            Some(UpdateKind::RevokedInstallation(revoke_installation)) => {
                                IdentityUpdate::RevokeInstallation(RevokeInstallation {
                                    timestamp_ns: update.timestamp_ns,
                                    installation_key: revoke_installation.installation_key,
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

    pub async fn send_group_messages(&self, group_messages: Vec<&[u8]>) -> Result<(), ApiError> {
        let to_send: Vec<GroupMessageInput> = group_messages
            .iter()
            .map(|msg| GroupMessageInput {
                version: Some(GroupMessageInputVersion::V1(GroupMessageInputV1 {
                    data: msg.to_vec(),
                    sender_hmac: vec![],
                })),
            })
            .collect();

        retry_async!(
            self.retry_strategy,
            (async {
                self.api_client
                    .send_group_messages(SendGroupMessagesRequest {
                        messages: to_send.clone(),
                    })
                    .await
            })
        )?;

        Ok(())
    }

    pub async fn subscribe_group_messages(
        &self,
        filters: Vec<GroupFilter>,
    ) -> Result<GroupMessageStream, ApiError> {
        self.api_client
            .subscribe_group_messages(SubscribeGroupMessagesRequest {
                filters: filters.into_iter().map(|f| f.into()).collect(),
            })
            .await
    }

    pub async fn subscribe_welcome_messages(
        &self,
        installation_key: Vec<u8>,
        id_cursor: Option<u64>,
    ) -> Result<WelcomeMessageStream, ApiError> {
        self.api_client
            .subscribe_welcome_messages(SubscribeWelcomeMessagesRequest {
                filters: vec![WelcomeFilterProto {
                    installation_key,
                    id_cursor: id_cursor.unwrap_or(0),
                }],
            })
            .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::test_utils::*;
    use super::super::*;

    use xmtp_proto::{
        api_client::{Error, ErrorKind},
        xmtp::mls::api::v1::{
            fetch_key_packages_response::KeyPackage,
            get_identity_updates_response::{
                update::Kind as UpdateKind, NewInstallationUpdate, Update, WalletUpdates,
            },
            FetchKeyPackagesResponse, GetIdentityUpdatesResponse, PagingInfo,
            QueryGroupMessagesResponse, RegisterInstallationResponse,
        },
    };

    #[tokio::test]
    async fn test_register_installation() {
        let mut mock_api = MockApiClient::new();
        mock_api.expect_register_installation().returning(move |_| {
            Ok(RegisterInstallationResponse {
                installation_key: vec![1, 2, 3],
            })
        });
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper
            .register_installation(vec![2, 3, 4], false)
            .await
            .unwrap();
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
        let result = wrapper.upload_key_package(key_package_clone, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_key_packages() {
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
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
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
                                            installation_key: vec![1, 2, 3],
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
                                            installation_key: vec![7, 8, 9],
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
                        installation_key: vec![1, 2, 3],
                        credential_bytes: vec![4, 5, 6],
                        timestamp_ns: 1,
                    })
                );
            } else {
                assert_eq!(v.len(), 1);
                assert_eq!(
                    v[0],
                    super::IdentityUpdate::NewInstallation(super::NewInstallation {
                        installation_key: vec![7, 8, 9],
                        credential_bytes: vec![10, 11, 12],
                        timestamp_ns: 2,
                    })
                );
            }
        }
    }

    #[tokio::test]
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

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);
    }

    #[tokio::test]
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

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 100);
    }

    #[tokio::test]
    async fn test_read_topic_multi_page() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3, 4];
        let group_id_clone = group_id.clone();
        let group_id_clone2 = group_id.clone();
        // Set expectation for first request with no cursor
        mock_api
            .expect_query_group_messages()
            .withf(move |req| match req.paging_info.clone() {
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
            .withf(|req| match req.paging_info.clone() {
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

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper
            .query_group_messages(group_id_clone2, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 200);
    }

    #[tokio::test]
    async fn it_retries_twice_then_succeeds() {
        let mut mock_api = MockApiClient::new();
        let group_id = vec![1, 2, 3];
        let group_id_clone = group_id.clone();

        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| Err(Error::new(ErrorKind::QueryError)));
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| Err(Error::new(ErrorKind::QueryError)));
        mock_api
            .expect_query_group_messages()
            .times(1)
            .returning(move |_| {
                Ok(QueryGroupMessagesResponse {
                    paging_info: None,
                    messages: build_group_messages(50, group_id.clone()),
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let result = wrapper
            .query_group_messages(group_id_clone, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 50);
    }
}
