use crate::{
    V3Client,
    protocol::{XmtpEnvelope, XmtpQuery},
    v3::{FetchKeyPackages, GetIdentityUpdatesV2, QueryGroupMessages, QueryWelcomeMessages},
};
use xmtp_common::RetryableError;
use xmtp_configuration::MAX_PAGE_SIZE;
use xmtp_proto::identity_v1::{
    get_identity_updates_request, get_identity_updates_response::IdentityUpdateLog,
};
use xmtp_proto::{
    api::{ApiClientError, Client, EndpointExt, Query},
    mls_v1::{PagingInfo, SortDirection},
    types::{GlobalCursor, Topic, TopicKind},
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, E> XmtpQuery for V3Client<C>
where
    C: Client<Error = E>,
    E: std::error::Error + RetryableError + Send + Sync,
    ApiClientError<E>: From<ApiClientError<<C as Client>::Error>> + Send + Sync + 'static,
{
    type Error = ApiClientError<E>;
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        use TopicKind::*;
        match topic.kind() {
            GroupMessagesV1 => {
                let id_cursor = at.map(|c| c.v3_message()).unwrap_or(0);
                let result = QueryGroupMessages::builder()
                    .group_id(topic.identifier())
                    .paging_info(PagingInfo {
                        direction: SortDirection::Ascending as i32,
                        limit: MAX_PAGE_SIZE,
                        id_cursor,
                    })
                    .build()?
                    .v3_paged(Some(id_cursor))
                    .query(&self.client)
                    .await?;
                Ok(XmtpEnvelope::new(result))
            }
            WelcomeMessagesV1 => {
                let id_cursor = at.map(|c| c.v3_welcome()).unwrap_or(0);
                let result = QueryWelcomeMessages::builder()
                    .installation_key(topic.identifier())
                    .paging_info(PagingInfo {
                        direction: SortDirection::Ascending as i32,
                        limit: MAX_PAGE_SIZE,
                        id_cursor,
                    })
                    .build()?
                    .v3_paged(Some(id_cursor))
                    .query(&self.client)
                    .await?;
                Ok(XmtpEnvelope::new(result))
            }
            IdentityUpdatesV1 => {
                let result = GetIdentityUpdatesV2::builder()
                    .request(get_identity_updates_request::Request {
                        inbox_id: hex::encode(topic.identifier()),
                        sequence_id: at.map(|c| c.inbox_log()).unwrap_or(0),
                    })
                    .build()?
                    .query(&self.client)
                    .await?;
                let updates: Vec<IdentityUpdateLog> = result
                    .responses
                    .into_iter()
                    .flat_map(|r| r.updates)
                    .collect();
                Ok(XmtpEnvelope::new(updates))
            }
            KeyPackagesV1 => {
                let result = FetchKeyPackages::builder()
                    .installation_key(topic.identifier().to_vec())
                    .build()?
                    .query(&self.client)
                    .await?;
                Ok(XmtpEnvelope::new(result.key_packages))
            }
            _ => unreachable!(),
        }
    }
}
