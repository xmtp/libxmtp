use super::*;
use prost::Message as _;
use tls_codec::Serialize;
use xmtp_proto::{
    types::ConversationType,
    xmtp::mls::message_contents::{
        MembershipPolicy, PermissionsUpdatePolicy,
        membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
        permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
    },
};

fn policy_set() -> PolicySetProto {
    let membership = MembershipPolicy {
        kind: Some(MembershipPolicyKind::Base(
            MembershipBasePolicy::Allow as i32,
        )),
    };
    let admin = PermissionsUpdatePolicy {
        kind: Some(PermissionsPolicyKind::Base(
            PermissionsBasePolicy::AllowIfAdmin as i32,
        )),
    };
    let super_admin = PermissionsUpdatePolicy {
        kind: Some(PermissionsPolicyKind::Base(
            PermissionsBasePolicy::AllowIfSuperAdmin as i32,
        )),
    };
    PolicySetProto {
        add_member_policy: Some(membership.clone()),
        remove_member_policy: Some(membership),
        update_metadata_policy: Default::default(),
        add_admin_policy: Some(admin.clone()),
        remove_admin_policy: Some(admin),
        update_permissions_policy: Some(super_admin),
    }
}

fn inbox(byte: u8) -> String {
    hex::encode([byte; 32])
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: META-075, PERM-002
fn initial_group_dictionary_seeds_required_and_optional_values() {
    let creator = inbox(1);
    let options = crate::group::GroupMetadataOptions {
        name: Some("name".into()),
        description: Some("description".into()),
        image_url_square: Some("image".into()),
        app_data: Some("app data".into()),
        message_disappearing_settings: Some(
            crate::group_mutable_metadata::MessageDisappearingSettings::new(7, 9),
        ),
    };
    let oneshot = xmtp_proto::xmtp::mls::message_contents::OneshotMessage::default();
    let root = GroupRoot::from_bytes(&[3; 32])?;
    let dictionary = initial_dictionary(
        InitialGroupKind::Group {
            conversation_type: ConversationType::Group,
            oneshot_message: Some(&oneshot),
        },
        &policy_set(),
        &options,
        &creator,
        Some(&root),
        &[],
    )?;

    let creator_id = InboxId::from_hex(&creator)?;
    let empty_admins =
        xmtp_mls_tls_collections::tls_set::TlsSet::<InboxId>::new().tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::ADMIN_LIST.as_u16()),
        Some(empty_admins.as_slice())
    );
    let super_admins = [creator_id]
        .into_iter()
        .collect::<xmtp_mls_tls_collections::tls_set::TlsSet<_>>()
        .tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::SUPER_ADMIN_LIST.as_u16()),
        Some(super_admins.as_slice())
    );
    let membership = super::super::migration::decode_group_membership_dict(
        dictionary
            .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
            .unwrap(),
    )?;
    assert!(matches!(
        membership.get(&creator_id).unwrap().version,
        Some(xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version::V1(ref entry))
            if entry.sequence_id == 0 && entry.failed_installations.is_empty()
    ));
    for (id, value) in [
        (ComponentId::GROUP_NAME, b"name".as_slice()),
        (ComponentId::GROUP_DESCRIPTION, b"description".as_slice()),
        (ComponentId::GROUP_IMAGE_URL, b"image".as_slice()),
        (ComponentId::APP_DATA, b"app data".as_slice()),
        (ComponentId::MESSAGE_DISAPPEAR_FROM_NS, &7_i64.to_be_bytes()),
        (ComponentId::MESSAGE_DISAPPEAR_IN_NS, &9_i64.to_be_bytes()),
    ] {
        assert_eq!(dictionary.get(&id.as_u16()), Some(value));
    }
    assert_eq!(
        dictionary.get(&ComponentId::GROUP_ROOT_KEY.as_u16()),
        Some(root.as_bytes().as_slice())
    );
    assert!(!dictionary.contains(&ComponentId::COMMIT_LOG_SIGNER.as_u16()));
    // The immutable registry row: super-admin insert, no update or delete.
    assert_eq!(
        registry_of(&dictionary).get(&ComponentId::GROUP_ROOT_KEY)?,
        Some(new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
                update_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
                delete_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
            },
            ComponentType::Bytes,
        ))
    );
    assert_eq!(
        dictionary.get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16()),
        Some(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes())
    );
    let conversation_type = encode_conversation_type(
        xmtp_proto::xmtp::mls::message_contents::ConversationType::Group as i32,
    );
    assert_eq!(
        dictionary.get(&ComponentId::CONVERSATION_TYPE.as_u16()),
        Some(conversation_type.as_slice())
    );
    let serialized_creator_id = creator_id.tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::CREATOR_INBOX_ID.as_u16()),
        Some(serialized_creator_id.as_slice())
    );
    let encoded_oneshot = oneshot.encode_to_vec();
    assert_eq!(
        dictionary.get(&ComponentId::ONESHOT_MESSAGE.as_u16()),
        Some(encoded_oneshot.as_slice())
    );
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: META-075, DMS-002
fn initial_dm_dictionary_has_empty_admin_sets_and_dm_members() {
    let creator = inbox(1);
    let target = inbox(2);
    let root = GroupRoot::from_bytes(&[4; 32])?;
    let dictionary = initial_dictionary(
        InitialGroupKind::Dm {
            target_inbox_id: &target,
        },
        &policy_set(),
        &crate::group::GroupMetadataOptions::default(),
        &creator,
        Some(&root),
        &[],
    )?;

    let creator_id = InboxId::from_hex(&creator)?;
    let target_id = InboxId::from_hex(&target)?;
    let empty =
        xmtp_mls_tls_collections::tls_set::TlsSet::<InboxId>::new().tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::ADMIN_LIST.as_u16()),
        Some(empty.as_slice())
    );
    assert_eq!(
        dictionary.get(&ComponentId::SUPER_ADMIN_LIST.as_u16()),
        Some(empty.as_slice())
    );
    let dm_members = [creator_id, target_id]
        .into_iter()
        .collect::<xmtp_mls_tls_collections::tls_set::TlsSet<_>>()
        .tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::DM_MEMBERS.as_u16()),
        Some(dm_members.as_slice())
    );
    let conversation_type = encode_conversation_type(
        xmtp_proto::xmtp::mls::message_contents::ConversationType::Dm as i32,
    );
    assert_eq!(
        dictionary.get(&ComponentId::CONVERSATION_TYPE.as_u16()),
        Some(conversation_type.as_slice())
    );
    let serialized_creator_id = creator_id.tls_serialize_detached()?;
    assert_eq!(
        dictionary.get(&ComponentId::CREATOR_INBOX_ID.as_u16()),
        Some(serialized_creator_id.as_slice())
    );
    for id in [
        ComponentId::GROUP_NAME,
        ComponentId::GROUP_DESCRIPTION,
        ComponentId::GROUP_IMAGE_URL,
        ComponentId::APP_DATA,
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
        ComponentId::MESSAGE_DISAPPEAR_IN_NS,
        ComponentId::COMMIT_LOG_SIGNER,
        ComponentId::ONESHOT_MESSAGE,
    ] {
        assert!(!dictionary.contains(&id.as_u16()));
    }
    assert_eq!(
        dictionary.get(&ComponentId::GROUP_ROOT_KEY.as_u16()),
        Some(root.as_bytes().as_slice())
    );
    // DMS section 2's immutable row: super-admin insert, no update or delete.
    assert_eq!(
        registry_of(&dictionary).get(&ComponentId::GROUP_ROOT_KEY)?,
        Some(new_component_metadata(
            ComponentPermissions {
                insert_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
                update_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
                delete_policy: Some(metadata_policy(MetadataBasePolicy::Deny)),
            },
            ComponentType::Bytes,
        ))
    );
}

