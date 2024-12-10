use serde::{Deserialize, Serialize};
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto;

use crate::storage::consent_record::StoredConsentRecord;

#[derive(Serialize, Deserialize, Clone)]
#[repr(i32)]
pub enum UserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord) = 1,
    HmacKeyUpdate { key: Vec<u8> } = 2,
}

impl TryFrom<UserPreferenceUpdateProto> for UserPreferenceUpdate {
    type Error = DeserializationError;
    fn try_from(value: UserPreferenceUpdateProto) -> Result<Self, Self::Error> {
        let update =
            bincode::deserialize(&value.content).map_err(|_| DeserializationError::Bincode)?;

        Ok(update)
    }
}

impl TryInto<UserPreferenceUpdateProto> for UserPreferenceUpdate {
    type Error = bincode::Error;

    fn try_into(self) -> Result<UserPreferenceUpdateProto, Self::Error> {
        let content = bincode::serialize(&self)?;
        Ok(UserPreferenceUpdateProto { content })
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::consent_record::{ConsentState, ConsentType};

    use super::*;

    #[derive(Serialize, Deserialize, Clone)]
    #[repr(i32)]
    enum OldUserPreferenceUpdate {
        ConsentUpdate(StoredConsentRecord) = 1,
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_can_deserialize_between_versions() {
        let consent_record = StoredConsentRecord {
            entity: "hello there".to_string(),
            entity_type: ConsentType::Address,
            state: ConsentState::Allowed,
        };
        let update = UserPreferenceUpdate::ConsentUpdate(consent_record);

        let bytes = bincode::serialize(&update).unwrap();

        let old_update: OldUserPreferenceUpdate = bincode::deserialize(&bytes).unwrap();

        let OldUserPreferenceUpdate::ConsentUpdate(update) = old_update;
        assert_eq!(update.state, ConsentState::Allowed);
    }
}
