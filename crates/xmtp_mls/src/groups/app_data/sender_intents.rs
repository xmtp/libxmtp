//! AppDataUpdate-path helpers for the migrated-group sender intents.
//!
//! Each helper here corresponds to one `IntentKind` branch in
//! `mls_sync.rs::get_publish_intent_data`. The caller has already
//! confirmed `is_migrated_group(openmls_group)` is true; these
//! functions stage the inline `AppDataUpdate` commit and return the
//! resulting `PublishIntentData`.

use openmls::{group::MlsGroup as OpenMlsGroup, prelude::tls_codec::Serialize};
use openmls_traits::signatures::Signer;
use prost::Message;
use tls_codec::VLBytes;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentOp,
        components::{
            inbox_id_set::{AdminListComponent, SuperAdminListComponent},
            tls_map_components::ComponentRegistryComponent,
        },
        typed::Component,
    },
    group_mutable_metadata::GroupMutableMetadataError,
    inbox_id::InboxId,
    tls_map::TlsMapDelta,
    tls_set::{TlsSetDelta, TlsSetMutation},
};
use xmtp_proto::xmtp::mls::message_contents::{
    MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::component_source::{ComponentSourceError, metadata_field_to_component_id};
use super::{load_component_registry, stage_inline_app_data_commit};
use crate::{
    context::XmtpSharedContext,
    groups::{
        AdminListActionType, GroupError,
        intents::{
            PermissionPolicyOption, PermissionUpdateType, UpdateAdminListIntentData,
            UpdatePermissionIntentData,
        },
        mls_sync::{PublishIntentData, generate_commit_with_rollback},
    },
};

/// Stage the `AppDataUpdate` commit for an `UpdateAdminList` intent on
/// a migrated group. Maps the intent action onto a one-element
/// `TlsSetDelta` over `ADMIN_LIST` (Add/Remove) or `SUPER_ADMIN_LIST`
/// (AddSuper/RemoveSuper) and returns the staged commit's
/// `PublishIntentData`. The wire format always carries a delta, even
/// for single mutations.
pub(crate) fn apply_update_admin_list_app_data_intent(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateAdminListIntentData,
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
    let storage = context.mls_storage();

    let inbox_id = InboxId::from_hex(&intent_data.inbox_id)
        .map_err(|e| GroupError::ComponentSource(e.into()))?;
    let (component_id, mutation) = match intent_data.action_type {
        AdminListActionType::Add => (ComponentId::ADMIN_LIST, TlsSetMutation::Insert(inbox_id)),
        AdminListActionType::Remove => (ComponentId::ADMIN_LIST, TlsSetMutation::Remove(inbox_id)),
        AdminListActionType::AddSuper => (
            ComponentId::SUPER_ADMIN_LIST,
            TlsSetMutation::Insert(inbox_id),
        ),
        AdminListActionType::RemoveSuper => (
            ComponentId::SUPER_ADMIN_LIST,
            TlsSetMutation::Remove(inbox_id),
        ),
    };

    let delta = TlsSetDelta::<InboxId> {
        mutations: vec![mutation],
    };
    let payload = match component_id {
        ComponentId::ADMIN_LIST => <AdminListComponent as Component>::encode_mutation(&delta),
        ComponentId::SUPER_ADMIN_LIST => {
            <SuperAdminListComponent as Component>::encode_mutation(&delta)
        }
        _ => unreachable!("admin-list intent maps to ADMIN_LIST or SUPER_ADMIN_LIST only"),
    }
    .map_err(|e| GroupError::ComponentSource(ComponentSourceError::from(e)))?;

    let (bundle, staged_commit, group_epoch) = generate_commit_with_rollback(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_inline_app_data_commit(
                group,
                provider,
                &signer,
                component_id,
                payload,
            )?)
        },
    )?;

    let (commit, welcome, _group_info) = bundle.into_messages();
    debug_assert!(
        welcome.is_none(),
        "UpdateAdminList via AppDataUpdate must not produce a welcome"
    );
    Ok(PublishIntentData {
        payloads_to_publish: vec![commit.tls_serialize_detached()?],
        staged_commit,
        post_commit_action: None,
        should_send_push_notification,
        group_epoch,
    })
}

