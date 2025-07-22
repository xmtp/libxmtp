use std::collections::HashMap;

use super::ApiClientWrapper;
use crate::ApiError;
use crate::Result;
use futures::future::try_join_all;
use xmtp_common::RetryableError;
use xmtp_proto::prelude::XmtpIdentityClient;
use xmtp_proto::xmtp::identity::api::v1::{
    get_identity_updates_request::Request as GetIdentityUpdatesV2RequestProto,
    get_identity_updates_response::IdentityUpdateLog,
    get_inbox_ids_request::Request as GetInboxIdsRequestProto,
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request, GetInboxIdsRequest,
    PublishIdentityUpdateRequest,
};

use xmtp_proto::xmtp::identity::api::v1::{
    VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
};
use xmtp_proto::xmtp::identity::associations::{IdentifierKind, IdentityUpdate};

const GET_IDENTITY_UPDATES_CHUNK_SIZE: usize = 50;

#[derive(Debug)]
/// A filter for querying identity updates. `sequence_id` is the starting sequence, and only later updates will be returned.
pub struct GetIdentityUpdatesV2Filter {
    pub inbox_id: String,
    pub sequence_id: Option<u64>,
}

impl From<&GetIdentityUpdatesV2Filter> for GetIdentityUpdatesV2RequestProto {
    fn from(filter: &GetIdentityUpdatesV2Filter) -> Self {
        Self {
            inbox_id: filter.inbox_id.clone(),
            sequence_id: filter.sequence_id.unwrap_or(0),
        }
    }
}

