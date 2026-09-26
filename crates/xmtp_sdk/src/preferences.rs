use std::sync::Arc;

use crate::{XmtpError, client::CoreClient, conversation::on_sdk_worker};
use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ConsentState {
    Unknown,
    Allowed,
    Denied,
}

impl From<ConsentState> for xmtp_db::consent_record::ConsentState {
    fn from(value: ConsentState) -> Self {
        match value {
            ConsentState::Unknown => Self::Unknown,
            ConsentState::Allowed => Self::Allowed,
            ConsentState::Denied => Self::Denied,
        }
    }
}

impl From<xmtp_db::consent_record::ConsentState> for ConsentState {
    fn from(value: xmtp_db::consent_record::ConsentState) -> Self {
        match value {
            xmtp_db::consent_record::ConsentState::Unknown => Self::Unknown,
            xmtp_db::consent_record::ConsentState::Allowed => Self::Allowed,
            xmtp_db::consent_record::ConsentState::Denied => Self::Denied,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ConsentEntity {
    Conversation {
        conversation_id: crate::ConversationID,
    },
    Inbox {
        inbox_id: crate::InboxID,
    },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConsentRecord {
    pub entity: ConsentEntity,
    pub state: ConsentState,
}

#[derive(uniffi::Object)]
pub struct Preferences {
    pub(crate) client: Arc<CoreClient>,
}

#[xmtp_macro::sdk_export]
impl Preferences {
    pub async fn sync(&self) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .sync_all_welcomes_and_groups(None)
                    .await
                    .map_err(XmtpError::unknown)?;
                Ok(())
            }),
        )
        .await
    }

    pub async fn set_consent_states(&self, records: Vec<ConsentRecord>) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let records = records
                .into_iter()
                .map(|record| {
                    let (entity_type, entity) = match record.entity {
                        ConsentEntity::Conversation { conversation_id } => {
                            (ConsentType::ConversationId, conversation_id.0)
                        }
                        ConsentEntity::Inbox { inbox_id } => (ConsentType::InboxId, inbox_id.0),
                    };
                    StoredConsentRecord {
                        entity_type,
                        entity,
                        state: record.state.into(),
                        consented_at_ns: xmtp_common::time::now_ns(),
                    }
                })
                .collect::<Vec<_>>();
            client
                .set_consent_states(&records)
                .await
                .map_err(XmtpError::from_client)
        })
        .await
    }

    pub async fn consent_state(&self, entity: ConsentEntity) -> Result<ConsentState, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let (kind, value) = match entity {
                ConsentEntity::Conversation { conversation_id } => {
                    (ConsentType::ConversationId, conversation_id.0)
                }
                ConsentEntity::Inbox { inbox_id } => (ConsentType::InboxId, inbox_id.0),
            };
            client
                .get_consent_state(kind, value)
                .await
                .map(Into::into)
                .map_err(XmtpError::from_client)
        })
        .await
    }
}
