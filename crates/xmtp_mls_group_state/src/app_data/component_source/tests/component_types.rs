//! Component source tests: inbox ids, component types and metadata field mapping.

use super::*;

// --- inbox-id helpers --------------------------------------------------

#[xmtp_common::test]
fn test_inbox_id_round_trip() {
    let original = fake_inbox_id(0xAB);
    let id = inbox_id_str_to_bytes(&original).unwrap();
    assert_eq!(id.as_bytes(), &[0xAB; 32]);
    assert_eq!(id.to_hex(), original);
}

#[xmtp_common::test]
fn test_inbox_id_invalid_hex() {
    let err = inbox_id_str_to_bytes("not_hex").unwrap_err();
    assert!(
        matches!(
            err,
            ComponentSourceError::InvalidInboxId(InboxIdError::InvalidHex(_))
        ),
        "got {err:?}"
    );
}

#[xmtp_common::test]
fn test_inbox_id_wrong_length() {
    // Valid hex but too short — only 16 bytes.
    let err = inbox_id_str_to_bytes(&"ab".repeat(16)).unwrap_err();
    assert!(
        matches!(
            err,
            ComponentSourceError::InvalidInboxId(InboxIdError::InvalidLength {
                expected: INBOX_ID_BYTE_LEN,
                actual: 16,
            })
        ),
        "got {err:?}"
    );
}

// --- component_type lookups --------------------------------------------

#[xmtp_common::test]
fn test_component_type_string_family() {
    // GMM-backed components whose wire format is UTF-8 text — names,
    // descriptions, URLs, and the app-data string blob.
    for id in [
        ComponentId::GROUP_NAME,
        ComponentId::GROUP_DESCRIPTION,
        ComponentId::GROUP_IMAGE_URL,
        ComponentId::APP_DATA,
        ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
    ] {
        assert_eq!(component_type(id), Some(ComponentType::String));
    }
}

#[xmtp_common::test]
fn test_component_type_bytes_family() {
    // Components whose payload is opaque bytes, not UTF-8 text:
    // timestamped disappearance windows and the commit-log signer key.
    for id in [
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
        ComponentId::MESSAGE_DISAPPEAR_IN_NS,
        ComponentId::COMMIT_LOG_SIGNER,
    ] {
        assert_eq!(component_type(id), Some(ComponentType::Bytes));
    }
}

#[xmtp_common::test]
fn test_component_type_set_inbox_id_family() {
    for id in [
        ComponentId::ADMIN_LIST,
        ComponentId::SUPER_ADMIN_LIST,
        ComponentId::DM_MEMBERS,
    ] {
        assert_eq!(component_type(id), Some(ComponentType::TlsSetInboxId));
    }
}

#[xmtp_common::test]
fn test_component_type_group_membership() {
    assert_eq!(
        component_type(ComponentId::GROUP_MEMBERSHIP),
        Some(ComponentType::TlsMapInboxIdBytes)
    );
}

/// The two newest well-known ids carry the fixed types every client
/// must agree on: a UTF-8 inbox map for display names, opaque bytes for
/// the group image.
// verifies: META-010
#[xmtp_common::test]
fn test_component_type_profile_and_image() {
    assert_eq!(
        component_type(ComponentId::USER_DISPLAY_NAME),
        Some(ComponentType::TlsMapInboxIdString)
    );
    assert_eq!(
        component_type(ComponentId::GROUP_IMAGE),
        Some(ComponentType::Bytes)
    );
}

#[xmtp_common::test]
fn test_component_type_app_range_is_none() {
    assert_eq!(component_type(ComponentId::new(0xC000)), None);
    assert_eq!(component_type(ComponentId::new(0xFDAB)), None);
}

// --- MetadataField <-> ComponentId mapping -----------------------------

#[xmtp_common::test]
fn test_metadata_field_round_trip() {
    for field in [
        MetadataField::GroupName,
        MetadataField::Description,
        MetadataField::GroupImageUrlSquare,
        MetadataField::MessageDisappearFromNS,
        MetadataField::MessageDisappearInNS,
        MetadataField::MinimumSupportedProtocolVersion,
        MetadataField::CommitLogSigner,
        MetadataField::AppData,
    ] {
        let id = metadata_field_to_component_id(field.as_str())
            .expect("every MetadataField has a ComponentId");
        let back = component_id_to_metadata_field(id)
            .expect("every mapped ComponentId has a MetadataField");
        assert_eq!(back, field, "round-trip mismatch for {field:?}");
    }
}

#[xmtp_common::test]
fn test_metadata_field_unknown_name_returns_none() {
    assert!(metadata_field_to_component_id("nonexistent_field").is_none());
}

#[xmtp_common::test]
fn test_component_id_to_metadata_field_non_gmm_returns_none() {
    assert!(component_id_to_metadata_field(ComponentId::ADMIN_LIST).is_none());
    assert!(component_id_to_metadata_field(ComponentId::GROUP_MEMBERSHIP).is_none());
    assert!(component_id_to_metadata_field(ComponentId::new(0xC000)).is_none());
}
