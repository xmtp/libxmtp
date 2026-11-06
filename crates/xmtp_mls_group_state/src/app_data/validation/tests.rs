use super::*;
use crate::app_data::component_permissions::component_permissions;
use crate::app_data::component_registry::new_component_metadata;
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentType, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

fn make_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(base as i32)),
    }
}

fn allow() -> MetadataPolicyProto {
    make_policy(MetadataBasePolicy::Allow)
}

fn deny() -> MetadataPolicyProto {
    make_policy(MetadataBasePolicy::Deny)
}

fn admin_only() -> MetadataPolicyProto {
    make_policy(MetadataBasePolicy::AllowIfAdmin)
}

fn super_admin_only() -> MetadataPolicyProto {
    make_policy(MetadataBasePolicy::AllowIfSuperAdmin)
}

fn make_actor(is_admin: bool, is_super_admin: bool) -> ActorAuthority {
    ActorAuthority {
        is_admin,
        is_super_admin,
    }
}

/// Test helper. Constructs a [`ComponentChange`] with no values, key,
/// proposer, or membership. Role policies read none of these; the
/// self-owned policy and immutability tests set them explicitly.
fn change<'a>(id: ComponentId, op: ComponentOp, actor: ActorAuthority) -> ComponentChange<'a> {
    ComponentChange::builder()
        .component_id(id)
        .op(op)
        .actor(actor)
        .build()
}

fn member() -> ActorAuthority {
    make_actor(false, false)
}

fn admin() -> ActorAuthority {
    make_actor(true, false)
}

fn super_admin() -> ActorAuthority {
    make_actor(true, true)
}

fn setup_registry_with(
    id: ComponentId,
    insert: MetadataPolicyProto,
    update: MetadataPolicyProto,
    delete: MetadataPolicyProto,
) -> ComponentRegistry {
    typed_registry(id, ComponentType::Bytes, insert, update, delete)
}

fn typed_registry(
    id: ComponentId,
    ty: ComponentType,
    insert: MetadataPolicyProto,
    update: MetadataPolicyProto,
    delete: MetadataPolicyProto,
) -> ComponentRegistry {
    let mut reg = ComponentRegistry::new();
    reg.set(
        id,
        new_component_metadata(
            component_permissions()
                .insert(insert)
                .update(update)
                .delete(delete)
                .call(),
            ty,
        ),
    )
    .unwrap();
    reg
}

// === Immutability Tests ===

#[xmtp_common::test]
// verifies: META-004
fn test_immutable_insert_allowed() {
    let id = ComponentId::CONVERSATION_TYPE;
    let reg = setup_registry_with(id, allow(), deny(), deny());
    let actor = super_admin();
    let result = validate_component_write(&change(id, ComponentOp::Insert, actor), &reg);
    assert!(result.is_ok());
}

/// An immutable component that already has a value rejects every
/// further write, whatever its policies allow.
#[xmtp_common::test]
// verifies: META-004
fn test_immutable_update_rejected() {
    let id = ComponentId::CONVERSATION_TYPE;
    let reg = setup_registry_with(id, allow(), allow(), allow());
    for op in [ComponentOp::Insert, ComponentOp::Update] {
        let written = ComponentChange::builder()
            .component_id(id)
            .op(op)
            .actor(super_admin())
            .old_value(b"dm")
            .build();
        assert!(matches!(
            validate_component_write(&written, &reg),
            Err(ComponentPermissionError::ImmutableViolation(_, got)) if got == op
        ));
    }
}

/// An absent immutable scalar accepts one initial Update, which its
/// registry policy then judges like any other write.
#[xmtp_common::test]
// verifies: META-004
fn test_immutable_first_scalar_write_uses_policy() {
    let id = ComponentId::new(0xFD00);
    let first = |actor| change(id, ComponentOp::Update, actor);
    let reg = setup_registry_with(id, deny(), super_admin_only(), deny());
    assert!(validate_component_write(&first(super_admin()), &reg).is_ok());
    assert!(matches!(
        validate_component_write(&first(admin()), &reg),
        Err(ComponentPermissionError::PermissionDenied(
            _,
            ComponentOp::Update
        ))
    ));
}

