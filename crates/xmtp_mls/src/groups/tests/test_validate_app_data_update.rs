//! Unit tests for the `AppDataUpdate` validator helpers.
//!
//! These cover the pure-logic seams of
//! `validated_commit::validate_one_app_data_update_with_old_value` and
//! `validated_commit::app_data_update_proposer_leaf`. The commit-time
//! wrapper `validate_one_app_data_update` adds only a dictionary read
//! on top of the pure core, so exercising the pure core plus the
//! sender dispatch gives us the full decision-tree without requiring a
//! real `OpenMlsGroup` / `StagedCommit`.

use openmls::messages::proposals::AppDataUpdateOperation;
use openmls::prelude::{LeafNodeIndex, Sender, SenderExtensionIndex};
use tls_codec::Serialize as _;

use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_mls_common::app_data::component_permissions::component_permissions;
use xmtp_mls_common::app_data::component_registry::{ComponentRegistry, new_component_metadata};
use xmtp_mls_common::app_data::validation::ActorAuthority;
use xmtp_mls_common::inbox_id::InboxId;
use xmtp_mls_common::tls_set::{TlsKeyHash, TlsSetDelta};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentType, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use crate::groups::validated_commit::{
    CommitValidationError, app_data_update_proposer_leaf,
    validate_one_app_data_update_with_old_value,
};

// --- actor / policy / registry helpers -----------------------------------

fn member() -> ActorAuthority {
    ActorAuthority {
        is_admin: false,
        is_super_admin: false,
    }
}

fn admin() -> ActorAuthority {
    ActorAuthority {
        is_admin: true,
        is_super_admin: false,
    }
}

fn super_admin() -> ActorAuthority {
    ActorAuthority {
        is_admin: true,
        is_super_admin: true,
    }
}

fn base_policy(base: MetadataBasePolicy) -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(base as i32)),
    }
}

fn allow() -> MetadataPolicyProto {
    base_policy(MetadataBasePolicy::Allow)
}

fn deny() -> MetadataPolicyProto {
    base_policy(MetadataBasePolicy::Deny)
}

fn admin_only() -> MetadataPolicyProto {
    base_policy(MetadataBasePolicy::AllowIfAdmin)
}

/// Build a `ComponentRegistry` with a single entry permitting exactly
/// the insert/update/delete policies given.
fn registry_with(
    id: ComponentId,
    insert: MetadataPolicyProto,
    update: MetadataPolicyProto,
    delete: MetadataPolicyProto,
    component_type: ComponentType,
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
            component_type,
        ),
    )
    .unwrap();
    reg
}

fn fake_inbox(byte: u8) -> InboxId {
    InboxId::from_bytes([byte; 32])
}

// ------------------------------------------------------------------------
// validate_one_app_data_update_with_old_value — Bytes component happy paths
// ------------------------------------------------------------------------