/// A restored placeholder takes no root: neither the value nor its
/// registry entry, so no later commit can insert one.
#[xmtp_common::test(unwrap_try = true)]
fn a_dictionary_without_a_root_registers_no_root() {
    for kind in [
        InitialGroupKind::Group {
            conversation_type: ConversationType::Group,
            oneshot_message: None,
        },
        InitialGroupKind::RestoredDm {
            member_one_inbox_id: &inbox(1),
            member_two_inbox_id: &inbox(2),
        },
    ] {
        let dictionary = initial_dictionary(
            kind,
            &policy_set(),
            &crate::group::GroupMetadataOptions::default(),
            &inbox(1),
            None,
            &[],
        )?;
        for id in [ComponentId::GROUP_ROOT_KEY, ComponentId::COMMIT_LOG_SIGNER] {
            assert!(!dictionary.contains(&id.as_u16()), "{id:?}");
        }
        assert!(!registry_of(&dictionary).contains_raw(&ComponentId::GROUP_ROOT_KEY));
    }
}

fn definition(component_id: u16, in_groups: bool, in_dms: bool) -> ApplicationComponentDefinition {
    let policy = xmtp_configuration::MetadataPolicy::Base(MetadataBasePolicy::AllowIfAdmin as i32);
    ApplicationComponentDefinition {
        component_id,
        name: format!("field_{component_id:x}"),
        component_type: ComponentType::String as i32,
        permissions: xmtp_configuration::ComponentPermissions {
            insert: Some(policy.clone()),
            update: Some(policy.clone()),
            delete: Some(policy),
        },
        in_groups,
        in_dms,
    }
}

