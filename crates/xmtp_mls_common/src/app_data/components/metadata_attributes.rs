//! [`Component`] impls for the eight `GroupMutableMetadata`-backed
//! attribute components.
//!
//! Three flavours, distinguished by the typed `Value` they expose:
//!
//! - **String** (`GROUP_NAME`, `GROUP_DESCRIPTION`, `GROUP_IMAGE_URL`,
//!   `APP_DATA`, `MIN_SUPPORTED_PROTOCOL_VERSION`): `Value = String`,
//!   wire bytes are UTF-8.
//! - **Big-endian `i64`** (`MESSAGE_DISAPPEAR_FROM_NS`,
//!   `MESSAGE_DISAPPEAR_IN_NS`): `Value = i64`, wire bytes are exactly
//!   8 bytes (`i64::to_be_bytes`). The legacy `GroupMutableMetadata`
//!   path stringifies these as decimal; the AppData path uses the
//!   binary representation directly so callers don't have to round-trip
//!   through ASCII.
//! - **Fixed-length signer key** (`COMMIT_LOG_SIGNER`):
//!   `Value = xmtp_cryptography::Secret` (zeroized on drop) holding
//!   exactly `ED25519_KEY_LENGTH` bytes — the raw Ed25519 private-key
//!   material that the legacy path hex-encoded into a string. The
//!   length is enforced at decode time; callers continue to handle a
//!   `Secret` end-to-end without a hex round-trip.
//!
//! `AppDataUpdate::Update` payloads pass through verbatim after the
//! component-specific length/UTF-8 validation, and there is no delta
//! encoding.

use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_cryptography::{Secret, configuration::ED25519_KEY_LENGTH};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::app_data::{
    component_id::ComponentId,
    component_registry::ComponentOp,
    typed::{Component, ComponentTypedError, ExpandedComponentChange},
};

/// Apply a passthrough Update payload — no delta math, the payload is
/// the new full value bytes.
fn apply_passthrough(payload: &[u8]) -> Result<Vec<u8>, ComponentTypedError> {
    Ok(payload.to_vec())
}

/// Expand an `AppDataUpdate` proposal for a passthrough component:
/// `Update` produces one `Update` change carrying the payload bytes;
/// `Remove` produces one `Delete` change with no value.
fn expand_passthrough(
    op: &AppDataUpdateOperation,
) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
    match op {
        AppDataUpdateOperation::Update(payload) => Ok(vec![ExpandedComponentChange {
            op: ComponentOp::Update,
            value: Some(payload.as_slice().to_vec()),
        }]),
        AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
            op: ComponentOp::Delete,
            value: None,
        }]),
    }
}

/// Decode UTF-8 bytes into a `String`, surfacing a structured
/// [`ComponentTypedError::MalformedValue`] on invalid input rather than
/// the bare `Utf8Error`.
fn decode_utf8(component_id: ComponentId, bytes: &[u8]) -> Result<String, ComponentTypedError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|err| ComponentTypedError::MalformedValue {
            component_id,
            reason: format!("not valid UTF-8: {err}"),
        })
}

/// Decode an 8-byte big-endian `i64`, surfacing
/// [`ComponentTypedError::MalformedValue`] on length mismatch.
fn decode_be_i64(component_id: ComponentId, bytes: &[u8]) -> Result<i64, ComponentTypedError> {
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| ComponentTypedError::MalformedValue {
            component_id,
            reason: format!("expected 8 bytes (BE i64), got {}", bytes.len()),
        })?;
    Ok(i64::from_be_bytes(arr))
}

/// Validate a byte slice has the expected fixed length, surfacing
/// [`ComponentTypedError::MalformedValue`] on mismatch.
fn require_exact_len(
    component_id: ComponentId,
    bytes: &[u8],
    expected: usize,
) -> Result<(), ComponentTypedError> {
    if bytes.len() != expected {
        return Err(ComponentTypedError::MalformedValue {
            component_id,
            reason: format!("expected {} bytes, got {}", expected, bytes.len()),
        });
    }
    Ok(())
}

/// Internal macro for declaring a passthrough metadata-attribute
/// component impl.
///
/// Each invocation produces a unit struct + `Component` impl. The
/// `decode_value` / `encode_value` / `encode_mutation` bodies vary
/// between the `String` and `Bytes` shapes, so the macro takes them as
/// arms.
///
/// The macro stays internal (`macro_rules!` with no `pub` attribute) —
/// it's a within-this-file convenience, not a public API.
macro_rules! passthrough_string_component {
    ($struct_name:ident, $id:expr) => {
        pub struct $struct_name;

        impl Component for $struct_name {
            const ID: ComponentId = $id;
            const COMPONENT_TYPE: ComponentType = ComponentType::String;
            type Value = String;
            type Mutation = String;

            fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
                decode_utf8(Self::ID, bytes)
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
                Ok(value.as_bytes().to_vec())
            }

            fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
                Ok(mutation.as_bytes().to_vec())
            }

            fn apply_update_payload(
                payload: &[u8],
                _prior: Option<&[u8]>,
            ) -> Result<Vec<u8>, ComponentTypedError> {
                apply_passthrough(payload)
            }

            fn expand_to_changes(
                op: &AppDataUpdateOperation,
                _prior: Option<&[u8]>,
            ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
                expand_passthrough(op)
            }
        }
    };
}

