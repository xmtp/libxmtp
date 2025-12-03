use serde_json::{Map, Value};

/// Trait for error types that can expose structured, JSON-serializable details.
pub trait ErrorDetailsProvider {
    /// Returns optional structured details for the error.
    fn details(&self) -> Option<Map<String, Value>>;
}

impl ErrorDetailsProvider for xmtp_db::StorageError {
    fn details(&self) -> Option<Map<String, Value>> {
        use xmtp_db::StorageError;

        match self {
            StorageError::NotFound(nf) => {
                let mut map = Map::new();
                map.insert("entity".to_string(), Value::String(nf.to_string()));
                Some(map)
            }
            StorageError::Duplicate(dup) => {
                let mut map = Map::new();
                map.insert("entity".to_string(), Value::String(dup.to_string()));
                Some(map)
            }
            _ => None,
        }
    }
}

impl ErrorDetailsProvider for xmtp_api::ApiError {
    fn details(&self) -> Option<Map<String, Value>> {
        use xmtp_api::ApiError;

        match self {
            ApiError::MismatchedKeyPackages {
                key_packages,
                installation_keys,
            } => {
                let mut map = Map::new();
                map.insert("keyPackages".to_string(), Value::from(*key_packages));
                map.insert("installationKeys".to_string(), Value::from(*installation_keys));
                Some(map)
            }
            _ => None,
        }
    }
}

impl ErrorDetailsProvider for crate::identity::IdentityError {
    fn details(&self) -> Option<Map<String, Value>> {
        use crate::identity::IdentityError;

        match self {
            IdentityError::InstallationIdNotFound(id) => {
                let mut map = Map::new();
                map.insert("installationId".to_string(), Value::String(id.clone()));
                Some(map)
            }
            IdentityError::InboxIdMismatch { id, stored } => {
                let mut map = Map::new();
                map.insert("id".to_string(), Value::String(id.clone()));
                map.insert("stored".to_string(), Value::String(stored.clone()));
                Some(map)
            }
            IdentityError::NoAssociatedInboxId(addr) => {
                let mut map = Map::new();
                map.insert("address".to_string(), Value::String(addr.clone()));
                Some(map)
            }
            IdentityError::TooManyInstallations { inbox_id, count, max } => {
                let mut map = Map::new();
                map.insert("inboxId".to_string(), Value::String(inbox_id.clone()));
                map.insert("count".to_string(), Value::from(*count));
                map.insert("max".to_string(), Value::from(*max));
                Some(map)
            }
            _ => None,
        }
    }
}

impl ErrorDetailsProvider for crate::groups::GroupError {
    fn details(&self) -> Option<Map<String, Value>> {
        use crate::groups::GroupError;

        match self {
            GroupError::NotFound(nf) => {
                let mut map = Map::new();
                map.insert("entity".to_string(), Value::String(nf.to_string()));
                Some(map)
            }
            GroupError::AddressNotFound(addrs) => {
                let mut map = Map::new();
                map.insert("addresses".to_string(), Value::from(addrs.clone()));
                Some(map)
            }
            GroupError::LeaveCantProcessed(reason) => {
                let mut map = Map::new();
                let reason = match reason {
                    crate::groups::GroupLeaveValidationError::DmLeaveForbidden => {
                        "DmLeaveForbidden"
                    }
                    crate::groups::GroupLeaveValidationError::SingleMemberLeaveRejected => {
                        "SingleMemberLeaveRejected"
                    }
                    crate::groups::GroupLeaveValidationError::SuperAdminLeaveForbidden => {
                        "SuperAdminLeaveForbidden"
                    }
                    crate::groups::GroupLeaveValidationError::InboxAlreadyInPendingList => {
                        "InboxAlreadyInPendingList"
                    }
                    crate::groups::GroupLeaveValidationError::InboxNotInPendingList => {
                        "InboxNotInPendingList"
                    }
                    crate::groups::GroupLeaveValidationError::NotAGroupMember => {
                        "NotAGroupMember"
                    }
                };
                map.insert("reason".to_string(), Value::String(reason.to_string()));
                Some(map)
            }
            GroupError::InvalidPublicKeys(keys) => {
                let mut map = Map::new();
                map.insert("count".to_string(), Value::from(keys.len()));
                Some(map)
            }
            GroupError::TooManyCharacters { length } => {
                let mut map = Map::new();
                map.insert("maxLength".to_string(), Value::from(*length));
                Some(map)
            }
            GroupError::GroupPausedUntilUpdate(version) => {
                let mut map = Map::new();
                map.insert("requiredVersion".to_string(), Value::String(version.clone()));
                Some(map)
            }
            GroupError::WelcomeDataNotFound(topic) => {
                let mut map = Map::new();
                map.insert("topic".to_string(), Value::String(topic.clone()));
                Some(map)
            }
            _ => None,
        }
    }
}

impl ErrorDetailsProvider for crate::subscriptions::SubscribeError {
    fn details(&self) -> Option<Map<String, Value>> {
        use crate::subscriptions::SubscribeError;

        match self {
            SubscribeError::NotFound(nf) => {
                let mut map = Map::new();
                map.insert("entity".to_string(), Value::String(nf.to_string()));
                Some(map)
            }
            SubscribeError::MismatchedOriginators { expected, got } => {
                let mut map = Map::new();
                map.insert("expected".to_string(), Value::from(*expected));
                map.insert("got".to_string(), Value::from(*got));
                Some(map)
            }
            _ => None,
        }
    }
}

impl ErrorDetailsProvider for xmtp_content_types::CodecError {
    fn details(&self) -> Option<Map<String, Value>> {
        use xmtp_content_types::CodecError;

        match self {
            CodecError::Encode(msg) | CodecError::Decode(msg) => {
                let mut map = Map::new();
                map.insert("message".to_string(), Value::String(msg.clone()));
                Some(map)
            }
            CodecError::CodecNotFound(content_type_id) => {
                let mut map = Map::new();
                map.insert("contentType".to_string(), Value::String(format!("{:?}", content_type_id)));
                Some(map)
            }
            _ => None,
        }
    }
}
