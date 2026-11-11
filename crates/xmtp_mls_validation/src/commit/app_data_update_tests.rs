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

use xmtp_cryptography::secret::Zeroizing;
use xmtp_mls_group_state::app_data::component_id::ComponentId;
use xmtp_mls_group_state::app_data::component_permissions::component_permissions;
use xmtp_mls_group_state::app_data::component_registry::{
    ComponentRegistry, new_component_metadata,
};
use xmtp_mls_group_state::app_data::components::metadata_attributes::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
};
use xmtp_mls_group_state::app_data::validation::{
    ActorAuthority, ValidationWork, count_validation_work,
};
use xmtp_mls_group_state::app_data::working_set::counters::count_snapshot_decodes;
use xmtp_mls_group_state::group_metadata::DmMembers;
use xmtp_mls_group_state::inbox_id::InboxId;
use xmtp_mls_tls_collections::tls_map::{TlsMap, TlsMapDelta, TlsMapMutation};
use xmtp_mls_tls_collections::tls_set::{TlsKeyHash, TlsSet, TlsSetDelta};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentType, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{AndCondition, Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::{
    AppDataUpdateInCommit, CommitRuleError, MAX_DICTIONARY_BYTES, app_data_update_proposer_leaf,
    removes_super_admin, validate_app_data_update_sequence,
    validate_one_app_data_update_with_old_value, validate_post_dictionary_size,
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
    use xmtp_mls_group_state::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_group_state::app_data::typed::Component;

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
    use xmtp_mls_group_state::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_group_state::app_data::typed::Component;

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
    use xmtp_mls_group_state::app_data::components::inbox_id_set::SuperAdminListComponent;
    use xmtp_mls_group_state::app_data::typed::Component;

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
    assert_eq!(
        sequence(&[&first])?[&id],
        Some(Zeroizing::new(b"a".to_vec()))
    );
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
    let mut committed: Option<Zeroizing<Vec<u8>>> = None;
    let mut commit = |key: u8| {
        let operation = entry(key);
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: PROFILE,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: &alice.to_hex(),
            }],
            |_| committed.as_deref().map(Vec::as_slice),
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
    let mut committed: Option<Zeroizing<Vec<u8>>> = None;
    let mut commit = |offset: u16| {
        let operation = registry_entry_insert(ComponentId::new(0xC100 + offset), 8000);
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: ComponentId::COMPONENT_REGISTRY,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: &alice.to_hex(),
            }],
            |_| committed.as_deref().map(Vec::as_slice),
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

/// A write of `GROUP_ROOT_KEY` must be exactly 32 bytes, the length META
/// section 2 gives it: an empty, 31-byte, or 33-byte first write is
/// rejected even where the registry allows the write.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
fn group_root_key_writes_must_be_32_bytes() {
    let id = ComponentId::GROUP_ROOT_KEY;
    // A scalar first write is an update, so the registry allows updates.
    let registry = registry_with(id, deny(), allow(), deny(), ComponentType::Bytes);
    let write = |len: usize| {
        validate_one_app_data_update_with_old_value(
            id,
            &AppDataUpdateOperation::Update(vec![7; len].into()),
            member(),
            "inbox_alice",
            &registry,
            None,
            None,
            None,
        )
    };
    write(32)?;
    for len in [0, 31, 33] {
        assert!(write(len).is_err(), "a {len}-byte root was accepted");
    }
}

