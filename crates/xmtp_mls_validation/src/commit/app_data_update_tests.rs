//! Unit tests for the `AppDataUpdate` validator helpers.
//!
//! These cover the pure-logic seams of
//! `validate_one_app_data_update_with_old_value` and
//! `app_data_update_proposer_leaf`. The commit-time
//! wrapper `validate_one_app_data_update` adds only a dictionary read
//! on top of the pure core, so exercising the pure core plus the
//! sender dispatch gives us the full decision-tree without requiring a
//! real `OpenMlsGroup` / `StagedCommit`.

use std::collections::HashSet;

use openmls::messages::proposals::AppDataUpdateOperation;
use openmls::prelude::{LeafNodeIndex, Sender, SenderExtensionIndex};
use prost::Message as _;
use tls_codec::{Serialize as _, VLBytes};

use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_mls_common::app_data::component_permissions::component_permissions;
use xmtp_mls_common::app_data::component_registry::{ComponentRegistry, new_component_metadata};
use xmtp_mls_common::app_data::components::metadata_attributes::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
};
use xmtp_mls_common::app_data::validation::ActorAuthority;
use xmtp_mls_common::group_metadata::DmMembers;
use xmtp_mls_common::inbox_id::InboxId;
use xmtp_mls_common::tls_map::{TlsMap, TlsMapDelta, TlsMapMutation};
use xmtp_mls_common::tls_set::{TlsKeyHash, TlsSet, TlsSetDelta};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentType, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::{
    AppDataUpdateInCommit, CommitRuleError, app_data_update_proposer_leaf, removes_super_admin,
    validate_app_data_update_sequence, validate_one_app_data_update_with_old_value,
    validate_standalone_app_data_update,
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
        None,
        None,
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
        None,
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
        None,
        None,
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
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
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
        "expected InsufficientPermissions, got {err:?}"
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn remove_by_hash_miss_does_not_short_circuit_policy() {
    // RemoveByHash against an empty prior set surfaces `value: None`
    // from expansion. The actor has authority, but the delta cannot
    // produce a valid post-state because the removed entry is absent.
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
        None,
        None,
    );
    assert!(matches!(
        result,
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
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
    let mut prior = TlsSet::new();
    prior.insert(fake_inbox(0x03))?;
    let prior = prior.tls_serialize_detached()?;

    let result = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &op,
        super_admin(),
        "inbox_super",
        &reg,
        Some(&prior),
        None,
        None,
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ------------------------------------------------------------------------
// validate_one_app_data_update_with_old_value — receiver invariants
// ------------------------------------------------------------------------

/// Super admins stay members, so a membership proposal may not delete one.
/// An admin passes the membership policy; without this check a receiver
/// stores the proposal, and every later commit of pending proposals fails.
// verifies: GMOD-019
#[xmtp_common::test(unwrap_try = true)]
fn membership_update_may_not_remove_super_admin() {
    use xmtp_mls_common::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_common::app_data::typed::Component;

    let (super_admin, member) = (fake_inbox(0x42), fake_inbox(0x43));
    let mut super_admins = TlsSet::new();
    super_admins.insert(super_admin)?;
    let super_admins = SuperAdminListComponent::encode_value(&super_admins)?;
    let update = |delta: TlsMapDelta<InboxId, VLBytes>| {
        AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
    };
    let entry = || VLBytes::new(vec![1]);

    let removes =
        |operation: &AppDataUpdateOperation| removes_super_admin(operation, Some(&super_admins));
    assert!(removes(&update(TlsMapDelta::new().delete(super_admin))));
    assert!(removes(&update(
        TlsMapDelta::new().delete(member).delete(super_admin)
    )));
    // Commits omit a whole-component Remove; it removes nobody.
    assert!(!removes(&AppDataUpdateOperation::Remove));
    assert!(!removes(&update(TlsMapDelta::new().delete(member))));
    assert!(!removes(&update(
        TlsMapDelta::new().update(super_admin, entry())
    )));
    assert!(!removes_super_admin(
        &update(TlsMapDelta::new().delete(super_admin)),
        None
    ));
}

// verifies: PERM-004
#[xmtp_common::test(unwrap_try = true)]
fn receiver_rejects_last_super_admin_removal() {
    use xmtp_mls_common::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_common::app_data::typed::Component;

    let only_super_admin = fake_inbox(0x42);
    let mut prior = TlsSet::new();
    prior.insert(only_super_admin)?;
    let prior = SuperAdminListComponent::encode_value(&prior)?;
    let delta = TlsSetDelta::new().remove(only_super_admin);
    let operation = AppDataUpdateOperation::Update(delta.tls_serialize_detached()?.into());

    let err = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &operation,
        super_admin(),
        "inbox_super",
        &ComponentRegistry::new(),
        Some(&prior),
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

// verifies: PERM-004
#[xmtp_common::test(unwrap_try = true)]
fn receiver_rejects_second_of_two_sequential_super_admin_removals() {
    use xmtp_mls_common::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_common::app_data::typed::Component;

    let first = fake_inbox(0x41);
    let second = fake_inbox(0x42);
    let mut initial = TlsSet::new();
    initial.insert(first)?;
    initial.insert(second)?;
    let initial = SuperAdminListComponent::encode_value(&initial)?;
    let remove_first = AppDataUpdateOperation::Update(
        TlsSetDelta::new()
            .remove(first)
            .tls_serialize_detached()?
            .into(),
    );
    let registry = ComponentRegistry::new();
    validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &remove_first,
        super_admin(),
        "inbox_super",
        &registry,
        Some(&initial),
        None,
        None,
    )?;
    let after_first = SuperAdminListComponent::apply_update_payload(
        match &remove_first {
            AppDataUpdateOperation::Update(payload) => payload.as_slice(),
            AppDataUpdateOperation::Remove => unreachable!(),
        },
        Some(&initial),
    )?;
    let remove_second = AppDataUpdateOperation::Update(
        TlsSetDelta::new()
            .remove(second)
            .tls_serialize_detached()?
            .into(),
    );
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::SUPER_ADMIN_LIST,
        &remove_second,
        super_admin(),
        "inbox_super",
        &registry,
        Some(&after_first),
        None,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

/// The cases run in a loop instead of `#[rstest]` `#[case]` attributes on
/// purpose. This test is sync and is compiled into the wasm32 test binary, and
/// the wasm derivation sets `RSTEST_TIMEOUT`. With that variable set, `rstest`
/// wraps a sync case in `execute_with_timeout_sync`, which calls
/// `std::thread::spawn`. Thread spawning is unsupported on wasm32, so the whole
/// test binary aborts — this exact combination broke the "Run WASM tests" CI
/// step. Keep the loop; do not convert this back to `#[rstest]`.
#[xmtp_common::test(unwrap_try = true)]
fn receiver_rejects_overlong_metadata_app_data_update() {
    let cases: [(&str, ComponentId, usize); 4] = [
        ("name", ComponentId::GROUP_NAME, MAX_GROUP_NAME_LENGTH),
        (
            "description",
            ComponentId::GROUP_DESCRIPTION,
            MAX_GROUP_DESCRIPTION_LENGTH,
        ),
        (
            "image_url",
            ComponentId::GROUP_IMAGE_URL,
            MAX_GROUP_IMAGE_URL_LENGTH,
        ),
        ("app_data", ComponentId::APP_DATA, MAX_APP_DATA_LENGTH),
    ];

    for (case_name, component_id, max_length) in cases {
        let registry = registry_with(
            component_id,
            allow(),
            allow(),
            allow(),
            ComponentType::String,
        );
        let operation = AppDataUpdateOperation::Update(vec![b'x'; max_length + 1].into());

        let result = validate_one_app_data_update_with_old_value(
            component_id,
            &operation,
            member(),
            "inbox_member",
            &registry,
            None,
            None,
            None,
        );
        let err = result.expect_err(&format!(
            "case {case_name} ({component_id:?}, max {max_length}): an overlong \
             value was accepted, expected InsufficientPermissions"
        ));
        assert!(
            matches!(err, CommitRuleError::InsufficientPermissions),
            "case {case_name} ({component_id:?}, max {max_length}): expected \
             InsufficientPermissions, got {err:?}"
        );
    }
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
        matches!(err, CommitRuleError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}

#[test]
fn proposer_leaf_new_member_commit_rejected() {
    let sender = Sender::NewMemberCommit;
    let err = app_data_update_proposer_leaf(&sender).unwrap_err();
    assert!(
        matches!(err, CommitRuleError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}

#[test]
fn proposer_leaf_new_member_proposal_rejected() {
    let sender = Sender::NewMemberProposal;
    let err = app_data_update_proposer_leaf(&sender).unwrap_err();
    assert!(
        matches!(err, CommitRuleError::ActorNotMember),
        "expected ActorNotMember, got {err:?}"
    );
}

// ------------------------------------------------------------------------
// validate_one_app_data_update_with_old_value — unknown-component
// tolerance (XIP §2.2)
//
// These pin the relaxed-rejection branch added in jj `lmtv`. The
// receive-side accepts unknown ids inside the XMTP/application range
// opaquely (registry-policy still gates writes; per-component invariants
// are skipped because the receiver has no `Component` impl to consult).
// Reserved range `0xFF00+` still rejects.
// ------------------------------------------------------------------------

/// Unknown id with no registry entry: registry-policy validation runs
/// (deny-by-default) and rejects. The relaxation does NOT bypass
/// permissions — it only allows the validator to skip the per-component
/// invariant hook when no `Component` impl exists.
#[test]
fn unknown_component_in_xmtp_range_rejected_without_registry_entry() {
    let reg = ComponentRegistry::new();
    let op = AppDataUpdateOperation::Update(b"opaque".to_vec().into());
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::new(0x8FFF),
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
        "deny-by-default must fire for unknown ids without a registry entry, got {err:?}"
    );
}

/// Unknown id with a permissive registry entry: validation passes.
/// This is the "graceful unknown" path — old clients learn the new
/// component exists via the registry write that newer clients ship
/// alongside the component, and policy gates the write the same way it
/// would for a known component.
// verifies: PERM-014
#[test]
fn unknown_component_in_xmtp_range_allowed_when_registry_permits() {
    let reg = registry_with(
        ComponentId::new(0x8FFF),
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Update(b"opaque".to_vec().into());
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::new(0x8FFF),
        &op,
        member(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    );
    assert!(
        result.is_ok(),
        "unknown id with permissive registry must pass, got {result:?}"
    );
}

/// Same as the prior case but in the app range (`0xC000-0xFCFF`).
/// Confirms the type-aware dispatch is range-agnostic across the
/// XMTP / application id space.
#[test]
fn unknown_component_in_app_range_allowed_when_registry_permits() {
    let reg = registry_with(
        ComponentId::new(0xC123),
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Update(b"opaque".to_vec().into());
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::new(0xC123),
        &op,
        member(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    );
    assert!(
        result.is_ok(),
        "unknown app-range id with permissive registry must pass, got {result:?}"
    );
}

/// Reserved range `0xFF00+` is NOT in the tolerance predicate. The
/// validator falls through to the catch-all rejection path — these
/// slots are protocol-level and have no graceful-degrade story. We
/// can't construct a registry entry for a reserved-range id
/// (`ComponentRegistry::set` rejects them at construction), so this
/// only exercises the "no registry + reserved id" path. That's the
/// only production path anyway: a malicious sender can put a reserved
/// id on the wire, but they can't get a registry entry to back it.
#[test]
fn unknown_component_in_reserved_range_rejected_with_empty_registry() {
    let reg = ComponentRegistry::new();
    let op = AppDataUpdateOperation::Update(b"x".to_vec().into());
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::new(0xFF00),
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
        "reserved-range ids must be rejected, got {err:?}"
    );
}

/// Unknown id Remove with no prior value: registry-delete policy
/// gates this the same way as for known components.
#[test]
fn unknown_component_remove_with_no_prior_rejected_without_registry_entry() {
    let reg = ComponentRegistry::new();
    let op = AppDataUpdateOperation::Remove;
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::new(0x8FFF),
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
        "deny-by-default applies to Remove on unknown ids, got {err:?}"
    );
}

/// Unknown id Remove with permissive delete policy passes — same
/// contract as known components. Pins that the validator doesn't
/// require `old_value` to be present for the unknown-Remove path.
// verifies: PERM-014
#[test]
fn unknown_component_remove_allowed_when_registry_permits_delete() {
    let reg = registry_with(
        ComponentId::new(0x8FFF),
        allow(),
        allow(),
        allow(),
        ComponentType::Bytes,
    );
    let op = AppDataUpdateOperation::Remove;
    let result = validate_one_app_data_update_with_old_value(
        ComponentId::new(0x8FFF),
        &op,
        member(),
        "inbox_alice",
        &reg,
        None,
        None,
        None,
    );
    assert!(
        result.is_ok(),
        "Remove on unknown id with permissive delete-policy must pass, got {result:?}"
    );
}

/// Unknown `TlsSet`-typed component with a well-formed delta payload
/// but a malformed prior snapshot in the dict: the type-aware expander
/// fails when it tries to decode `old_value` as a `TlsSet`. Surfaces
/// as `InsufficientPermissions` (the validator's catch-all rejection
/// for any expand-time failure) and produces a log line tagged with
/// the component id so triage can distinguish "bad payload" from
/// "bad prior."
#[test]
fn unknown_component_update_with_malformed_prior_rejected() {
    let id = ComponentId::new(0x8FFF);
    let reg = registry_with(id, allow(), allow(), allow(), ComponentType::TlsSetBytes);
    let delta = TlsSetDelta::<VLBytes>::new()
        .remove_by_hash(TlsKeyHash::of(&VLBytes::new(b"x".to_vec())).unwrap());
    let payload = delta.tls_serialize_detached().unwrap();
    let op = AppDataUpdateOperation::Update(payload.into());

    // Prior bytes don't decode as a `TlsSet<VLBytes>` — the
    // RemoveByHash arm builds the prior hash index, which tls-codec
    // rejects.
    let corrupt_prior = b"\xDE\xAD\xBE\xEF";

    let err = validate_one_app_data_update_with_old_value(
        id,
        &op,
        super_admin(),
        "inbox_alice",
        &reg,
        Some(corrupt_prior),
        None,
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, CommitRuleError::InsufficientPermissions),
        "malformed prior on unknown id must reject, got {err:?}"
    );
}

// ------------------------------------------------------------------------
// Self-owned inbox maps and DM authority
// ------------------------------------------------------------------------

const PROFILE: ComponentId = ComponentId::new(0xC100);

fn self_owned() -> MetadataPolicyProto {
    base_policy(MetadataBasePolicy::AllowIfSelfOrNonMember)
}

fn profile_registry() -> ComponentRegistry {
    registry_with(
        PROFILE,
        self_owned(),
        self_owned(),
        self_owned(),
        ComponentType::TlsMapInboxIdString,
    )
}

fn profile_delta(mutation: TlsMapMutation<InboxId, VLBytes>) -> AppDataUpdateOperation {
    let payload = TlsMapDelta {
        mutations: vec![mutation],
    }
    .tls_serialize_detached()
    .unwrap();
    AppDataUpdateOperation::Update(payload.into())
}

fn profile_snapshot(entries: &[(InboxId, &str)]) -> Vec<u8> {
    let mut map = TlsMap::<InboxId, VLBytes>::new();
    for (inbox, name) in entries {
        map.insert(*inbox, VLBytes::new(name.as_bytes().to_vec()))
            .unwrap();
    }
    map.tls_serialize_detached().unwrap()
}

/// A member writes its own profile entry through the full expand and
/// policy path, and may not write another member's, so a proposer
/// cannot forge a display name.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
fn self_owned_profile_accepts_owner_and_rejects_other_member() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let members = HashSet::from([alice, bob]);
    let reg = profile_registry();
    let prior = profile_snapshot(&[(bob, "Bob")]);
    let own = profile_delta(TlsMapMutation::Insert {
        key: alice,
        value: VLBytes::new(b"Alice".to_vec()),
    });
    validate_one_app_data_update_with_old_value(
        PROFILE,
        &own,
        member(),
        &alice.to_hex(),
        &reg,
        Some(&prior),
        None,
        Some(&members),
    )?;
    let forged = profile_delta(TlsMapMutation::Update {
        key: bob,
        value: VLBytes::new(b"Mallory".to_vec()),
    });
    // Even a super admin cannot write another member's self-owned key.
    let err = validate_one_app_data_update_with_old_value(
        PROFILE,
        &forged,
        super_admin(),
        &alice.to_hex(),
        &reg,
        Some(&prior),
        None,
        Some(&members),
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

/// A string-map value that is not UTF-8 rejects the proposal, even on
/// the proposer's own key.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
fn string_map_rejects_invalid_utf8() {
    let alice = fake_inbox(1);
    let invalid = profile_delta(TlsMapMutation::Insert {
        key: alice,
        value: VLBytes::new(vec![0xC3, 0x28]),
    });
    let err = validate_one_app_data_update_with_old_value(
        PROFILE,
        &invalid,
        member(),
        &alice.to_hex(),
        &profile_registry(),
        None,
        None,
        Some(&HashSet::from([alice])),
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

/// Any member may delete a stale entry whose inbox is not in the
/// membership it is judged against, but not the entry of a current
/// member. The in-commit path passes the membership after the commit, so
/// a removal and its cleanup delete can share one commit; a
/// standalone proposal is judged against the committed membership, in
/// which the removed inbox is still present.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
fn self_owned_delete_uses_the_membership_it_is_given() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let reg = profile_registry();
    let prior = profile_snapshot(&[(alice, "Alice"), (bob, "Bob")]);
    let cleanup = profile_delta(TlsMapMutation::Delete { key: bob });
    let judge = |membership: &HashSet<InboxId>| {
        validate_one_app_data_update_with_old_value(
            PROFILE,
            &cleanup,
            member(),
            &alice.to_hex(),
            &reg,
            Some(&prior),
            None,
            Some(membership),
        )
    };
    judge(&HashSet::from([alice]))?;
    assert!(matches!(
        judge(&HashSet::from([alice, bob])),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A whole-component Remove has no inbox key, so the self-owned policy
/// denies it even for a super admin.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
fn self_owned_denies_whole_component_remove() {
    let alice = fake_inbox(1);
    let err = validate_one_app_data_update_with_old_value(
        PROFILE,
        &AppDataUpdateOperation::Remove,
        super_admin(),
        &alice.to_hex(),
        &profile_registry(),
        Some(&profile_snapshot(&[(alice, "Alice")])),
        None,
        Some(&HashSet::new()),
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

fn dm_of(one: InboxId, two: InboxId) -> DmMembers<String> {
    DmMembers {
        member_one_inbox_id: one.to_hex(),
        member_two_inbox_id: two.to_hex(),
    }
}

fn registry_insert(ids: &[ComponentId]) -> AppDataUpdateOperation {
    let entry = new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(allow())
            .delete(allow())
            .call(),
        ComponentType::Bytes,
    )
    .encode_to_vec();
    let payload = ids
        .iter()
        .fold(TlsMapDelta::<ComponentId, VLBytes>::new(), |delta, id| {
            delta.insert(*id, VLBytes::new(entry.clone()))
        })
        .tls_serialize_detached()
        .unwrap();
    AppDataUpdateOperation::Update(payload.into())
}

/// Either DM participant may register an application component, but not
/// a well-known one, and a non-participant may register neither.
// verifies: PERM-026
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_registers_application_components_only() {
    let (alice, bob, carol) = (fake_inbox(1), fake_inbox(2), fake_inbox(3));
    let dm = dm_of(alice, bob);
    let registry = ComponentRegistry::new();
    let register = |proposer: InboxId, ids: &[ComponentId]| {
        validate_one_app_data_update_with_old_value(
            ComponentId::COMPONENT_REGISTRY,
            &registry_insert(ids),
            member(),
            &proposer.to_hex(),
            &registry,
            None,
            Some(&dm),
            None,
        )
    };
    register(alice, &[PROFILE])?;
    register(bob, &[ComponentId::new(0xFD00)])?;
    assert!(register(alice, &[ComponentId::GROUP_IMAGE]).is_err());
    assert!(register(alice, &[PROFILE, ComponentId::GROUP_IMAGE]).is_err());
    assert!(register(carol, &[PROFILE]).is_err());
}

/// Immutability still holds for a DM participant: an immutable-range
/// registry entry, once present, cannot be updated.
// verifies: META-015
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_cannot_update_immutable_registry_entry() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let id = ComponentId::new(0xFD00);
    let registry = registry_with(id, allow(), allow(), allow(), ComponentType::Bytes);
    let entry = VLBytes::new(registry.get(&id)?.unwrap().encode_to_vec());
    let payload = TlsMapDelta::<ComponentId, VLBytes>::new()
        .update(id, entry)
        .tls_serialize_detached()?;
    let err = validate_one_app_data_update_with_old_value(
        ComponentId::COMPONENT_REGISTRY,
        &AppDataUpdateOperation::Update(payload.into()),
        member(),
        &alice.to_hex(),
        &registry,
        Some(&registry.to_bytes()?),
        Some(&dm_of(alice, bob)),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CommitRuleError::InsufficientPermissions));
}

/// A DM participant satisfies `ALLOW_IF_SUPER_ADMIN` on an application
/// component, and nobody else does.
// verifies: PERM-028
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_satisfies_super_admin_policy_on_application_component() {
    let (alice, bob, carol) = (fake_inbox(1), fake_inbox(2), fake_inbox(3));
    let super_admin_policy = base_policy(MetadataBasePolicy::AllowIfSuperAdmin);
    let registry = registry_with(
        PROFILE,
        super_admin_policy.clone(),
        super_admin_policy.clone(),
        super_admin_policy,
        ComponentType::Bytes,
    );
    let write = |proposer: InboxId| {
        validate_one_app_data_update_with_old_value(
            PROFILE,
            &AppDataUpdateOperation::Update(b"v".to_vec().into()),
            member(),
            &proposer.to_hex(),
            &registry,
            None,
            Some(&dm_of(alice, bob)),
            None,
        )
    };
    write(bob)?;
    assert!(write(carol).is_err());
}

/// An immutable application scalar accepts one authorized first write,
/// and rejects a write once it has a value.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
fn immutable_application_scalar_is_written_once() {
    let id = ComponentId::new(0xFD00);
    let registry = registry_with(id, deny(), allow(), deny(), ComponentType::Bytes);
    let write = |old_value: Option<&[u8]>| {
        validate_one_app_data_update_with_old_value(
            id,
            &AppDataUpdateOperation::Update(b"v".to_vec().into()),
            member(),
            "inbox_alice",
            &registry,
            old_value,
            None,
            None,
        )
    };
    write(None)?;
    assert!(write(Some(b"v")).is_err());
}

/// Inside one commit, a second proposal sees the state the first one
/// left, so an absent immutable component cannot be written twice by
/// splitting the writes across proposals.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
fn immutable_first_write_cannot_be_split_across_proposals() {
    let id = ComponentId::new(0xFD00);
    let registry = registry_with(id, deny(), allow(), deny(), ComponentType::Bytes);
    let (first, second) = (
        AppDataUpdateOperation::Update(b"a".to_vec().into()),
        AppDataUpdateOperation::Update(b"b".to_vec().into()),
    );
    let sequence = |operations: &[&AppDataUpdateOperation]| {
        validate_app_data_update_sequence(
            operations.iter().map(|&operation| AppDataUpdateInCommit {
                component_id: id,
                operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }),
            |_| None,
            &registry,
            None,
            None,
        )
    };
    assert_eq!(sequence(&[&first])?[&id], Some(b"a".to_vec()));
    assert!(matches!(
        sequence(&[&first, &second]),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A DM participant cannot grow an application map without limit by
/// writing entries one commit at a time: the commit that would push the
/// serialized snapshot past 65536 bytes is rejected.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_cannot_grow_application_map_past_snapshot_bound() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let entry = |key: u8| {
        let payload = TlsMapDelta::<VLBytes, VLBytes>::new()
            .insert(VLBytes::new(vec![key]), VLBytes::new(vec![0; 8000]))
            .tls_serialize_detached()
            .unwrap();
        AppDataUpdateOperation::Update(payload.into())
    };
    let dm = dm_of(alice, bob);
    let mut committed: Option<Vec<u8>> = None;
    let mut commit = |key: u8| {
        let operation = entry(key);
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: PROFILE,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: &alice.to_hex(),
            }],
            |_| committed.clone(),
            &registry,
            Some(&dm),
            None,
        )
        .map(|post| committed = post[&PROFILE].clone())
    };
    for key in 0..8 {
        commit(key)?;
    }
    assert!(matches!(
        commit(8),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A registry entry padded to `len` bytes by an unknown protobuf field,
/// which the registry decodes past and stores whole.
fn padded_registry_entry(len: usize) -> VLBytes {
    let mut entry = new_component_metadata(
        component_permissions()
            .insert(allow())
            .update(allow())
            .delete(allow())
            .call(),
        ComponentType::Bytes,
    )
    .encode_to_vec();
    // Field 15, length-delimited, with a two-byte length varint.
    let pad = len - entry.len() - 3;
    assert!((128..16384).contains(&pad));
    entry.extend([0x7A, pad as u8 | 0x80, (pad >> 7) as u8]);
    entry.resize(len, 0);
    VLBytes::new(entry)
}

fn registry_entry_insert(id: ComponentId, len: usize) -> AppDataUpdateOperation {
    let payload = TlsMapDelta::<ComponentId, VLBytes>::new()
        .insert(id, padded_registry_entry(len))
        .tls_serialize_detached()
        .unwrap();
    AppDataUpdateOperation::Update(payload.into())
}

/// A DM participant's standalone proposal is refused on receipt when it
/// registers an application component whose entry, unknown protobuf fields
/// included, is over the element bound.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_standalone_registry_entry_over_bound_is_rejected() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let dm = dm_of(alice, bob);
    let registry = ComponentRegistry::new();
    let propose = |len: usize| {
        validate_standalone_app_data_update(
            ComponentId::COMPONENT_REGISTRY,
            &registry_entry_insert(PROFILE, len),
            member(),
            &alice.to_hex(),
            &registry,
            None,
            Some(&dm),
            None,
        )
    };
    propose(8192)?;
    assert!(matches!(
        propose(8193),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A DM participant cannot grow the registry without limit by registering
/// application components one commit at a time: the commit that would push
/// the serialized registry past 65536 bytes is rejected.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn dm_participant_cannot_grow_registry_past_snapshot_bound() {
    let (alice, bob) = (fake_inbox(1), fake_inbox(2));
    let dm = dm_of(alice, bob);
    let registry = ComponentRegistry::new();
    let mut committed: Option<Vec<u8>> = None;
    let mut commit = |offset: u16| {
        let operation = registry_entry_insert(ComponentId::new(0xC100 + offset), 8000);
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: ComponentId::COMPONENT_REGISTRY,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: &alice.to_hex(),
            }],
            |_| committed.clone(),
            &registry,
            Some(&dm),
            None,
        )
        .map(|post| committed = post[&ComponentId::COMPONENT_REGISTRY].clone())
    };
    for offset in 0..8 {
        commit(offset)?;
    }
    assert!(matches!(
        commit(8),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// The first write of an absent immutable map is one insert-only delta: a
/// later Update of a key the same delta inserted would leave a value other
/// than the first one, so the commit is refused.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
fn commit_rejects_rewriting_an_immutable_map_key_inserted_in_the_same_delta() {
    let immutable = ComponentId::new(0xFD01);
    let registry = registry_with(
        immutable,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let commit = |delta: TlsMapDelta<VLBytes, VLBytes>| {
        let operation =
            AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into());
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: immutable,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }],
            |_| None,
            &registry,
            None,
            None,
        )
        .map(drop)
    };
    let key = || VLBytes::new(b"k".to_vec());
    let first = || TlsMapDelta::new().insert(key(), VLBytes::new(b"v1".to_vec()));
    commit(first())?;
    assert!(matches!(
        commit(first().update(key(), VLBytes::new(b"v2".to_vec()))),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A commit cannot carry an oversized map value by deleting it later in the
/// same payload: element bounds hold for every value the payload names, not
/// only those left in the final snapshot.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn commit_rejects_oversized_value_that_the_payload_later_deletes() {
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let commit = |len: usize| {
        let payload = TlsMapDelta::<VLBytes, VLBytes>::new()
            .insert(VLBytes::new(b"k".to_vec()), VLBytes::new(vec![0; len]))
            .delete(VLBytes::new(b"k".to_vec()))
            .tls_serialize_detached()
            .unwrap();
        let operation = AppDataUpdateOperation::Update(payload.into());
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: PROFILE,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }],
            |_| None,
            &registry,
            None,
            None,
        )
        .map(drop)
    };
    commit(8192)?;
    assert!(matches!(
        commit(8193),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A standalone proposal whose value is over a field bound is refused on
/// receipt, so members cannot fill each other's proposal stores with
/// values that no commit may carry.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn standalone_proposal_over_field_bound_is_rejected() {
    let registry = registry_with(PROFILE, allow(), allow(), allow(), ComponentType::Bytes);
    let propose = |len: usize| {
        validate_standalone_app_data_update(
            PROFILE,
            &AppDataUpdateOperation::Update(vec![0; len].into()),
            member(),
            "inbox_alice",
            &registry,
            None,
            None,
            None,
        )
    };
    propose(8192)?;
    assert!(matches!(
        propose(8193),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A standalone proposal that does not apply to the committed state is
/// still stored: its commit may order another proposal ahead of it, such
/// as the insert that creates the key this one updates.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn standalone_proposal_is_not_judged_on_committed_state_alone() {
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let payload = TlsMapDelta::<VLBytes, VLBytes>::new()
        .update(VLBytes::new(b"k".to_vec()), VLBytes::new(b"v".to_vec()))
        .tls_serialize_detached()?;
    validate_standalone_app_data_update(
        PROFILE,
        &AppDataUpdateOperation::Update(payload.into()),
        member(),
        "inbox_alice",
        &registry,
        None,
        None,
        None,
    )?;
}

/// An oversized map value is rejected even when its update cannot apply to
/// the committed state: element bounds are judged on the payload alone, so
/// the deferred state check cannot hide them.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn standalone_proposal_update_of_absent_key_over_bound_is_rejected() {
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let validate = |len: usize| {
        let payload = TlsMapDelta::<VLBytes, VLBytes>::new()
            .update(VLBytes::new(b"k".to_vec()), VLBytes::new(vec![0; len]))
            .tls_serialize_detached()
            .unwrap();
        validate_standalone_app_data_update(
            PROFILE,
            &AppDataUpdateOperation::Update(payload.into()),
            member(),
            "inbox_alice",
            &registry,
            None,
            None,
            None,
        )
    };
    validate(8192)?;
    assert!(matches!(
        validate(8193),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A delete naming an oversized key is rejected even though the key is
/// absent from the committed state: no stored key can be that long, so the
/// proposal would only spend proposal storage.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
fn standalone_proposal_delete_of_oversized_key_is_rejected() {
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let validate = |len: usize| {
        let payload = TlsMapDelta::<VLBytes, VLBytes>::new()
            .delete(VLBytes::new(vec![0; len]))
            .tls_serialize_detached()
            .unwrap();
        validate_standalone_app_data_update(
            PROFILE,
            &AppDataUpdateOperation::Update(payload.into()),
            member(),
            "inbox_alice",
            &registry,
            None,
            None,
            None,
        )
    };
    validate(8192)?;
    assert!(matches!(
        validate(8193),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}