/// Maps account addresses to inbox IDs. If no inbox ID found, the value will be None
type IdentifierToInboxIdMap = HashMap<ApiIdentifier, String>;
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct ApiIdentifier {
    pub identifier: String,
    pub identifier_kind: IdentifierKind,
}

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpIdentityClient,
{
    pub async fn publish_identity_update<U: Into<IdentityUpdate>>(&self, update: U) -> Result<()> {
        let update: IdentityUpdate = update.into();
        self.api_client
            .publish_identity_update(PublishIdentityUpdateRequest {
                identity_update: Some(update),
            })
            .await
            .map_err(crate::dyn_err)?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all, fields(len = filters.len()))]
    pub async fn get_identity_updates_v2<T>(
        &self,
        filters: Vec<GetIdentityUpdatesV2Filter>,
    ) -> Result<impl Iterator<Item = (String, Vec<T>)>>
    where
        T: TryFrom<IdentityUpdateLog>,
        <T as TryFrom<IdentityUpdateLog>>::Error: RetryableError + Send + Sync + 'static,
    {
        let chunks = filters.chunks(GET_IDENTITY_UPDATES_CHUNK_SIZE);

        let res = try_join_all(chunks.map(|chunk| async move {
            let res = self
                .api_client
                .get_identity_updates_v2(GetIdentityUpdatesV2Request {
                    requests: chunk.iter().map(|filter| filter.into()).collect(),
                })
                .await
                .map_err(crate::dyn_err)?
                .responses
                .into_iter()
                .map(|item| {
                    let deser_items = item
                        .updates
                        .into_iter()
                        .map(move |update| update.try_into().map_err(crate::dyn_err))
                        .collect::<Result<Vec<_>>>()?;
                    Ok::<_, ApiError>((item.inbox_id, deser_items))
                });
            Ok::<_, ApiError>(res)
        }))
        .await?
        .into_iter()
        .flatten()
        .collect::<Result<Vec<(String, Vec<T>)>>>()?
        .into_iter();

        Ok(res)
    }

    #[tracing::instrument(level = "trace", skip_all, fields(len = account_identifiers.len()))]
    pub async fn get_inbox_ids(
        &self,
        account_identifiers: Vec<ApiIdentifier>,
    ) -> Result<IdentifierToInboxIdMap> {
        tracing::info!(
            "Getting inbox_ids for account identities: {:?}",
            &account_identifiers
        );
        let requests = account_identifiers
            .into_iter()
            .map(|r| GetInboxIdsRequestProto {
                identifier: r.identifier,
                identifier_kind: r.identifier_kind as i32,
            })
            .collect();

        let result = self
            .api_client
            .get_inbox_ids(GetInboxIdsRequest { requests })
            .await
            .map_err(crate::dyn_err)?;

        Ok(result
            .responses
            .into_iter()
            .filter_map(|resp| {
                let kind = match resp.identifier_kind() {
                    IdentifierKind::Unspecified => IdentifierKind::Ethereum,
                    kind => kind,
                };
                Some((
                    ApiIdentifier {
                        identifier_kind: kind,
                        identifier: resp.identifier,
                    },
                    resp.inbox_id?,
                ))
            })
            .collect())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn verify_smart_contract_wallet_signatures(
        &self,
        request: VerifySmartContractWalletSignaturesRequest,
    ) -> Result<VerifySmartContractWalletSignaturesResponse> {
        self.api_client
            .verify_smart_contract_wallet_signatures(request)
            .await
            .map_err(crate::dyn_err)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::super::test_utils::*;
    use super::GetIdentityUpdatesV2Filter;
    use crate::{identity::ApiIdentifier, ApiClientWrapper};
    use std::collections::HashMap;
    use xmtp_common::rand_hexstring;
    use xmtp_id::associations::unverified::UnverifiedIdentityUpdate;
    use xmtp_proto::xmtp::identity::{
        api::v1::{
            get_identity_updates_response::{
                IdentityUpdateLog, Response as GetIdentityUpdatesResponseItem,
            },
            get_inbox_ids_response::Response as GetInboxIdsResponseItem,
            GetIdentityUpdatesResponse, GetInboxIdsResponse, PublishIdentityUpdateResponse,
        },
        associations::IdentifierKind,
    };

    fn create_identity_update(inbox_id: String) -> UnverifiedIdentityUpdate {
        UnverifiedIdentityUpdate::new_test(
            // TODO:nm Add default actions
            vec![],
            inbox_id,
        )
    }

    #[xmtp_common::test]
    async fn publish_identity_update() {
        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_hexstring();
        let identity_update = create_identity_update(inbox_id.clone());

        mock_api
            .expect_publish_identity_update()
            .withf(move |req| req.identity_update.as_ref().unwrap().inbox_id.eq(&inbox_id))
            .returning(move |_| Ok(PublishIdentityUpdateResponse {}));

        let wrapper = ApiClientWrapper::new(mock_api, exponential().build());
        let result = wrapper.publish_identity_update(identity_update).await;

        assert!(result.is_ok());
    }

    #[xmtp_common::test]
    async fn get_identity_update_v2() {
        pub struct InboxIdentityUpdate {
            inbox_id: String,
        }
        impl TryFrom<IdentityUpdateLog> for InboxIdentityUpdate {
            type Error = crate::ApiError;
            fn try_from(v: IdentityUpdateLog) -> Result<InboxIdentityUpdate, Self::Error> {
                Ok(InboxIdentityUpdate {
                    inbox_id: v.update.unwrap().inbox_id,
                })
            }
        }

        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_hexstring();
        let inbox_id_clone = inbox_id.clone();
        let inbox_id_clone_2 = inbox_id.clone();
        mock_api
            .expect_get_identity_updates_v2()
            .withf(move |req| req.requests.first().unwrap().inbox_id.eq(&inbox_id))
            .returning(move |_| {
                let identity_update = create_identity_update(inbox_id_clone.clone());
                Ok(GetIdentityUpdatesResponse {
                    responses: vec![GetIdentityUpdatesResponseItem {
                        inbox_id: inbox_id_clone.clone(),
                        updates: vec![IdentityUpdateLog {
                            sequence_id: 1,
                            server_timestamp_ns: 1,
                            update: Some(identity_update.into()),
                        }],
                    }],
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, exponential().build());
        let result = wrapper
            .get_identity_updates_v2(vec![GetIdentityUpdatesV2Filter {
                inbox_id: inbox_id_clone_2.clone(),
                sequence_id: None,
            }])
            .await
            .expect("should work")
            .collect::<HashMap<_, Vec<InboxIdentityUpdate>>>();

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&inbox_id_clone_2).unwrap().len(), 1);
        assert_eq!(
            result
                .get(&inbox_id_clone_2)
                .unwrap()
                .first()
                .unwrap()
                .inbox_id,
            inbox_id_clone_2
        );
    }

    #[xmtp_common::test]
    async fn get_inbox_ids() {
        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_hexstring();
        let inbox_id_clone = inbox_id.clone();
        let inbox_id_clone_2 = inbox_id.clone();
        let address = rand_hexstring();
        let address_clone = address.clone();
        let address_clone_2 = address.clone();

        mock_api
            .expect_get_inbox_ids()
            .withf(move |req| req.requests.first().unwrap().identifier.eq(&address_clone))
            .returning(move |_| {
                Ok(GetInboxIdsResponse {
                    responses: vec![GetInboxIdsResponseItem {
                        identifier: address_clone_2.clone(),
                        identifier_kind: IdentifierKind::Ethereum as i32,
                        inbox_id: Some(inbox_id_clone.clone()),
                    }],
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, exponential().build());
        let result = wrapper
            .get_inbox_ids(vec![ApiIdentifier {
                identifier: address.clone(),
                identifier_kind: IdentifierKind::Ethereum,
            }])
            .await
            .expect("should work");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result
                .get(&ApiIdentifier {
                    identifier: address,
                    identifier_kind: IdentifierKind::Ethereum
                })
                .unwrap(),
            &inbox_id_clone_2
        );
    }
}