/// A commit may grow the summed values of every component other than
/// `GROUP_MEMBERSHIP` to 4 MiB and no further. A dictionary already past
/// the limit stays usable for commits that do not grow it, and membership
/// bytes are not counted.
// verifies: META-077
#[xmtp_common::test(unwrap_try = true)]
fn dictionary_growth_past_the_limit_is_rejected() {
    const APP: ComponentId = ComponentId::new(0xC077);
    const MEMBERS: ComponentId = ComponentId::GROUP_MEMBERSHIP;
    const MAX: usize = MAX_DICTIONARY_BYTES;
    // The committed `APP` and membership lengths, the component the commit
    // writes and its new length, and whether the commit is accepted.
    let rows = [
        (MAX / 2, 0, APP, MAX, true),
        (MAX / 2, 0, APP, MAX + 1, false),
        (MAX + 2, 0, APP, MAX + 1, true),
        (MAX + 2, 0, APP, MAX + 2, true),
        (MAX + 1, 0, APP, MAX + 2, false),
        (MAX, MAX / 2, MEMBERS, MAX, true),
    ];
    for (row, (app, members, id, len, accepted)) in rows.into_iter().enumerate() {
        let mut committed = openmls::extensions::AppDataDictionary::new();
        committed.insert(APP.as_u16(), vec![0; app]);
        committed.insert(MEMBERS.as_u16(), vec![0; members]);
        let state = Some(Zeroizing::new(vec![0; len]));
        let result = validate_post_dictionary_size(Some(&committed), [(id, &state)], MAX);
        assert_eq!(result.is_ok(), accepted, "row {row}: {result:?}");
    }
}

/// A commit builder passes a bound below the receivers' 4 MiB. Growth past
/// that bound is rejected, and a dictionary already above it can still
/// shrink or stay the same.
// verifies: META-078
#[xmtp_common::test(unwrap_try = true)]
fn a_builder_bound_rejects_only_growth_past_it() {
    const APP: ComponentId = ComponentId::new(0xC078);
    const MAX: usize = 1024;
    // The committed `APP` length, its new length, and whether the commit is
    // accepted.
    let rows = [
        (0, MAX, true),
        (0, MAX + 1, false),
        (MAX + 8, MAX + 4, true),
        (MAX + 8, MAX + 8, true),
        (MAX + 8, MAX + 9, false),
    ];
    for (row, (app, len, accepted)) in rows.into_iter().enumerate() {
        let mut committed = openmls::extensions::AppDataDictionary::new();
        committed.insert(APP.as_u16(), vec![0; app]);
        let state = Some(Zeroizing::new(vec![0; len]));
        let result = validate_post_dictionary_size(Some(&committed), [(APP, &state)], MAX);
        assert_eq!(result.is_ok(), accepted, "row {row}: {result:?}");
    }
}

// ------------------------------------------------------------------------
// Empty set and map deltas
// ------------------------------------------------------------------------

fn super_admin_only() -> MetadataPolicyProto {
    base_policy(MetadataBasePolicy::AllowIfSuperAdmin)
}

/// An `Update` whose set or map delta has no mutations.
fn empty_delta(component_type: ComponentType) -> AppDataUpdateOperation {
    let payload = match component_type {
        ComponentType::TlsSetInboxId => TlsSetDelta::<InboxId>::new().tls_serialize_detached(),
        ComponentType::TlsMapBytesBytes => {
            TlsMapDelta::<VLBytes, VLBytes>::new().tls_serialize_detached()
        }
        other => panic!("no empty delta for {other:?}"),
    };
    AppDataUpdateOperation::Update(payload.unwrap().into())
}

/// Judge one proposal on receipt and inside a commit, against `old_value`.
fn standalone_and_in_commit(
    id: ComponentId,
    operation: &AppDataUpdateOperation,
    actor: ActorAuthority,
    registry: &ComponentRegistry,
    old_value: Option<&[u8]>,
    dm_members: Option<&DmMembers<String>>,
) -> [Result<(), CommitRuleError>; 2] {
    let proposer = fake_inbox(0x11).to_hex();
    [
        validate_standalone_app_data_update(
            id, operation, actor, &proposer, registry, old_value, dm_members, None,
        ),
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: id,
                operation,
                actor,
                proposer_inbox_id: &proposer,
            }],
            |_| old_value,
            registry,
            dm_members,
            None,
        )
        .map(drop),
    ]
}

/// An empty delta to an absent immutable application component is the
/// component's one first write, so it needs the insert policy: a member
/// cannot make it where only a super admin may. Both a set and a map, on
/// receipt and inside a commit.
// verifies: PERM-011, META-004
#[xmtp_common::test(unwrap_try = true)]
fn empty_delta_to_absent_immutable_component_is_authorized() {
    for (id, component_type) in [
        (ComponentId::new(0xFD01), ComponentType::TlsSetInboxId),
        (ComponentId::new(0xFD02), ComponentType::TlsMapBytesBytes),
    ] {
        let registry = registry_with(id, super_admin_only(), deny(), deny(), component_type);
        let operation = empty_delta(component_type);
        for result in standalone_and_in_commit(id, &operation, member(), &registry, None, None) {
            assert!(
                matches!(result, Err(CommitRuleError::InsufficientPermissions)),
                "{component_type:?}: {result:?}"
            );
        }
        for result in standalone_and_in_commit(id, &operation, super_admin(), &registry, None, None)
        {
            result?;
        }
    }
}

