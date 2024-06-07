use std::collections::HashMap;

use super::{ApiClientWrapper, WrappedApiError};
use crate::XmtpApi;
use futures::future::try_join_all;
use xmtp_id::{
    associations::{DeserializationError, IdentityUpdate},
    InboxId,
};
use xmtp_proto::xmtp::identity::api::v1::{
    get_identity_updates_request::Request as GetIdentityUpdatesV2RequestProto,
    get_identity_updates_response::IdentityUpdateLog,
    get_inbox_ids_request::Request as GetInboxIdsRequestProto,
    GetIdentityUpdatesRequest as GetIdentityUpdatesV2Request, GetIdentityUpdatesResponse,
    GetInboxIdsRequest, PublishIdentityUpdateRequest,
};

const GET_IDENTITY_UPDATES_CHUNK_SIZE: usize = 50;

/// A filter for querying identity updates. `sequence_id` is the starting sequence, and only later updates will be returned.
pub struct GetIdentityUpdatesV2Filter {
    pub inbox_id: InboxId,
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

#[derive(Clone)]
pub struct InboxUpdate {
    pub sequence_id: u64,
    pub server_timestamp_ns: u64,
    pub update: IdentityUpdate,
}

impl TryFrom<IdentityUpdateLog> for InboxUpdate {
    type Error = DeserializationError;

    fn try_from(update: IdentityUpdateLog) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence_id: update.sequence_id,
            server_timestamp_ns: update.server_timestamp_ns,
            update: update
                .update
                .ok_or(DeserializationError::MissingUpdate)?
                // TODO: Figure out what to do with requests that don't deserialize correctly. Maybe we want to just filter them out?,
                .try_into()?,
        })
    }
}

/// A mapping of `inbox_id` -> Vec<InboxUpdate>
type InboxUpdateMap = HashMap<InboxId, Vec<InboxUpdate>>;

/// Maps account addresses to inbox IDs. If no inbox ID found, the value will be None
type AddressToInboxIdMap = HashMap<String, InboxId>;

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub async fn publish_identity_update(
        &self,
        update: IdentityUpdate,
    ) -> Result<(), WrappedApiError> {
        self.api_client
            .publish_identity_update(PublishIdentityUpdateRequest {
                identity_update: Some(update.into()),
            })
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_identity_updates_v2(
        &self,
        filters: Vec<GetIdentityUpdatesV2Filter>,
    ) -> Result<InboxUpdateMap, WrappedApiError> {
        let chunks = filters.chunks(GET_IDENTITY_UPDATES_CHUNK_SIZE);

        let chunked_results: Result<Vec<GetIdentityUpdatesResponse>, WrappedApiError> =
            try_join_all(chunks.map(|chunk| async move {
                let result = self
                    .api_client
                    .get_identity_updates_v2(GetIdentityUpdatesV2Request {
                        requests: chunk.iter().map(|filter| filter.into()).collect(),
                    })
                    .await?;

                Ok(result)
            }))
            .await;

        let inbox_map = chunked_results?
            .into_iter()
            .flat_map(|response| {
                response.responses.into_iter().map(|item| {
                    let deserialized_updates = item
                        .updates
                        .into_iter()
                        .map(|update| update.try_into().map_err(WrappedApiError::from))
                        .collect::<Result<Vec<InboxUpdate>, WrappedApiError>>()?;

                    Ok((item.inbox_id, deserialized_updates))
                })
            })
            .collect::<Result<InboxUpdateMap, WrappedApiError>>()?;

        Ok(inbox_map)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_inbox_ids(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<AddressToInboxIdMap, WrappedApiError> {
        log::info!("Asked for account addresses: {:?}", &account_addresses);
        let result = self
            .api_client
            .get_inbox_ids(GetInboxIdsRequest {
                requests: account_addresses
                    .into_iter()
                    .map(|address| GetInboxIdsRequestProto { address })
                    .collect(),
            })
            .await?;

        Ok(result
            .responses
            .into_iter()
            .filter(|inbox_id| inbox_id.inbox_id.is_some())
            .map(|inbox_id| (inbox_id.address, inbox_id.inbox_id.unwrap()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::*;
    use super::GetIdentityUpdatesV2Filter;
    use crate::{api::ApiClientWrapper, retry::Retry};
    use xmtp_id::associations::{test_utils::rand_string, Action, CreateInbox, IdentityUpdate};
    use xmtp_proto::xmtp::identity::api::v1::{
        get_identity_updates_response::{
            IdentityUpdateLog, Response as GetIdentityUpdatesResponseItem,
        },
        get_inbox_ids_response::Response as GetInboxIdsResponseItem,
        GetIdentityUpdatesResponse, GetInboxIdsResponse, PublishIdentityUpdateResponse,
    };

    fn create_identity_update(inbox_id: String) -> IdentityUpdate {
        IdentityUpdate::new_test(vec![Action::CreateInbox(CreateInbox::default())], inbox_id)
    }

    #[tokio::test]
    async fn publish_identity_update() {
        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_string();
        let identity_update = create_identity_update(inbox_id.clone());

        mock_api
            .expect_publish_identity_update()
            .withf(move |req| req.identity_update.as_ref().unwrap().inbox_id.eq(&inbox_id))
            .returning(move |_| Ok(PublishIdentityUpdateResponse {}));

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper.publish_identity_update(identity_update).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_identity_update_v2() {
        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_string();
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
                            update: Some(identity_update.to_proto()),
                        }],
                    }],
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper
            .get_identity_updates_v2(vec![GetIdentityUpdatesV2Filter {
                inbox_id: inbox_id_clone_2.clone(),
                sequence_id: None,
            }])
            .await
            .expect("should work");

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&inbox_id_clone_2).unwrap().len(), 1);
        assert_eq!(
            result
                .get(&inbox_id_clone_2)
                .unwrap()
                .first()
                .unwrap()
                .update
                .inbox_id,
            inbox_id_clone_2
        );
    }

    #[tokio::test]
    async fn get_inbox_ids() {
        let mut mock_api = MockApiClient::new();
        let inbox_id = rand_string();
        let inbox_id_clone = inbox_id.clone();
        let inbox_id_clone_2 = inbox_id.clone();
        let address = rand_string();
        let address_clone = address.clone();
        let address_clone_2 = address.clone();

        mock_api
            .expect_get_inbox_ids()
            .withf(move |req| req.requests.first().unwrap().address.eq(&address_clone))
            .returning(move |_| {
                Ok(GetInboxIdsResponse {
                    responses: vec![GetInboxIdsResponseItem {
                        address: address_clone_2.clone(),
                        inbox_id: Some(inbox_id_clone.clone()),
                    }],
                })
            });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let result = wrapper
            .get_inbox_ids(vec![address.clone()])
            .await
            .expect("should work");

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&address).unwrap(), &inbox_id_clone_2);
    }
}
