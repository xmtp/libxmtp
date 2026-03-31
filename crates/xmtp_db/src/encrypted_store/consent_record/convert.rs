use super::*;

impl TryFrom<ConsentSave> for StoredConsentRecord {
    type Error = ConversionError;
    fn try_from(value: ConsentSave) -> Result<Self, Self::Error> {
        let entity_type = value.entity_type().try_into()?;
        let state = value.state().try_into()?;

        Ok(Self {
            entity_type,
            state,
            entity: value.entity,
            consented_at_ns: value.consented_at_ns,
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
            consented_at_ns: value.consented_at_ns,
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

impl TryFrom<ConsentTypeSave> for ConsentType {
    type Error = ConversionError;
    #[allow(deprecated)]
    fn try_from(value: ConsentTypeSave) -> Result<Self, Self::Error> {
        Ok(match value {
            ConsentTypeSave::InboxId => Self::InboxId,
            ConsentTypeSave::ConversationId => Self::ConversationId,
            ConsentTypeSave::Address => return Err(ConversionError::Deprecated("address")),
            ConsentTypeSave::Unspecified => {
                return Err(ConversionError::Unspecified("consent_type"));
            }
        })
    }
}

impl TryFrom<ConsentStateSave> for ConsentState {
    type Error = ConversionError;
    fn try_from(value: ConsentStateSave) -> Result<Self, Self::Error> {
        Ok(match value {
            ConsentStateSave::Allowed => Self::Allowed,
            ConsentStateSave::Denied => Self::Denied,
            ConsentStateSave::Unknown => Self::Unknown,
            ConsentStateSave::Unspecified => {
                return Err(ConversionError::Unspecified("consent_state"));
            }
        })
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