/// Internal macro for the two `i64` disappearance-window components.
/// Both share the wire shape (8-byte BE) and only differ in their ID.
macro_rules! be_i64_component {
    ($struct_name:ident, $id:expr) => {
        pub struct $struct_name;

        impl Component for $struct_name {
            const ID: ComponentId = $id;
            const COMPONENT_TYPE: ComponentType = ComponentType::Bytes;
            type Value = i64;
            type Mutation = i64;

            fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
                decode_be_i64(Self::ID, bytes)
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
                Ok(value.to_be_bytes().to_vec())
            }

            fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
                Ok(mutation.to_be_bytes().to_vec())
            }

            fn apply_update_payload(
                payload: &[u8],
                _prior: Option<&[u8]>,
            ) -> Result<Vec<u8>, ComponentTypedError> {
                // Validate length+shape and pass through.
                let _ = decode_be_i64(Self::ID, payload)?;
                Ok(payload.to_vec())
            }

            fn expand_to_changes(
                op: &AppDataUpdateOperation,
                _prior: Option<&[u8]>,
            ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
                match op {
                    AppDataUpdateOperation::Update(payload) => {
                        let _ = decode_be_i64(Self::ID, payload.as_slice())?;
                        Ok(vec![ExpandedComponentChange {
                            op: ComponentOp::Update,
                            value: Some(payload.as_slice().to_vec()),
                        }])
                    }
                    AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
                        op: ComponentOp::Delete,
                        value: None,
                    }]),
                }
            }
        }
    };
}

passthrough_string_component!(GroupNameComponent, ComponentId::GROUP_NAME);
passthrough_string_component!(GroupDescriptionComponent, ComponentId::GROUP_DESCRIPTION);
passthrough_string_component!(GroupImageUrlComponent, ComponentId::GROUP_IMAGE_URL);
passthrough_string_component!(AppDataComponent, ComponentId::APP_DATA);
passthrough_string_component!(
    MinSupportedProtocolVersionComponent,
    ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION
);

be_i64_component!(
    MessageDisappearFromNsComponent,
    ComponentId::MESSAGE_DISAPPEAR_FROM_NS
);
be_i64_component!(
    MessageDisappearInNsComponent,
    ComponentId::MESSAGE_DISAPPEAR_IN_NS
);

/// `Component` impl for the `COMMIT_LOG_SIGNER` component.
///
/// The decoded value is an [`xmtp_cryptography::Secret`] holding
/// exactly `ED25519_KEY_LENGTH` bytes — the raw Ed25519 private-key
/// material. The length is enforced at decode time so callers can
/// always treat the resulting `Secret` as a 32-byte key.
pub struct CommitLogSignerComponent;

