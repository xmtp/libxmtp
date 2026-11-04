//! Component source tests: dictionary and extension reads, and the metadata merge.

use super::*;

// ========================================================================
// Dict-reader helpers — happy path and malformed coverage
// ========================================================================
//
// The three `read_*_from_dict` helpers execute before bootstrap is
// wired end-to-end; the unit tests below pin the wire-format
// contract so a decoder drift can't silently ship a broken read path.

use openmls::extensions::{
    AppDataDictionary, AppDataDictionaryExtension, Extension as OpenMlsExtension, Extensions,
};

/// Build a synthetic `Extensions<GroupContext>` with an AppData dictionary.
fn extensions_with_entries(entries: &[(u16, Vec<u8>)]) -> Extensions<openmls::group::GroupContext> {
    let mut dict = AppDataDictionary::new();
    for (id, bytes) in entries {
        let _ = dict.insert(*id, bytes.clone());
    }
    Extensions::from_vec(vec![OpenMlsExtension::AppDataDictionary(
        AppDataDictionaryExtension::new(dict),
    )])
    .expect("valid group-context extension set")
}

// --- read_super_admin_list_from_extensions ------------------------------

#[xmtp_common::test]
fn read_super_admin_list_absent_returns_none() {
    // The dict has no SUPER_ADMIN_LIST entry —
    // `Ok(None)` rather than surfacing a malformed-value error.
    let exts = extensions_with_entries(&[]);
    assert!(
        read_super_admin_list_from_extensions(&exts)
            .unwrap()
            .is_none()
    );
}

#[xmtp_common::test]
fn read_super_admin_list_happy_path() {
    let ids = vec![fake_inbox_id(0xAA), fake_inbox_id(0xBB)];
    let bytes = encode_inbox_id_set(&ids).unwrap();
    let exts = extensions_with_entries(&[(ComponentId::SUPER_ADMIN_LIST.as_u16(), bytes)]);
    let got = read_super_admin_list_from_extensions(&exts)
        .unwrap()
        .unwrap();
    assert_eq!(got.len(), 2);
    // TlsSet sorts by value so 0xAA sorts before 0xBB.
    assert_eq!(got[0], fake_inbox_id(0xAA));
    assert_eq!(got[1], fake_inbox_id(0xBB));
}

#[xmtp_common::test]
fn read_super_admin_list_malformed_bytes_surface_error() {
    let exts = extensions_with_entries(&[(
        ComponentId::SUPER_ADMIN_LIST.as_u16(),
        vec![0x00, 0xDE, 0xAD],
    )]);
    let err = read_super_admin_list_from_extensions(&exts).unwrap_err();
    assert!(matches!(
        err,
        ComponentSourceError::MalformedComponentValue {
            component_id,
            ..
        } if component_id == ComponentId::SUPER_ADMIN_LIST
    ));
}

// --- read_group_metadata_from_extensions --------------------------------

