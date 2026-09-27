use crate::{ContentTypeId, ConversationID, XmtpError, client::CoreClient};
use xmtp_mls::subscriptions::internal::InternalEvent;
use xmtp_mls::{client::ClientError, mls_store::MlsStoreError};

use super::EventKind;

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct EventFilter {
    pub kinds: Vec<EventKind>,
    pub conversation_ids: Option<Vec<ConversationID>>,
    pub content_types: Option<Vec<ContentTypeId>>,
    pub references_own_messages: bool,
}

impl EventFilter {
    pub(crate) fn to_core(
        &self,
        client: &CoreClient,
    ) -> Result<xmtp_events::EventFilter<InternalEvent>, XmtpError> {
        let mut filter = xmtp_events::EventFilter::new(self.kinds.iter().copied().map(Into::into));
        if let Some(ids) = &self.conversation_ids {
            let mut group_ids = Vec::with_capacity(ids.len());
            for id in ids {
                let group_id: xmtp_proto::types::GroupId = id.clone().try_into()?;
                match client.group(&group_id) {
                    Ok(group) => {
                        if let Some(dm_id) = group.dm_id {
                            filter.dm_identifiers.push(dm_id.into_bytes());
                        }
                    }
                    Err(ClientError::MlsStore(MlsStoreError::NotFound(_))) => {}
                    Err(error) => return Err(XmtpError::from_client(error)),
                }
                group_ids.push(group_id.as_slice().to_vec());
            }
            filter.group_ids = Some(group_ids);
        }
        filter.content_types = self.content_types.as_ref().map(|ids| {
            ids.iter()
                .map(|id| xmtp_events::ContentTypeId {
                    authority_id: id.authority_id.clone(),
                    type_id: id.type_id.clone(),
                    version_major: id.version_major,
                })
                .collect()
        });
        filter.references_own_messages = self.references_own_messages;
        Ok(filter)
    }
}