#[test]
fn bytes_update_allowed_when_registry_allows() {
    let reg = registry_with(
        ComponentId::GROUP_NAME,
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Update(b"new-name".to_vec().into());
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::GROUP_NAME,
        &op,
        member(),
        "inbox_alice",
        &reg,
        Some(b"old-name"),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn bytes_update_accepts_none_old_value_for_first_write() {
    // First-write case: AppData dict has no prior bytes. Bytes-component
    // expansion ignores `old_value`, so this must succeed when the
    // policy allows.
    let reg = registry_with(
        ComponentId::GROUP_NAME,
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Update(b"first-name".to_vec().into());
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::GROUP_NAME,
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        None,
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn bytes_remove_allowed_when_registry_allows_delete() {
    let reg = registry_with(
        ComponentId::GROUP_NAME,
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Remove;
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::GROUP_NAME,
        &op,
        admin(),
        "inbox_alice",
        &reg,
        Some(b"y"),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ------------------------------------------------------------------------
// validate_one_app_data_update_with_old_value — permission rejections
// ------------------------------------------------------------------------

#[test]
fn bytes_update_rejected_when_registry_empty() {
    // Deny-by-default: component has no registry entry.
    let reg = ComponentRegistry::new();
    let op = AppDataUpdateOperation::Update(b"x".to_vec().into());
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::GROUP_NAME,
        &op,
        member(),
        "inbox_alice",
        &reg,
        Some(b"y"),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[test]
fn bytes_update_rejected_when_policy_denies() {
    // Update policy is explicitly Deny — even super_admin is rejected.
    let reg = registry_with(
        ComponentId::GROUP_NAME,
        allow(),
        deny(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Update(b"x".to_vec().into());
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::GROUP_NAME,
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        Some(b"y"),
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[test]
fn admin_list_insert_rejected_for_member() {
    // ADMIN_LIST is constrained to AllowIfAdmin / AllowIfSuperAdmin.
    // A plain member proposing an Insert is rejected.
    let reg = registry_with(
        ComponentId::ADMIN_LIST,
        admin_only(),
        admin_only(),
        admin_only(),
        ComponentType::TlsSetInboxId,
    );
    let alice = fake_inbox(0x11);
    let delta: TlsSetDelta<InboxId> = TlsSetDelta::new().insert(alice);
    let op = AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into());

    let err = validate_one_app_data_update_with_old_value(
        ComponentId::ADMIN_LIST,
        &op,
        member(),
        "inbox_member",
        &reg,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[test]
fn super_admin_list_insert_rejected_for_admin() {
    // SUPER_ADMIN_LIST is hardcoded super-admin-only, enforced in code
    // and not through the registry. An admin (but not super admin) is
    // rejected.
    let reg = ComponentRegistry::new();
    let alice = fake_inbox(0x11);
    let delta: TlsSetDelta<InboxId> = TlsSetDelta::new().insert(alice);
    let op = AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into());

    let err = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &op,
        admin(),
        "inbox_admin",
        &reg,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

// ------------------------------------------------------------------------
// validate_one_app_data_update_with_old_value — expansion-failure mapping
// ------------------------------------------------------------------------

#[test]
fn malformed_delta_maps_to_insufficient_permissions() {
    // Corrupt TlsSetDelta payload on ADMIN_LIST — the expansion step
    // surfaces a TlsCodec error. The validator intentionally collapses
    // that into InsufficientPermissions so an ill-formed proposal is
    // rejected wholesale; the underlying parse error is still logged
    // via `tracing::warn!` for debuggability.
    let reg = registry_with(
        ComponentId::ADMIN_LIST,
        admin_only(),
        admin_only(),
        admin_only(),
        ComponentType::TlsSetInboxId,
    );
    let op = AppDataUpdateOperation::Update(vec![0xde, 0xad, 0xbe, 0xef].into());

    let err = validate_one_app_data_update_with_old_value(
        ComponentId::ADMIN_LIST,
        &op,
        super_admin(),
        "inbox_super",
        &reg,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[test]
fn unknown_collection_component_maps_to_insufficient_permissions() {
    // A component in the XMTP range that has no expansion handler
    // (neither metadata-field-mapped nor ADMIN/SUPER_ADMIN_LIST) fails
    // expansion with UnknownComponent — which the validator flattens
    // to InsufficientPermissions.
    let reg = ComponentRegistry::new();
    // 0xBE00 is an XMTP-immutable id with no expansion handler.
    let op = AppDataUpdateOperation::Update(vec![0x00, 0x01].into());
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::new(0xBE00),
        &op,
        super_admin(),
        "inbox_super",
        &reg,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitValidationError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[test]
fn remove_by_hash_miss_does_not_short_circuit_policy() {
    // RemoveByHash against an empty prior set surfaces `value: None`
    // from the expansion. Per-change policy still runs — super_admin
    // on SUPER_ADMIN_LIST passes regardless of value, so the
    // validator must return Ok.
    let delta: TlsSetDelta<InboxId> =
        TlsSetDelta::new().remove_by_hash(TlsKeyHash::of(&fake_inbox(0x55)).unwrap());
    let op = AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into());
    let reg = ComponentRegistry::new();

    let result = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &op,
        super_admin(),
        "inbox_super",
        &reg,
        None,
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn multi_mutation_delta_all_allowed_returns_ok() {
    // Super admin inserting two new inboxes and removing one — all
    // three expanded per-element writes must pass. Cheap happy-path
    // check that the per-change loop doesn't spuriously reject.
    let delta: TlsSetDelta<InboxId> = TlsSetDelta::new()
        .insert(fake_inbox(0x01))
        .insert(fake_inbox(0x02))
        .remove(fake_inbox(0x03));
    let op = AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into());
    let reg = ComponentRegistry::new();

    let result = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &op,
        super_admin(),
        "inbox_super",
        &reg,
        None,
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ------------------------------------------------------------------------
// app_data_update_proposer_leaf — sender dispatch
// ------------------------------------------------------------------------

#[test]
fn proposer_leaf_member_returns_leaf_index() {
    let leaf = LeafNodeIndex::new(7);
    let sender = Sender::Member(leaf);
    let result = app_data_update_proposer_leaf(&sender).unwrap();
    assert_eq!(*result, leaf);
}

#[test]
fn proposer_leaf_external_rejected_as_actor_not_member() {
    let sender = Sender::External(SenderExtensionIndex::new(0));
    let err = app_data_update_proposer_leaf(&sender).unwrap_err();
    assert!(
        matches!(err, CommitValidationError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}

#[test]
fn proposer_leaf_new_member_commit_rejected() {
    let sender = Sender::NewMemberCommit;
    let err = app_data_update_proposer_leaf(&sender).unwrap_err();
    assert!(
        matches!(err, CommitValidationError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}

#[test]
fn proposer_leaf_new_member_proposal_rejected() {
    let sender = Sender::NewMemberProposal;
    let err = app_data_update_proposer_leaf(&sender).unwrap_err();
    assert!(
        matches!(err, CommitValidationError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}