/// An absent immutable map takes insert-only mutations: a keyed
/// Update could only rewrite a value the same delta just inserted.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
fn test_immutable_absent_map_rejects_element_update() {
    let id = ComponentId::new(0xFD01);
    let reg = typed_registry(
        id,
        ComponentType::TlsMapBytesBytes,
        allow(),
        allow(),
        allow(),
    );
    let element = |op| {
        ComponentChange::builder()
            .component_id(id)
            .op(op)
            .actor(super_admin())
            .key(b"k")
            .build()
    };
    validate_component_write(&element(ComponentOp::Insert), &reg)?;
    assert!(matches!(
        validate_component_write(&element(ComponentOp::Update), &reg),
        Err(ComponentPermissionError::ImmutableViolation(
            _,
            ComponentOp::Update
        ))
    ));
}

#[xmtp_common::test]
// verifies: META-004
fn test_immutable_delete_rejected() {
    let id = ComponentId::CONVERSATION_TYPE;
    let reg = setup_registry_with(id, allow(), allow(), allow());
    let actor = super_admin();
    let result = validate_component_write(&change(id, ComponentOp::Delete, actor), &reg);
    assert!(matches!(
        result,
        Err(ComponentPermissionError::ImmutableViolation(
            _,
            ComponentOp::Delete
        ))
    ));
}

// === Hardcoded Tests ===

#[xmtp_common::test]
fn test_registry_super_admin_allowed() {
    let reg = ComponentRegistry::new();
    let actor = super_admin();
    let result = validate_component_write(
        &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
// verifies: PERM-026
fn test_registry_admin_rejected() {
    let reg = ComponentRegistry::new();
    let actor = admin();
    let result = validate_component_write(
        &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::SuperAdminRequired(_))
    ));
}

#[xmtp_common::test]
// verifies: PERM-026
fn test_registry_member_rejected() {
    let reg = ComponentRegistry::new();
    let actor = member();
    let result = validate_component_write(
        &change(ComponentId::COMPONENT_REGISTRY, ComponentOp::Update, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::SuperAdminRequired(_))
    ));
}

#[xmtp_common::test]
fn test_super_admin_list_super_admin_allowed() {
    let reg = ComponentRegistry::new();
    let actor = super_admin();
    let result = validate_component_write(
        &change(ComponentId::SUPER_ADMIN_LIST, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
// verifies: PERM-026
fn test_super_admin_list_admin_rejected() {
    let reg = ComponentRegistry::new();
    let actor = admin();
    let result = validate_component_write(
        &change(ComponentId::SUPER_ADMIN_LIST, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::SuperAdminRequired(_))
    ));
}

#[xmtp_common::test]
fn test_admin_list_with_admin_policy_admin_allowed() {
    let reg = setup_registry_with(
        ComponentId::ADMIN_LIST,
        admin_only(),
        admin_only(),
        admin_only(),
    );
    let actor = admin();
    let result = validate_component_write(
        &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
fn test_admin_list_with_admin_policy_member_rejected() {
    let reg = setup_registry_with(
        ComponentId::ADMIN_LIST,
        admin_only(),
        admin_only(),
        admin_only(),
    );
    let actor = member();
    let result = validate_component_write(
        &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::PermissionDenied(_, _))
    ));
}

#[xmtp_common::test]
fn test_admin_list_with_super_admin_policy() {
    let reg = setup_registry_with(
        ComponentId::ADMIN_LIST,
        super_admin_only(),
        super_admin_only(),
        super_admin_only(),
    );
    let admin_actor = admin();
    let super_admin_actor = super_admin();
    // Admin rejected
    assert!(
        validate_component_write(
            &change(ComponentId::ADMIN_LIST, ComponentOp::Insert, admin_actor),
            &reg,
        )
        .is_err()
    );
    // Super admin allowed
    assert!(
        validate_component_write(
            &change(
                ComponentId::ADMIN_LIST,
                ComponentOp::Insert,
                super_admin_actor,
            ),
            &reg,
        )
        .is_ok()
    );
}

// === Registry Lookup Tests ===

#[xmtp_common::test]
// verifies: PERM-012
fn test_deny_by_default_no_entry() {
    let reg = ComponentRegistry::new();
    let actor = super_admin();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::NoRegistryEntry(_))
    ));
}

