use anyhow::Result;
use xmtp_common::time::now_ns;
use xmtp_db::{
    EncryptedMessageStore, NativeDb,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord},
};

pub fn enable_groups(store: &EncryptedMessageStore<NativeDb>, group_ids: &[Vec<u8>]) -> Result<()> {
    for group_id in group_ids {
        store
            .db()
            .insert_newer_consent_record(StoredConsentRecord {
                consented_at_ns: now_ns(),
                entity: hex::encode(group_id),
                entity_type: ConsentType::ConversationId,
                state: ConsentState::Allowed,
            })?;
    }

    Ok(())
}

pub fn disable_groups(
    store: &EncryptedMessageStore<NativeDb>,
    group_ids: &[Vec<u8>],
) -> Result<()> {
    for group_id in group_ids {
        store
            .db()
            .insert_newer_consent_record(StoredConsentRecord {
                consented_at_ns: now_ns(),
                entity: hex::encode(group_id),
                entity_type: ConsentType::ConversationId,
                state: ConsentState::Denied,
            })?;
    }

    Ok(())
}
