use serde::{Deserialize, Serialize};
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto;

use crate::storage::consent_record::StoredConsentRecord;

#[derive(Serialize, Deserialize, Clone)]
pub enum UserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord),
    HmacKeyUpdate { key: Vec<u8> },
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