#[xmtp_common::test]
// verifies: PERM-027
fn test_insert_allow_policy() {
    let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), deny(), deny());
    let actor = member();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Insert, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
// verifies: PERM-027
fn test_update_admin_only_policy_admin_passes() {
    let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), admin_only(), deny());
    let actor = admin();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Update, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
// verifies: PERM-027
fn test_update_admin_only_policy_member_fails() {
    let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), admin_only(), deny());
    let actor = member();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Update, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::PermissionDenied(
            _,
            ComponentOp::Update
        ))
    ));
}

#[xmtp_common::test]
// verifies: PERM-027
fn test_delete_deny_policy() {
    let reg = setup_registry_with(ComponentId::GROUP_NAME, allow(), allow(), deny());
    let actor = super_admin();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Delete, actor),
        &reg,
    );
    assert!(matches!(
        result,
        Err(ComponentPermissionError::PermissionDenied(
            _,
            ComponentOp::Delete
        ))
    ));
}

#[xmtp_common::test]
// verifies: PERM-027
fn test_delete_super_admin_only_policy() {
    let reg = setup_registry_with(
        ComponentId::GROUP_NAME,
        allow(),
        allow(),
        super_admin_only(),
    );
    let actor = super_admin();
    let result = validate_component_write(
        &change(ComponentId::GROUP_NAME, ComponentOp::Delete, actor),
        &reg,
    );
    assert!(result.is_ok());
}

#[xmtp_common::test]
// verifies: PERM-011
fn test_different_insert_vs_update_permissions() {
    // Mimics group membership: anyone can update, only admin can insert
    let reg = setup_registry_with(
        ComponentId::GROUP_MEMBERSHIP,
        admin_only(),
        allow(),
        admin_only(),
    );
    let member_actor = member();
    let admin_actor = admin();

    // Member can update
    assert!(
        validate_component_write(
            &change(
                ComponentId::GROUP_MEMBERSHIP,
                ComponentOp::Update,
                member_actor,
            ),
            &reg,
        )
        .is_ok()
    );

    // Member cannot insert
    assert!(
        validate_component_write(
            &change(
                ComponentId::GROUP_MEMBERSHIP,
                ComponentOp::Insert,
                member_actor,
            ),
            &reg,
        )
        .is_err()
    );

    // Admin can insert
    assert!(
        validate_component_write(
            &change(
                ComponentId::GROUP_MEMBERSHIP,
                ComponentOp::Insert,
                admin_actor,
            ),
            &reg,
        )
        .is_ok()
    );
}

#[xmtp_common::test]
// verifies: PERM-014
fn test_app_range_component() {
    let app_id = ComponentId::new(0xC100);
    let reg = setup_registry_with(app_id, allow(), allow(), deny());
    let actor = member();
    let result = validate_component_write(&change(app_id, ComponentOp::Insert, actor), &reg);
    assert!(result.is_ok());
}

// === Self-owned policy ===

const PROFILE: ComponentId = ComponentId::new(0xC100);

fn inbox(byte: u8) -> InboxId {
    InboxId::from_bytes([byte; 32])
}

fn self_owned() -> MetadataPolicyProto {
    make_policy(MetadataBasePolicy::AllowIfSelfOrNonMember)
}