/// Stage the `AppDataUpdate` commit for an `UpdatePermission` intent on
/// a migrated group. The commit only mutates the affected
/// `COMPONENT_REGISTRY` entry's policy field — custom-component
/// entries survive untouched. `PolicyOption::Allow` is permitted
/// here because the underlying `COMPONENT_REGISTRY` mutation is
/// hardcoded super-admin-only by the dispatch layer's permission
/// check.
pub(crate) fn apply_update_permission_app_data_intent(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdatePermissionIntentData,
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
    let storage = context.mls_storage();

    let base = match intent_data.policy_option {
        PermissionPolicyOption::Allow => MetadataBasePolicy::Allow,
        PermissionPolicyOption::Deny => MetadataBasePolicy::Deny,
        PermissionPolicyOption::AdminOnly => MetadataBasePolicy::AllowIfAdmin,
        PermissionPolicyOption::SuperAdminOnly => MetadataBasePolicy::AllowIfSuperAdmin,
    };
    let new_policy = MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(base as i32)),
    };

    // Map (update_type, metadata_field_name) onto (target_component,
    // which policy field to mutate).
    let (target, op) = match intent_data.update_type {
        PermissionUpdateType::AddMember => (ComponentId::GROUP_MEMBERSHIP, ComponentOp::Insert),
        PermissionUpdateType::RemoveMember => (ComponentId::GROUP_MEMBERSHIP, ComponentOp::Delete),
        PermissionUpdateType::AddAdmin => (ComponentId::ADMIN_LIST, ComponentOp::Insert),
        PermissionUpdateType::RemoveAdmin => (ComponentId::ADMIN_LIST, ComponentOp::Delete),
        PermissionUpdateType::UpdateMetadata => {
            let field_name = intent_data.metadata_field_name.as_deref().ok_or_else(|| {
                GroupError::MetadataPermissionsError(
                    GroupMutableMetadataError::MissingMetadataField.into(),
                )
            })?;
            let component_id = metadata_field_to_component_id(field_name).ok_or_else(|| {
                GroupError::ComponentSource(ComponentSourceError::UnknownMetadataField(
                    field_name.to_owned(),
                ))
            })?;
            (component_id, ComponentOp::Update)
        }
    };

    let registry = load_component_registry(openmls_group)?;
    let mut metadata = registry
        .get(&target)
        .map_err(|e| {
            GroupError::ComponentSource(ComponentSourceError::MalformedComponentValue {
                component_id: target,
                reason: format!("registry get failed: {e}"),
            })
        })?
        .ok_or_else(|| {
            GroupError::ComponentSource(ComponentSourceError::MalformedComponentValue {
                component_id: target,
                reason: "registry has no entry for target component".into(),
            })
        })?;
    let mut perms = metadata.permissions.clone().ok_or_else(|| {
        GroupError::ComponentSource(ComponentSourceError::MalformedComponentValue {
            component_id: target,
            reason: "registry entry missing permissions".into(),
        })
    })?;
    match op {
        ComponentOp::Insert => perms.insert_policy = Some(new_policy),
        ComponentOp::Update => perms.update_policy = Some(new_policy),
        ComponentOp::Delete => perms.delete_policy = Some(new_policy),
    }
    metadata.permissions = Some(perms);

    let new_metadata_bytes = metadata.encode_to_vec();
    let delta =
        TlsMapDelta::<ComponentId, VLBytes>::new().update(target, VLBytes::new(new_metadata_bytes));
    let payload = <ComponentRegistryComponent as Component>::encode_mutation(&delta)
        .map_err(|e| GroupError::ComponentSource(ComponentSourceError::from(e)))?;

    let (bundle, staged_commit, group_epoch) = generate_commit_with_rollback(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_inline_app_data_commit(
                group,
                provider,
                &signer,
                ComponentId::COMPONENT_REGISTRY,
                payload,
            )?)
        },
    )?;

    let (commit, welcome, _group_info) = bundle.into_messages();
    debug_assert!(
        welcome.is_none(),
        "UpdatePermission via AppDataUpdate must not produce a welcome"
    );
    Ok(PublishIntentData {
        payloads_to_publish: vec![commit.tls_serialize_detached()?],
        staged_commit,
        post_commit_action: None,
        should_send_push_notification,
        group_epoch,
    })
}