/// An empty delta is an insert when the component is absent and an update
/// when it has a value, for a built-in set and for a map with no built-in
/// definition.
// verifies: PERM-011
#[xmtp_common::test(unwrap_try = true)]
fn empty_delta_selects_insert_or_update_policy() {
    let present_set = TlsSet::<InboxId>::from_iter([fake_inbox(0x22)])
        .tls_serialize_detached()
        .unwrap();
    let present_map = TlsMap::<VLBytes, VLBytes>::new()
        .tls_serialize_detached()
        .unwrap();
    // The component, its type, its insert policy, the actor that policy
    // admits, and a present value.
    let cases = [
        (
            ComponentId::ADMIN_LIST,
            ComponentType::TlsSetInboxId,
            admin_only(),
            admin(),
            present_set,
        ),
        (
            ComponentId::new(0xC0E0),
            ComponentType::TlsMapBytesBytes,
            allow(),
            member(),
            present_map,
        ),
    ];
    for (id, component_type, insert, inserter, present) in cases {
        let registry = registry_with(id, insert, super_admin_only(), deny(), component_type);
        let operation = empty_delta(component_type);
        let write = |actor, old_value| {
            validate_one_app_data_update_with_old_value(
                id,
                &operation,
                actor,
                "inbox_alice",
                &registry,
                old_value,
                None,
                None,
            )
        };
        write(inserter, None)?;
        assert!(
            matches!(
                write(inserter, Some(&present)),
                Err(CommitRuleError::InsufficientPermissions)
            ),
            "{id:?}: an update was judged under the insert policy"
        );
        write(super_admin(), None)?;
        write(super_admin(), Some(&present))?;
    }
}

/// An honest empty delta still creates an absent application map when the
/// actor's insert policy allows it, even where updates are denied.
// verifies: PERM-011
#[xmtp_common::test(unwrap_try = true)]
fn empty_delta_creates_absent_map_when_insert_allowed() {
    let id = ComponentId::new(0xC0E1);
    let registry = registry_with(id, allow(), deny(), deny(), ComponentType::TlsMapBytesBytes);
    let operation = empty_delta(ComponentType::TlsMapBytesBytes);
    let post = validate_app_data_update_sequence(
        [AppDataUpdateInCommit {
            component_id: id,
            operation: &operation,
            actor: member(),
            proposer_inbox_id: "inbox_alice",
        }],
        |_| None,
        &registry,
        None,
        None,
    )?;
    let empty_map = TlsMap::<VLBytes, VLBytes>::new().tls_serialize_detached()?;
    assert_eq!(post[&id], Some(Zeroizing::new(empty_map)));
}

/// No proposal writes `DM_MEMBERS`: not an empty delta or a pair, not a
/// member or a super admin, not in a group or a DM, and not after a super
/// admin registers the component with an allowing policy.
// verifies: META-030
#[xmtp_common::test(unwrap_try = true)]
fn dm_members_writes_are_rejected_in_every_conversation() {
    let id = ComponentId::DM_MEMBERS;
    let (alice, bob) = (fake_inbox(0x11), fake_inbox(0x22));
    let pair = TlsSet::<InboxId>::from_iter([alice, bob])
        .tls_serialize_detached()
        .unwrap();
    let dm = dm_of(alice, bob);
    let insert_pair = AppDataUpdateOperation::Update(
        TlsSetDelta::<InboxId>::new()
            .insert(alice)
            .insert(bob)
            .tls_serialize_detached()
            .unwrap()
            .into(),
    );
    let operations = [empty_delta(ComponentType::TlsSetInboxId), insert_pair];
    let registered = registry_with(id, allow(), allow(), allow(), ComponentType::TlsSetInboxId);
    // The registry, the committed pair, and the DM pair of the conversation.
    let conversations = [
        (ComponentRegistry::new(), None, None),
        (registered, None, None),
        (ComponentRegistry::new(), Some(pair.as_slice()), Some(&dm)),
    ];
    for (registry, old_value, dm_members) in &conversations {
        for operation in &operations {
            for actor in [member(), super_admin()] {
                for result in standalone_and_in_commit(
                    id,
                    operation,
                    actor,
                    registry,
                    *old_value,
                    *dm_members,
                ) {
                    assert!(
                        matches!(result, Err(CommitRuleError::InsufficientPermissions)),
                        "{result:?}"
                    );
                }
            }
        }
    }
}