fn registry_of(dictionary: &openmls::extensions::AppDataDictionary) -> ComponentRegistry {
    ComponentRegistry::from_bytes(
        dictionary
            .get(&ComponentId::COMPONENT_REGISTRY.as_u16())
            .unwrap(),
    )
    .unwrap()
}

fn group_dictionary(
    conversation_type: ConversationType,
    policy_set: &PolicySetProto,
    catalogue: &[ApplicationComponentDefinition],
) -> openmls::extensions::AppDataDictionary {
    initial_dictionary(
        InitialGroupKind::Group {
            conversation_type,
            oneshot_message: None,
        },
        policy_set,
        &crate::group::GroupMetadataOptions::default(),
        &inbox(1),
        None,
        catalogue,
    )
    .unwrap()
}

fn self_owned() -> ComponentPermissions {
    let policy = metadata_policy(MetadataBasePolicy::AllowIfSelfOrNonMember);
    ComponentPermissions {
        insert_policy: Some(policy.clone()),
        update_policy: Some(policy.clone()),
        delete_policy: Some(policy),
    }
}

/// A new group registers the catalogue's group definitions with their
/// own type and policies, `USER_DISPLAY_NAME` as a self-owned string
/// map, and `GROUP_IMAGE` as bytes, and writes no value for any of them.
/// A DM-only definition stays out of a group.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066, PERM-029
fn new_group_registers_configured_fields_without_values() {
    let catalogue = [
        definition(0xC000, true, false),
        definition(0xC001, false, true),
        definition(0xFD00, true, true),
    ];
    let dictionary = group_dictionary(ConversationType::Group, &policy_set(), &catalogue);
    let registry = registry_of(&dictionary);

    for id in [0xC000, 0xFD00] {
        let expected = &catalogue.iter().find(|d| d.component_id == id).unwrap();
        let entry = registry.get(&ComponentId::new(id))?.unwrap();
        assert_eq!(entry.component_type, ComponentType::String as i32);
        assert_eq!(entry.permissions, Some(expected.permissions.clone().into()));
    }
    assert!(!registry.contains_raw(&ComponentId::new(0xC001)));
    assert_eq!(
        registry.get(&ComponentId::USER_DISPLAY_NAME)?,
        Some(new_component_metadata(
            self_owned(),
            ComponentType::TlsMapInboxIdString
        ))
    );
    let image = registry.get(&ComponentId::GROUP_IMAGE)?.unwrap();
    assert_eq!(image.component_type, ComponentType::Bytes as i32);
    for id in [
        ComponentId::new(0xC000),
        ComponentId::new(0xFD00),
        ComponentId::USER_DISPLAY_NAME,
        ComponentId::GROUP_IMAGE,
    ] {
        assert!(!dictionary.contains(&id.as_u16()));
    }
}

/// `GROUP_IMAGE` takes the image URL policies chosen at creation, so a
/// group that lets members change its image URL lets them change its
/// image, and the delete policy stays super admin.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-029
fn group_image_copies_the_image_url_policies() {
    let mut policies = policy_set();
    policies.update_metadata_policy.insert(
        MetadataField::GroupImageUrlSquare.as_str().to_string(),
        metadata_policy(MetadataBasePolicy::Allow),
    );
    let registry = registry_of(&group_dictionary(ConversationType::Group, &policies, &[]));
    let image_url = registry.get(&ComponentId::GROUP_IMAGE_URL)?.unwrap();
    let image = registry.get(&ComponentId::GROUP_IMAGE)?.unwrap();
    assert_eq!(image.permissions, image_url.permissions);
    let permissions = image.permissions.unwrap();
    assert_eq!(
        permissions.update_policy,
        Some(metadata_policy(MetadataBasePolicy::Allow))
    );
    assert_eq!(
        permissions.delete_policy,
        Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin))
    );
}