fn encode_conv_type_bytes(value: i32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

fn encode_dm_pair(tag_a: u8, tag_b: u8) -> Vec<u8> {
    encode_inbox_id_set(&[fake_inbox_id(tag_a), fake_inbox_id(tag_b)]).unwrap()
}

/// Encode a single tagged inbox id in the `CREATOR_INBOX_ID` wire
/// form (versioned `InboxId` TLS encoding).
fn encode_creator_bytes(tag: u8) -> Vec<u8> {
    fake_inbox(tag).tls_serialize_detached().unwrap()
}

#[xmtp_common::test]
fn read_group_metadata_missing_required_seeds_returns_none() {
    // A missing CONVERSATION_TYPE seed is not malformed.
    let exts = extensions_with_entries(&[]);
    assert!(
        read_group_metadata_from_extensions(&exts)
            .unwrap()
            .is_none()
    );
}

#[xmtp_common::test]
fn read_group_metadata_happy_path_non_dm() {
    let exts = extensions_with_entries(&[
        (
            ComponentId::CONVERSATION_TYPE.as_u16(),
            encode_conv_type_bytes(1),
        ),
        (
            ComponentId::CREATOR_INBOX_ID.as_u16(),
            encode_creator_bytes(0x11),
        ),
    ]);
    let got = read_group_metadata_from_extensions(&exts).unwrap().unwrap();
    assert_eq!(got.conversation_type, 1);
    assert_eq!(got.creator_inbox_id, fake_inbox_id(0x11));
    assert!(got.dm_members.is_none());
    assert!(got.oneshot.is_none());
}

#[xmtp_common::test]
fn read_group_metadata_dm_happy_path() {
    // DM group — DM_MEMBERS decodes as TlsSet<InboxId>, re-shaped
    // to the proto's two-slot form.
    let exts = extensions_with_entries(&[
        (
            ComponentId::CONVERSATION_TYPE.as_u16(),
            encode_conv_type_bytes(2),
        ),
        (
            ComponentId::CREATOR_INBOX_ID.as_u16(),
            encode_creator_bytes(0x22),
        ),
        (ComponentId::DM_MEMBERS.as_u16(), encode_dm_pair(0x22, 0x33)),
    ]);
    let got = read_group_metadata_from_extensions(&exts).unwrap().unwrap();
    let dm = got.dm_members.unwrap();
    assert_eq!(dm.dm_member_one.unwrap().inbox_id, fake_inbox_id(0x22));
    assert_eq!(dm.dm_member_two.unwrap().inbox_id, fake_inbox_id(0x33));
}

#[xmtp_common::test]
fn read_group_metadata_dm_wrong_cardinality_errors() {
    // A 1-element TlsSet<InboxId> is invalid for DM_MEMBERS —
    // surfaces `MalformedComponentValue`.
    let one_element = encode_inbox_id_set(&[fake_inbox_id(0x44)]).unwrap();
    let exts = extensions_with_entries(&[
        (
            ComponentId::CONVERSATION_TYPE.as_u16(),
            encode_conv_type_bytes(2),
        ),
        (
            ComponentId::CREATOR_INBOX_ID.as_u16(),
            encode_creator_bytes(0x44),
        ),
        (ComponentId::DM_MEMBERS.as_u16(), one_element),
    ]);
    let err = read_group_metadata_from_extensions(&exts).unwrap_err();
    assert!(matches!(
        err,
        ComponentSourceError::MalformedComponentValue {
            component_id,
            ..
        } if component_id == ComponentId::DM_MEMBERS
    ));
}

/// A `DM_MEMBERS` pair outside a DM is malformed. Commit authorization
/// reads metadata through this parser, and the pair grants DM-only
/// authority, so a group persisted by an older client with the pair
/// must not reach the policy checks.
// verifies: META-030
#[xmtp_common::test(unwrap_try = true)]
fn read_group_metadata_rejects_dm_members_outside_a_dm() {
    let read = |conversation_type: i32| {
        read_group_metadata_from_extensions(&extensions_with_entries(&[
            (
                ComponentId::CONVERSATION_TYPE.as_u16(),
                encode_conv_type_bytes(conversation_type),
            ),
            (
                ComponentId::CREATOR_INBOX_ID.as_u16(),
                encode_creator_bytes(0x22),
            ),
            (ComponentId::DM_MEMBERS.as_u16(), encode_dm_pair(0x22, 0x33)),
        ]))
    };
    read(2)?;
    for non_dm in [0, 1, 3, 4] {
        assert!(matches!(
            read(non_dm),
            Err(ComponentSourceError::MalformedComponentValue { component_id, .. })
                if component_id == ComponentId::DM_MEMBERS
        ));
    }
}

#[xmtp_common::test]
fn read_group_metadata_malformed_creator_errors() {
    // CREATOR_INBOX_ID is the versioned `InboxId` TLS encoding —
    // a few stray bytes won't satisfy the varint length prefix
    // plus 32-byte payload, so deserialization must fail loud as
    // `MalformedComponentValue` rather than silently producing
    // a phantom inbox id.
    let exts = extensions_with_entries(&[
        (
            ComponentId::CONVERSATION_TYPE.as_u16(),
            encode_conv_type_bytes(1),
        ),
        (
            ComponentId::CREATOR_INBOX_ID.as_u16(),
            vec![0xFF, 0xFE, 0xFD],
        ),
    ]);
    let err = read_group_metadata_from_extensions(&exts).unwrap_err();
    assert!(matches!(
        err,
        ComponentSourceError::MalformedComponentValue {
            component_id,
            ..
        } if component_id == ComponentId::CREATOR_INBOX_ID
    ));
}

// --- read_group_membership_from_dict ------------------------------------

#[xmtp_common::test]
fn read_group_membership_happy_path_flattens_per_inbox() {
    use crate::app_data::migration::encode_group_membership_dict;
    use std::collections::BTreeMap;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1 as GroupMembershipEntryV1, Version},
    };
    let mut entries: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    entries.insert(
        InboxId::from_bytes([0x11; INBOX_ID_BYTE_LEN]),
        GroupMembershipEntry {
            version: Some(Version::V1(GroupMembershipEntryV1 {
                sequence_id: 7,
                failed_installations: vec![vec![0xA1; 16]],
            })),
        },
    );
    entries.insert(
        InboxId::from_bytes([0x22; INBOX_ID_BYTE_LEN]),
        GroupMembershipEntry {
            version: Some(Version::V1(GroupMembershipEntryV1 {
                sequence_id: 42,
                failed_installations: vec![vec![0xB1; 16]],
            })),
        },
    );
    let bytes = encode_group_membership_dict(&entries).unwrap();
    let exts = extensions_with_entries(&[(ComponentId::GROUP_MEMBERSHIP.as_u16(), bytes)]);

    let proto = read_group_membership_from_dict(&exts).unwrap().unwrap();
    // `members` is flat <hex_inbox_id, seq>
    assert_eq!(proto.members.len(), 2);
    assert_eq!(proto.members.get(&fake_inbox_id(0x11)), Some(&7));
    assert_eq!(proto.members.get(&fake_inbox_id(0x22)), Some(&42));
    // `failed_installations` concatenates the per-inbox lists.
    assert_eq!(proto.failed_installations.len(), 2);
}