// ------------------------------------------------------------------------
// Validation cost
// ------------------------------------------------------------------------

/// Base policies in [`wide`].
const WIDE: u64 = 1_000;

/// Policy questions one component can raise: three operations, four role
/// pairs, and the self-or-non-member answer.
const POLICY_QUESTIONS: u64 = 3 * 4 * 2;

/// An `And` of [`WIDE`] `Allow` policies, so that each walk of it counts.
fn wide() -> MetadataPolicyProto {
    MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::AndCondition(AndCondition {
            policies: vec![allow(); WIDE as usize],
        })),
    }
}

/// A `TlsMapBytesBytes` component whose updates [`wide`] governs, and one
/// proposal that inserts a key and updates it 10,000 times.
fn repeated_updates() -> (ComponentRegistry, AppDataUpdateOperation) {
    let registry = registry_with(
        PROFILE,
        allow(),
        wide(),
        deny(),
        ComponentType::TlsMapBytesBytes,
    );
    let empty = || VLBytes::new(Vec::new());
    let payload = (0..10_000)
        .fold(TlsMapDelta::new().insert(empty(), empty()), |delta, _| {
            delta.update(empty(), empty())
        })
        .tls_serialize_detached()
        .unwrap();
    (registry, AppDataUpdateOperation::Update(payload.into()))
}

/// Assert that `work` decoded one registry entry and walked [`wide`] at
/// least once and at most once for each policy question.
fn assert_judged_once(work: ValidationWork) {
    assert_eq!(work.registry_decodes, 1, "{work:?}");
    assert!(
        (WIDE..=POLICY_QUESTIONS * WIDE).contains(&work.base_policy_evaluations),
        "{work:?}"
    );
}

/// One proposal updates one key 10,000 times. Its registry entry is decoded
/// once and each policy question is answered once, not once per mutation.
#[xmtp_common::test(unwrap_try = true)]
fn repeated_updates_reuse_registry_entry_and_verdicts() {
    let (registry, operation) = repeated_updates();
    let (validated, work) = count_validation_work(|| {
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
    });
    validated?;
    assert_judged_once(work);
}

/// The same proposal judged on receipt, before any commit, applies its
/// payload with the registry entry its policy check decoded.
#[xmtp_common::test(unwrap_try = true)]
fn standalone_proposal_reuses_registry_entry_and_verdicts() {
    let (registry, operation) = repeated_updates();
    let (validated, work) = count_validation_work(|| {
        validate_standalone_app_data_update(
            PROFILE,
            &operation,
            member(),
            "inbox_alice",
            &registry,
            None,
            None,
            None,
        )
    });
    validated?;
    assert_judged_once(work);
}

/// One proposal inserts 4,096 distinct keys and values. A verdict does not
/// depend on the element, so the policy is walked once.
#[xmtp_common::test(unwrap_try = true)]
fn distinct_elements_reuse_one_verdict() {
    let registry = registry_with(
        PROFILE,
        wide(),
        deny(),
        deny(),
        ComponentType::TlsMapBytesBytes,
    );
    let element = |i: u16| VLBytes::new(i.to_be_bytes().to_vec());
    let payload = (0..4_096)
        .fold(TlsMapDelta::new(), |delta, i| {
            delta.insert(element(i), element(i))
        })
        .tls_serialize_detached()?;
    let operation = AppDataUpdateOperation::Update(payload.into());
    let (validated, work) = count_validation_work(|| {
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
    });
    validated?;
    assert_judged_once(work);
}