fn self_owned_registry(id: ComponentId, ty: ComponentType) -> ComponentRegistry {
    typed_registry(id, ty, self_owned(), self_owned(), self_owned())
}

/// Validate `op` on `key` in `id` by `proposer`, a plain member, with
/// `membership` after the commit.
fn write_key(
    reg: &ComponentRegistry,
    id: ComponentId,
    op: ComponentOp,
    proposer: InboxId,
    key: Option<InboxId>,
    membership: &HashSet<InboxId>,
) -> Result<(), ComponentPermissionError> {
    let key = key.map(|key| key.tls_serialize_detached().unwrap());
    let change = ComponentChange::builder()
        .component_id(id)
        .op(op)
        .actor(member())
        .proposer(proposer)
        .maybe_key(key.as_deref())
        .membership(membership)
        .build();
    validate_component_write(&change, reg)
}

/// A member may write its own entry of an inbox-keyed map, and no
/// member may write another's, so a display name cannot be forged.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-027
fn self_owned_writes_only_own_key() {
    let (alice, bob) = (inbox(1), inbox(2));
    let members = HashSet::from([alice, bob]);
    for ty in [
        ComponentType::TlsMapInboxIdBytes,
        ComponentType::TlsMapInboxIdString,
    ] {
        let reg = self_owned_registry(PROFILE, ty);
        for op in [
            ComponentOp::Insert,
            ComponentOp::Update,
            ComponentOp::Delete,
        ] {
            write_key(&reg, PROFILE, op, alice, Some(alice), &members)?;
            assert!(matches!(
                write_key(&reg, PROFILE, op, alice, Some(bob), &members),
                Err(ComponentPermissionError::PermissionDenied(_, got)) if got == op
            ));
        }
    }
}

/// Any member may delete the entry of an inbox that is not a member
/// after the commit, so a departed member's data can be cleaned up,
/// but may not insert or update it.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-027
fn self_owned_deletes_non_member_key() {
    let (alice, gone) = (inbox(1), inbox(3));
    let members = HashSet::from([alice]);
    let reg = self_owned_registry(PROFILE, ComponentType::TlsMapInboxIdString);
    write_key(
        &reg,
        PROFILE,
        ComponentOp::Delete,
        alice,
        Some(gone),
        &members,
    )?;
    for op in [ComponentOp::Insert, ComponentOp::Update] {
        assert!(write_key(&reg, PROFILE, op, alice, Some(gone), &members).is_err());
    }
}

/// The policy fails closed without an inbox key: a whole-component
/// Remove, a component that is not an inbox-keyed map, a key that is
/// not an inbox id, or a missing proposer or membership.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-027
fn self_owned_fails_closed_without_context() {
    let (alice, gone) = (inbox(1), inbox(3));
    let members = HashSet::from([alice]);
    let map = self_owned_registry(PROFILE, ComponentType::TlsMapInboxIdString);
    assert!(write_key(&map, PROFILE, ComponentOp::Delete, alice, None, &members).is_err());

    for ty in [
        ComponentType::TlsMapBytesBytes,
        ComponentType::TlsSetInboxId,
    ] {
        let reg = self_owned_registry(PROFILE, ty);
        assert!(
            write_key(
                &reg,
                PROFILE,
                ComponentOp::Insert,
                alice,
                Some(alice),
                &members
            )
            .is_err()
        );
    }

    // A well-known id keeps its fixed type, not its registry entry's.
    let forged = self_owned_registry(ComponentId::GROUP_NAME, ComponentType::TlsMapInboxIdBytes);
    assert!(
        write_key(
            &forged,
            ComponentId::GROUP_NAME,
            ComponentOp::Update,
            alice,
            Some(alice),
            &members
        )
        .is_err()
    );

    let short_key = ComponentChange::builder()
        .component_id(PROFILE)
        .op(ComponentOp::Insert)
        .actor(member())
        .proposer(alice)
        .key(&[0u8; 4])
        .membership(&members)
        .build();
    assert!(validate_component_write(&short_key, &map).is_err());

    // Without a proposer no key is its own; without a membership no
    // key is a non-member's.
    let own_key = alice.tls_serialize_detached()?;
    let no_proposer = ComponentChange::builder()
        .component_id(PROFILE)
        .op(ComponentOp::Delete)
        .actor(super_admin())
        .key(&own_key)
        .membership(&members)
        .build();
    assert!(validate_component_write(&no_proposer, &map).is_err());
    let gone_key = gone.tls_serialize_detached()?;
    let no_membership = ComponentChange {
        proposer: Some(alice),
        key: Some(&gone_key),
        membership: None,
        ..no_proposer
    };
    assert!(validate_component_write(&no_membership, &map).is_err());
}