/// A DM registers its DM definitions and `USER_DISPLAY_NAME`, but not
/// `GROUP_IMAGE` or a group-only definition.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066, PERM-029
fn new_dm_registers_dm_fields_only() {
    let catalogue = [
        definition(0xC000, true, false),
        definition(0xC001, false, true),
    ];
    let dictionary = initial_dictionary(
        InitialGroupKind::Dm {
            target_inbox_id: &inbox(2),
        },
        &policy_set(),
        &crate::group::GroupMetadataOptions::default(),
        &inbox(1),
        None,
        &catalogue,
    )?;
    let registry = registry_of(&dictionary);
    assert!(registry.contains(&ComponentId::new(0xC001)));
    assert!(!registry.contains_raw(&ComponentId::new(0xC000)));
    assert!(!registry.contains_raw(&ComponentId::GROUP_IMAGE));
    assert_eq!(
        registry.get(&ComponentId::USER_DISPLAY_NAME)?,
        Some(new_component_metadata(
            self_owned(),
            ComponentType::TlsMapInboxIdString
        ))
    );
    assert!(!dictionary.contains(&ComponentId::USER_DISPLAY_NAME.as_u16()));
}

/// A sync group carries no user or application fields, whatever the
/// catalogue says.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066
fn sync_group_registers_no_configured_fields() {
    let catalogue = [definition(0xC000, true, true)];
    let registry = registry_of(&group_dictionary(
        ConversationType::Sync,
        &policy_set(),
        &catalogue,
    ));
    for id in [
        ComponentId::new(0xC000),
        ComponentId::USER_DISPLAY_NAME,
        ComponentId::GROUP_IMAGE,
    ] {
        assert!(!registry.contains_raw(&id));
    }
}

/// A catalogue definition outside the application range never reaches
/// the registry, even from a snapshot no one validated: one that named
/// `GROUP_MEMBERSHIP` would otherwise replace its admin-only policies.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066
fn definitions_outside_the_application_range_are_skipped() {
    use xmtp_configuration::MetadataPolicy;
    let mut membership = definition(ComponentId::GROUP_MEMBERSHIP.as_u16(), true, true);
    let allow = Some(MetadataPolicy::Base(MetadataBasePolicy::Allow as i32));
    membership.permissions.insert = allow.clone();
    membership.permissions.update = allow.clone();
    membership.permissions.delete = allow;
    let expected = registry_of(&group_dictionary(
        ConversationType::Group,
        &policy_set(),
        &[],
    ));
    let registry = registry_of(&group_dictionary(
        ConversationType::Group,
        &policy_set(),
        &[membership.clone()],
    ));
    assert_eq!(
        registry.get(&ComponentId::GROUP_MEMBERSHIP)?,
        expected.get(&ComponentId::GROUP_MEMBERSHIP)?
    );
    assert_eq!(
        catalogue_registry_entries(&[membership], ConversationType::Group).count(),
        0
    );
}

/// Only the first definition of a repeated ID is registered, whether or
/// not the ID is immutable, so an unvalidated snapshot neither fails
/// creation nor lets a later duplicate replace the first.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066
fn only_the_first_definition_of_an_id_is_registered() {
    let catalogue: Vec<_> = [0xC000, 0xFD00]
        .into_iter()
        .flat_map(|id| {
            let mut duplicate = definition(id, true, true);
            duplicate.component_type = ComponentType::Bytes as i32;
            [definition(id, true, true), duplicate]
        })
        .collect();
    let registry = registry_of(&group_dictionary(
        ConversationType::Group,
        &policy_set(),
        &catalogue,
    ));
    for id in [0xC000, 0xFD00] {
        assert_eq!(
            registry
                .get(&ComponentId::new(id))?
                .map(|metadata| metadata.component_type),
            Some(ComponentType::String as i32)
        );
    }
}

/// A newer backend may publish a type or policy tag this build does not
/// know. Creation skips that definition instead of failing, and leaves
/// its ID absent so a client that knows the tags can register it.
#[xmtp_common::test(unwrap_try = true)]
// verifies: META-066
fn definitions_with_unknown_tags_are_skipped() {
    use xmtp_configuration::MetadataPolicy;
    let mut unknown_type = definition(0xC000, true, true);
    unknown_type.component_type = 99;
    let mut unknown_policy = definition(0xC001, true, true);
    unknown_policy.permissions.update = Some(MetadataPolicy::Any(vec![
        MetadataPolicy::Base(MetadataBasePolicy::Allow as i32),
        MetadataPolicy::Base(99),
    ]));
    let mut empty_condition = definition(0xC002, true, true);
    empty_condition.permissions.delete = Some(MetadataPolicy::And(vec![]));
    let catalogue = [
        unknown_type,
        unknown_policy,
        empty_condition,
        definition(0xC003, true, true),
    ];
    let registry = registry_of(&group_dictionary(
        ConversationType::Group,
        &policy_set(),
        &catalogue,
    ));
    for id in [0xC000, 0xC001, 0xC002] {
        assert!(!registry.contains_raw(&ComponentId::new(id)));
    }
    assert!(registry.contains(&ComponentId::new(0xC003)));
}