/// A commit carries 1,000 proposals for one component. It decodes the
/// registry entry and walks the policy once, not once per proposal.
#[xmtp_common::test(unwrap_try = true)]
fn proposals_of_one_commit_share_registry_entry_and_verdicts() {
    let id = ComponentId::new(0xC300);
    let registry = registry_with(id, deny(), wide(), deny(), ComponentType::Bytes);
    let operation = AppDataUpdateOperation::Update(b"new".to_vec().into());
    let updates = (0..1_000).map(|_| AppDataUpdateInCommit {
        component_id: id,
        operation: &operation,
        actor: member(),
        proposer_inbox_id: "inbox_alice",
    });
    let (validated, work) = count_validation_work(|| {
        validate_app_data_update_sequence(
            updates,
            |_| Some(b"old".as_slice()),
            &registry,
            None,
            None,
        )
    });
    validated?;
    assert_judged_once(work);
}

/// A verdict reached for one operation is not reused for another: a delta
/// that inserts a key and then updates it is denied when only inserts are
/// allowed.
// verifies: PERM-011
#[xmtp_common::test(unwrap_try = true)]
fn reused_verdicts_keep_operations_apart() {
    let id = ComponentId::new(0xC300);
    let registry = registry_with(id, allow(), deny(), deny(), ComponentType::TlsMapBytesBytes);
    let key = || VLBytes::new(b"key".to_vec());
    let commit = |delta: TlsMapDelta<VLBytes, VLBytes>| {
        let operation = AppDataUpdateOperation::Update(delta.tls_serialize_detached()?.into());
        validate_app_data_update_sequence(
            [AppDataUpdateInCommit {
                component_id: id,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }],
            |_| None,
            &registry,
            None,
            None,
        )
    };
    commit(TlsMapDelta::new().insert(key(), key()))?;
    assert!(matches!(
        commit(TlsMapDelta::new().insert(key(), key()).update(key(), key())),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// A verdict reached for one component is not reused for another component
/// in the same commit.
// verifies: PERM-011
#[xmtp_common::test(unwrap_try = true)]
fn reused_verdicts_keep_components_apart() {
    let (open, closed) = (ComponentId::new(0xC200), ComponentId::new(0xC201));
    let mut registry = registry_with(open, deny(), allow(), deny(), ComponentType::Bytes);
    registry.set(
        closed,
        new_component_metadata(
            component_permissions()
                .insert(deny())
                .update(deny())
                .delete(deny())
                .call(),
            ComponentType::Bytes,
        ),
    )?;
    let operation = AppDataUpdateOperation::Update(b"new".to_vec().into());
    let commit = |ids: [ComponentId; 2]| {
        validate_app_data_update_sequence(
            ids.map(|component_id| AppDataUpdateInCommit {
                component_id,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }),
            |_| Some(b"old".as_slice()),
            &registry,
            None,
            None,
        )
    };
    commit([open, open])?;
    assert!(matches!(
        commit([open, closed]),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}

/// The inbox id of member `n`.
fn nth_inbox(n: usize) -> InboxId {
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&(n as u64).to_be_bytes());
    InboxId::from_bytes(bytes)
}

/// A commit of 5,000 one-mutation Updates to a 2,000-entry application map
/// decodes the map once, not once per proposal, and leaves the map that
/// applying the Updates in order gives.
#[xmtp_common::test(unwrap_try = true)]
fn sequence_decodes_each_component_once() {
    const ENTRIES: u16 = 2_000;
    const UPDATES: u16 = 5_000;
    let registry = registry_with(
        PROFILE,
        allow(),
        allow(),
        allow(),
        ComponentType::TlsMapBytesBytes,
    );
    let key = |n: u16| VLBytes::new((n % ENTRIES).to_be_bytes().to_vec());
    let mut expected = TlsMap::from_pairs((0..ENTRIES).map(|n| (key(n), VLBytes::new(vec![]))));
    let committed = expected.tls_serialize_detached()?;
    let operations: Vec<_> = (0..UPDATES)
        .map(|n| {
            let value = VLBytes::new(n.to_be_bytes().to_vec());
            expected.set(key(n), value.clone());
            let delta = TlsMapDelta::new().update(key(n), value);
            AppDataUpdateOperation::Update(delta.tls_serialize_detached().unwrap().into())
        })
        .collect();

    let (post, decodes) = count_snapshot_decodes(|| {
        validate_app_data_update_sequence(
            operations.iter().map(|operation| AppDataUpdateInCommit {
                component_id: PROFILE,
                operation,
                actor: member(),
                proposer_inbox_id: "inbox_alice",
            }),
            |id| (id == PROFILE).then_some(committed.as_slice()),
            &registry,
            None,
            None,
        )
    });
    assert!(decodes <= 1, "{decodes} snapshot decodes");
    let post = post?.remove(&PROFILE).flatten();
    assert_eq!(
        post.as_deref().map(Vec::as_slice),
        Some(expected.tls_serialize_detached()?.as_slice())
    );
}

/// Each of 5,000 members updates its own membership entry in one commit.
/// The 5,000-entry membership is decoded once, not once per proposal, and
/// ends as applying the Updates in order leaves it.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
fn membership_updates_decode_once() {
    const MEMBERS: usize = 5_000;
    let membership = ComponentId::GROUP_MEMBERSHIP;
    let registry = registry_with(
        membership,
        deny(),
        self_owned(),
        deny(),
        ComponentType::TlsMapInboxIdBytes,
    );
    let entry = |sequence: u8| VLBytes::new(vec![sequence; 8]);
    let mut expected = TlsMap::from_pairs((0..MEMBERS).map(|n| (nth_inbox(n), entry(1))));
    let committed = expected.tls_serialize_detached()?;
    let proposers: Vec<_> = (0..MEMBERS).map(|n| nth_inbox(n).to_hex()).collect();
    let operations: Vec<_> = (0..MEMBERS)
        .map(|n| {
            expected.set(nth_inbox(n), entry(2));
            profile_delta(TlsMapMutation::Update {
                key: nth_inbox(n),
                value: entry(2),
            })
        })
        .collect();

    let (post, decodes) = count_snapshot_decodes(|| {
        validate_app_data_update_sequence(
            operations
                .iter()
                .zip(&proposers)
                .map(|(operation, proposer)| AppDataUpdateInCommit {
                    component_id: membership,
                    operation,
                    actor: member(),
                    proposer_inbox_id: proposer,
                }),
            |id| (id == membership).then_some(committed.as_slice()),
            &registry,
            None,
            None,
        )
    });
    assert!(decodes <= 1, "{decodes} snapshot decodes");
    let post = post?.remove(&membership).flatten();
    assert_eq!(
        post.as_deref().map(Vec::as_slice),
        Some(expected.tls_serialize_detached()?.as_slice())
    );
}

/// Validation reads a built-in collection's invariant from the decoded
/// state only for `SUPER_ADMIN_LIST`; every other built-in collection has
/// none, so any pair of values passes. A new collection invariant fails
/// here until validation judges it too.
#[xmtp_common::test(unwrap_try = true)]
fn only_the_super_admin_list_has_a_collection_invariant() {
    use xmtp_mls_group_state::app_data::{
        component_registry::ComponentOp, registry_table::lookup_component,
        validation::ComponentChange,
    };

    let (malformed, empty) = ([0xFF; 3], [0x00]);
    let values: [Option<&[u8]>; 3] = [None, Some(&malformed), Some(&empty)];
    for id in [
        ComponentId::ADMIN_LIST,
        ComponentId::DM_MEMBERS,
        ComponentId::COMPONENT_REGISTRY,
        ComponentId::GROUP_MEMBERSHIP,
    ] {
        let component = lookup_component(id).unwrap();
        for (op, old_value, new_value) in [ComponentOp::Update, ComponentOp::Delete]
            .into_iter()
            .flat_map(|op| values.map(|old| (op, old)))
            .flat_map(|(op, old)| values.map(|new| (op, old, new)))
        {
            let change = ComponentChange::builder()
                .component_id(id)
                .op(op)
                .actor(member())
                .maybe_old_value(old_value)
                .maybe_new_value(new_value)
                .build();
            component.validate_invariant(&change, &ComponentRegistry::new())?;
        }
    }
}

/// A verdict reused across a commit's proposals keeps the self-or-non-member
/// answer apart: a member's own delete and a non-member's delete are
/// allowed, and a later delete of another member's entry is still denied.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
fn reused_verdicts_keep_self_and_non_member_deletes_apart() {
    let (alice, bob, gone) = (fake_inbox(1), fake_inbox(2), fake_inbox(3));
    let proposer = alice.to_hex();
    let registry = profile_registry();
    let committed = profile_snapshot(&[(alice, "Alice"), (bob, "Bob"), (gone, "Gone")]);
    let members = HashSet::from([alice, bob]);
    let delete = |key| profile_delta(TlsMapMutation::Delete { key });
    let (own, other, stale) = (delete(alice), delete(bob), delete(gone));
    let commit = |operations: &[&AppDataUpdateOperation]| {
        validate_app_data_update_sequence(
            operations.iter().map(|&operation| AppDataUpdateInCommit {
                component_id: PROFILE,
                operation,
                actor: member(),
                proposer_inbox_id: &proposer,
            }),
            |_| Some(committed.as_slice()),
            &registry,
            None,
            Some(&members),
        )
    };
    commit(&[&own, &stale])?;
    for allowed in [&own, &stale] {
        assert!(matches!(
            commit(&[allowed, &other]),
            Err(CommitRuleError::InsufficientPermissions)
        ));
    }
}

/// A verdict reached for an admin's or a super admin's proposal is not
/// reused for a later proposal by a lesser role on the same component in the
/// same commit.
// verifies: PERM-011
#[xmtp_common::test(unwrap_try = true)]
fn reused_verdicts_keep_actors_apart() {
    let id = ComponentId::new(0xC200);
    let operation = AppDataUpdateOperation::Update(b"new".to_vec().into());
    let super_admin_only = base_policy(MetadataBasePolicy::AllowIfSuperAdmin);
    for (policy, allowed, denied) in [
        (admin_only(), admin(), member()),
        (super_admin_only, super_admin(), admin()),
    ] {
        let registry = registry_with(id, deny(), policy, deny(), ComponentType::Bytes);
        let commit = |actors: [ActorAuthority; 2]| {
            validate_app_data_update_sequence(
                actors.map(|actor| AppDataUpdateInCommit {
                    component_id: id,
                    operation: &operation,
                    actor,
                    proposer_inbox_id: "inbox_alice",
                }),
                |_| Some(b"old".as_slice()),
                &registry,
                None,
                None,
            )
        };
        commit([allowed, allowed])?;
        assert!(matches!(
            commit([allowed, denied]),
            Err(CommitRuleError::InsufficientPermissions)
        ));
    }
}

/// A verdict reached for a DM participant, whom DM authority lets satisfy
/// `ALLOW_IF_SUPER_ADMIN` on an application component, is not reused for a
/// later proposal by an inbox outside the DM.
// verifies: PERM-028
#[xmtp_common::test(unwrap_try = true)]
fn reused_verdicts_keep_dm_authority_apart() {
    let (alice, bob, carol) = (fake_inbox(1), fake_inbox(2), fake_inbox(3));
    let super_admin_only = base_policy(MetadataBasePolicy::AllowIfSuperAdmin);
    let registry = registry_with(
        PROFILE,
        deny(),
        super_admin_only,
        deny(),
        ComponentType::Bytes,
    );
    let dm = dm_of(alice, bob);
    let operation = AppDataUpdateOperation::Update(b"new".to_vec().into());
    let commit = |proposers: [InboxId; 2]| {
        let proposers = proposers.map(|inbox| inbox.to_hex());
        validate_app_data_update_sequence(
            proposers.iter().map(|proposer| AppDataUpdateInCommit {
                component_id: PROFILE,
                operation: &operation,
                actor: member(),
                proposer_inbox_id: proposer,
            }),
            |_| Some(b"old".as_slice()),
            &registry,
            Some(&dm),
            None,
        )
    };
    commit([alice, bob])?;
    assert!(matches!(
        commit([alice, carol]),
        Err(CommitRuleError::InsufficientPermissions)
    ));
}
