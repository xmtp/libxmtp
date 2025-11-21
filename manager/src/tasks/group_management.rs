use anyhow::Result;
use xmtp_common::time::now_ns;
use xmtp_db::{
    EncryptedMessageStore, NativeDb,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
};

pub fn enable_group(store: &EncryptedMessageStore<NativeDb>, group_id: &[u8]) -> Result<()> {
    store
        .db()
        .insert_newer_consent_record(StoredConsentRecord {
            consented_at_ns: now_ns(),
            entity: hex::encode(group_id),
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Allowed,
        })?;
    Ok(())
}

pub fn disable_group(store: &EncryptedMessageStore<NativeDb>, group_id: &[u8]) -> Result<()> {
    store
        .db()
        .insert_newer_consent_record(StoredConsentRecord {
            consented_at_ns: now_ns(),
            entity: hex::encode(group_id),
            entity_type: ConsentType::ConversationId,
            state: ConsentState::Denied,
        })?;

    Ok(())
}
