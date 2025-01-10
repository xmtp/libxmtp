use super::*;
use crate::storage::{
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    schema::consent_records,
};
use diesel::prelude::*;
use xmtp_proto::xmtp::device_sync::consent_backup::{
    ConsentRecordSave, ConsentStateSave, ConsentTypeSave,
};

impl BackupRecordProvider for ConsentRecordSave {
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
            .raw_query(|conn| query.load::<StoredConsentRecord>(conn))
            .expect("Failed to load consent records");

        batch
            .into_iter()
            .map(|record| BackupElement::Consent(record.into()))
            .collect()
    }
}

impl From<ConsentRecordSave> for StoredConsentRecord {
    fn from(value: ConsentRecordSave) -> Self {
        let entity_type = value.entity_type().into();
        let state = value.state().into();
        Self {
            entity_type,
            state,
            entity: value.entity,
        }
    }
}
impl From<ConsentTypeSave> for ConsentType {
    fn from(value: ConsentTypeSave) -> Self {
        match value {
            ConsentTypeSave::Address => Self::Address,
            ConsentTypeSave::InboxId => Self::InboxId,
            ConsentTypeSave::ConversationId => Self::ConversationId,
        }
    }
}
impl From<ConsentStateSave> for ConsentState {
    fn from(value: ConsentStateSave) -> Self {
        match value {
            ConsentStateSave::Allowed => Self::Allowed,
            ConsentStateSave::Denied => Self::Denied,
            ConsentStateSave::Unknown => Self::Unknown,
        }
    }
}

impl From<StoredConsentRecord> for ConsentRecordSave {
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
            ConsentType::Address => Self::Address,
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