// === DM authority ===

fn dm_change<'a>(id: ComponentId, op: ComponentOp, key: Option<&'a [u8]>) -> ComponentChange<'a> {
    ComponentChange::builder()
        .component_id(id)
        .op(op)
        .actor(member())
        .is_dm_participant(true)
        .maybe_key(key)
        .build()
}

/// A DM participant may change a registry entry whose key is an
/// application id, but not a well-known entry, not the super-admin
/// list, and not the whole registry.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-026
fn dm_participant_changes_only_application_registry_entries() {
    let reg = ComponentRegistry::new();
    let registry = ComponentId::COMPONENT_REGISTRY;
    for id in [0xC000, 0xFEFF] {
        let key = ComponentId::new(id).tls_serialize_detached()?;
        for op in [
            ComponentOp::Insert,
            ComponentOp::Update,
            ComponentOp::Delete,
        ] {
            validate_component_write(&dm_change(registry, op, Some(&key)), &reg)?;
        }
    }
    for id in [
        ComponentId::GROUP_NAME,
        ComponentId::new(0xBFFF),
        ComponentId::new(0xFF00),
    ] {
        let key = id.tls_serialize_detached()?;
        assert!(matches!(
            validate_component_write(&dm_change(registry, ComponentOp::Insert, Some(&key)), &reg),
            Err(ComponentPermissionError::SuperAdminRequired(_))
        ));
    }
    assert!(
        validate_component_write(&dm_change(registry, ComponentOp::Delete, None), &reg).is_err()
    );
    let app_key = ComponentId::new(0xC000).tls_serialize_detached()?;
    assert!(
        validate_component_write(
            &dm_change(
                ComponentId::SUPER_ADMIN_LIST,
                ComponentOp::Insert,
                Some(&app_key)
            ),
            &reg
        )
        .is_err()
    );
    let outsider = ComponentChange {
        is_dm_participant: false,
        ..dm_change(registry, ComponentOp::Insert, Some(&app_key))
    };
    assert!(validate_component_write(&outsider, &reg).is_err());
}

/// A DM participant satisfies `ALLOW_IF_SUPER_ADMIN` on an application
/// component and on nothing else.
#[xmtp_common::test(unwrap_try = true)]
// verifies: PERM-028
fn dm_participant_is_super_admin_for_application_components_only() {
    for id in [ComponentId::new(0xC000), ComponentId::new(0xFEFF)] {
        let reg = setup_registry_with(id, super_admin_only(), super_admin_only(), deny());
        validate_component_write(&dm_change(id, ComponentOp::Update, None), &reg)?;
        assert!(validate_component_write(&dm_change(id, ComponentOp::Delete, None), &reg).is_err());
    }
    for id in [
        ComponentId::GROUP_NAME,
        ComponentId::GROUP_MEMBERSHIP,
        ComponentId::new(0xBE00),
    ] {
        let reg = setup_registry_with(id, super_admin_only(), super_admin_only(), deny());
        assert!(matches!(
            validate_component_write(&dm_change(id, ComponentOp::Insert, None), &reg),
            Err(ComponentPermissionError::PermissionDenied(_, _))
        ));
    }
}