#[xmtp_common::test]
fn read_group_membership_malformed_bytes_surface_error() {
    let exts = extensions_with_entries(&[(
        ComponentId::GROUP_MEMBERSHIP.as_u16(),
        vec![0xDE, 0xAD, 0xBE, 0xEF],
    )]);
    let err = read_group_membership_from_dict(&exts).unwrap_err();
    assert!(matches!(
        err,
        ComponentSourceError::MalformedComponentValue {
            component_id,
            ..
        } if component_id == ComponentId::GROUP_MEMBERSHIP
    ));
}

// ========================================================================
// merge_app_data_into_mutable_metadata_from_extensions —
//   independence from COMPONENT_REGISTRY parseability
// ========================================================================
//
// These pin the call-graph invariant that a malformed
// `COMPONENT_REGISTRY` does **not** cause `mutable_metadata()` to
// drop authoritative dict-backed fields. The merge function reads
// each metadata field directly from the dict; nothing on this
// read path calls `load_component_registry`. Registry corruption is
// surfaced loudly on the *write* paths (sender gate in `mls_sync.rs`
// and the commit validator in `validated_commit.rs`), where it
// belongs.
//
use crate::group_mutable_metadata::{GroupMutableMetadata, MetadataField};

fn empty_base_gmm() -> GroupMutableMetadata {
    GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new())
}

#[xmtp_common::test]
fn merge_with_malformed_registry_returns_valid_field() {
    // Reviewer's alleged "data loss" scenario: COMPONENT_REGISTRY
    // contains malformed bytes, but a metadata field (GROUP_NAME)
    // is present and valid. The merge function does NOT validate
    // the registry — it reads GROUP_NAME directly — so the result
    // must contain "My Group" with no error.
    let exts = extensions_with_entries(&[
        (ComponentId::COMPONENT_REGISTRY.as_u16(), vec![0xCA; 4]),
        (ComponentId::GROUP_NAME.as_u16(), b"My Group".to_vec()),
    ]);
    let mut base = empty_base_gmm();
    merge_app_data_into_mutable_metadata_from_extensions(&mut base, &exts)
        .expect("merge ignores registry parseability and reads field directly");
    assert_eq!(
        base.attributes()
            .get(MetadataField::GroupName.as_str())
            .map(String::as_str),
        Some("My Group"),
    );
}

#[xmtp_common::test]
fn merge_with_malformed_field_surfaces_error_not_silent_loss() {
    // The other half of the invariant: when a metadata field's bytes
    // ARE malformed, the merge fails loudly with
    // `MalformedComponentValue` carrying the offending component id —
    // it never silently swallows the value. Pairs with the test
    // above to disprove "all metadata may be lost" framings.
    let exts =
        extensions_with_entries(&[(ComponentId::ADMIN_LIST.as_u16(), vec![0xff, 0xff, 0xff])]);
    let mut base = empty_base_gmm();
    let err = merge_app_data_into_mutable_metadata_from_extensions(&mut base, &exts).unwrap_err();
    assert!(
        matches!(
            err,
            ComponentSourceError::MalformedComponentValue { component_id, .. }
                if component_id == ComponentId::ADMIN_LIST
        ),
        "expected MalformedComponentValue for ADMIN_LIST, got: {err:?}"
    );
}
