use super::*;
use crate::storage::{
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    schema::consent_records,
};
use diesel::prelude::*;
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element,
    consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
};

impl BackupRecordProvider for ConsentSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let query = consent_records::table
            .order_by((consent_records::entity_type, consent_records::entity))
            .limit(Self::BATCH_SIZE)
            .offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredConsentRecord>(conn))
            .expect("Failed to load consent records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::Consent(record.into())),
            })
            .collect()
    }
}

impl TryFrom<ConsentSave> for StoredConsentRecord {
    type Error = DeserializationError;
    fn try_from(value: ConsentSave) -> Result<Self, Self::Error> {
        let entity_type = value.entity_type().try_into()?;
        let state = value.state().try_into()?;

        Ok(Self {
            entity_type,
            state,
            entity: value.entity,
        })
    }
}
impl TryFrom<ConsentTypeSave> for ConsentType {
    type Error = DeserializationError;
    fn try_from(value: ConsentTypeSave) -> Result<Self, Self::Error> {
        Ok(match value {
            ConsentTypeSave::InboxId => Self::InboxId,
            ConsentTypeSave::ConversationId => Self::ConversationId,
            ConsentTypeSave::Address => return Err(DeserializationError::Deprecated("address")),
            ConsentTypeSave::Unspecified => {
                return Err(DeserializationError::Unspecified("consent_type"))
            }
        })
    }
}
impl TryFrom<ConsentStateSave> for ConsentState {
    type Error = DeserializationError;
    fn try_from(value: ConsentStateSave) -> Result<Self, Self::Error> {
        Ok(match value {
            ConsentStateSave::Allowed => Self::Allowed,
            ConsentStateSave::Denied => Self::Denied,
            ConsentStateSave::Unknown => Self::Unknown,
            ConsentStateSave::Unspecified => {
                return Err(DeserializationError::Unspecified("consent_state"))
            }
        })
    }
}
impl From<StoredConsentRecord> for ConsentSave {
    fn from(value: StoredConsentRecord) -> Self {
        let entity_type: ConsentTypeSave = value.entity_type.into();
        let state: ConsentStateSave = value.state.into();

        Self {
            entity_type: entity_type as i32,
            state: state as i32,
            entity: value.entity,
        }
    }
}
impl From<ConsentType> for ConsentTypeSave {
    fn from(value: ConsentType) -> Self {
        match value {
            ConsentType::InboxId => Self::InboxId,
            ConsentType::ConversationId => Self::ConversationId,
        }
    }
}
impl From<ConsentState> for ConsentStateSave {
    fn from(value: ConsentState) -> Self {
        match value {
            ConsentState::Allowed => Self::Allowed,
            ConsentState::Denied => Self::Denied,
            ConsentState::Unknown => Self::Unknown,
        }
    }
}