impl Component for CommitLogSignerComponent {
    const ID: ComponentId = ComponentId::COMMIT_LOG_SIGNER;
    const COMPONENT_TYPE: ComponentType = ComponentType::Bytes;
    type Value = Secret;
    type Mutation = Secret;

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
        require_exact_len(Self::ID, bytes, ED25519_KEY_LENGTH)?;
        Ok(Secret::new(bytes.to_vec()))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
        require_exact_len(Self::ID, value.as_slice(), ED25519_KEY_LENGTH)?;
        Ok(value.as_slice().to_vec())
    }

    fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
        require_exact_len(Self::ID, mutation.as_slice(), ED25519_KEY_LENGTH)?;
        Ok(mutation.as_slice().to_vec())
    }

    fn apply_update_payload(
        payload: &[u8],
        _prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        require_exact_len(Self::ID, payload, ED25519_KEY_LENGTH)?;
        Ok(payload.to_vec())
    }

    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        _prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        match op {
            AppDataUpdateOperation::Update(payload) => {
                require_exact_len(Self::ID, payload.as_slice(), ED25519_KEY_LENGTH)?;
                Ok(vec![ExpandedComponentChange {
                    op: ComponentOp::Update,
                    value: Some(payload.as_slice().to_vec()),
                }])
            }
            AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
                op: ComponentOp::Delete,
                value: None,
            }]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls::messages::proposals::AppDataUpdateOperation;

    #[xmtp_common::test(unwrap_try = true)]
    fn string_component_round_trip() {
        let name = String::from("Test Group 🚀");
        let bytes = GroupNameComponent::encode_value(&name).unwrap();
        let decoded = GroupNameComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded, name);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn string_component_decode_rejects_non_utf8() {
        let bytes = vec![0xff, 0xfe, 0xfd];
        let err = GroupDescriptionComponent::decode_value(&bytes).unwrap_err();
        match err {
            ComponentTypedError::MalformedValue { component_id, .. } => {
                assert_eq!(component_id, ComponentId::GROUP_DESCRIPTION);
            }
            other => panic!("expected MalformedValue, got {other:?}"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn commit_log_signer_round_trip() {
        let key = Secret::new(vec![0xAB; ED25519_KEY_LENGTH]);
        let bytes = CommitLogSignerComponent::encode_value(&key).unwrap();
        assert_eq!(bytes.len(), ED25519_KEY_LENGTH);
        let decoded = CommitLogSignerComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded.as_slice(), key.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn commit_log_signer_rejects_wrong_length() {
        let too_short = vec![0u8; ED25519_KEY_LENGTH - 1];
        let err = CommitLogSignerComponent::decode_value(&too_short).unwrap_err();
        match err {
            ComponentTypedError::MalformedValue { component_id, .. } => {
                assert_eq!(component_id, ComponentId::COMMIT_LOG_SIGNER);
            }
            other => panic!("expected MalformedValue, got {other:?}"),
        }
        // Encoding a Secret that doesn't hold exactly 32 bytes also rejects.
        let wrong = Secret::new(vec![0u8; ED25519_KEY_LENGTH + 1]);
        let err = CommitLogSignerComponent::encode_value(&wrong).unwrap_err();
        assert!(matches!(err, ComponentTypedError::MalformedValue { .. }));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn message_disappear_round_trip() {
        // Use a value that exercises sign-extension: large negative i64.
        let v: i64 = -1_234_567_890_123;
        let bytes = MessageDisappearFromNsComponent::encode_value(&v).unwrap();
        assert_eq!(bytes.len(), 8);
        let decoded = MessageDisappearFromNsComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded, v);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn message_disappear_rejects_wrong_length() {
        // Catches a sender that emitted decimal-stringified bytes
        // (e.g. b"3600000000000") under the legacy convention.
        let ascii_decimal = b"3600000000000";
        let err = MessageDisappearInNsComponent::decode_value(ascii_decimal).unwrap_err();
        match err {
            ComponentTypedError::MalformedValue { component_id, .. } => {
                assert_eq!(component_id, ComponentId::MESSAGE_DISAPPEAR_IN_NS);
            }
            other => panic!("expected MalformedValue, got {other:?}"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn apply_update_passthrough() {
        // String component still passes payloads through verbatim.
        let payload = b"new value";
        let new = GroupNameComponent::apply_update_payload(payload, Some(b"old")).unwrap();
        assert_eq!(new, payload);

        // i64 components validate the length-and-shape but pass the
        // bytes through unchanged so receivers don't re-encode.
        let valid = 42_i64.to_be_bytes();
        let new = MessageDisappearInNsComponent::apply_update_payload(&valid, None).unwrap();
        assert_eq!(new.as_slice(), valid.as_slice());

        let err =
            MessageDisappearInNsComponent::apply_update_payload(b"not 8 bytes", None).unwrap_err();
        assert!(matches!(err, ComponentTypedError::MalformedValue { .. }));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_update_yields_one_change() {
        let payload = b"hello".to_vec();
        let op = AppDataUpdateOperation::Update(payload.clone().into());
        let changes = GroupNameComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Update);
        assert_eq!(changes[0].value.as_deref(), Some(payload.as_slice()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expand_remove_yields_delete_with_no_value() {
        let op = AppDataUpdateOperation::Remove;
        let changes = AppDataComponent::expand_to_changes(&op, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op, ComponentOp::Delete);
        assert!(changes[0].value.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_id_constants_match_static_id() {
        // Sanity check that each component reports its own ID via the
        // trait — a copy-paste error in the macro invocations would
        // surface here.
        assert_eq!(GroupNameComponent::ID, ComponentId::GROUP_NAME);
        assert_eq!(
            GroupDescriptionComponent::ID,
            ComponentId::GROUP_DESCRIPTION
        );
        assert_eq!(GroupImageUrlComponent::ID, ComponentId::GROUP_IMAGE_URL);
        assert_eq!(AppDataComponent::ID, ComponentId::APP_DATA);
        assert_eq!(
            MinSupportedProtocolVersionComponent::ID,
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION
        );
        assert_eq!(
            MessageDisappearFromNsComponent::ID,
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS
        );
        assert_eq!(
            MessageDisappearInNsComponent::ID,
            ComponentId::MESSAGE_DISAPPEAR_IN_NS
        );
        assert_eq!(
            CommitLogSignerComponent::ID,
            ComponentId::COMMIT_LOG_SIGNER
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_types_are_correct() {
        assert_eq!(GroupNameComponent::COMPONENT_TYPE, ComponentType::String);
        assert_eq!(
            CommitLogSignerComponent::COMPONENT_TYPE,
            ComponentType::Bytes
        );
        assert_eq!(
            MessageDisappearFromNsComponent::COMPONENT_TYPE,
            ComponentType::Bytes
        );
        assert_eq!(
            MinSupportedProtocolVersionComponent::COMPONENT_TYPE,
            ComponentType::String
        );
    }
}
