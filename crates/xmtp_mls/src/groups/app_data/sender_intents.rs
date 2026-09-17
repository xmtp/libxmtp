//! AppDataUpdate-path helpers for the migrated-group sender intents.
//!
//! Each helper here corresponds to one `IntentKind` branch in
//! `mls_sync.rs::get_publish_intent_data`. The caller has already
//! confirmed `is_migrated_group(openmls_group)` is true; these
//! functions stage the inline `AppDataUpdate` commit and return the
//! resulting `PublishIntentData`.

use std::collections::BTreeMap;

use openmls::{group::MlsGroup as OpenMlsGroup, prelude::tls_codec::Serialize};
use openmls_traits::signatures::Signer;
use prost::Message;
use tls_codec::VLBytes;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentOp, ComponentRegistry},
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
    GroupActionPolicies, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::component_source::{ComponentSourceError, metadata_field_to_component_id};
use super::{
    pending_app_data_updates, stage_app_data_proposals_and_commit,
    stage_app_data_propose_and_commit,
};
use crate::groups::{
    AdminListActionType, GroupError,
    error::MetadataPermissionsError,
    group_permissions::{
        MembershipPolicies, MembershipPolicy, PermissionsPolicies, PermissionsPolicy,
    },
    intents::{
        AppDataUpdateIntentData, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdatePermissionIntentData,
    },
    mls_sync::{PublishIntentData, generate_prepared_commit},
};
use xmtp_db::XmtpMlsStorageProvider;

/// Stage the `AppDataUpdate` commit for an `UpdateAdminList` intent on
/// a migrated group. Maps the intent action onto a one-element
/// `TlsSetDelta` over `ADMIN_LIST` (Add/Remove) or `SUPER_ADMIN_LIST`
/// (AddSuper/RemoveSuper) and returns the staged commit's
/// `PublishIntentData`. The wire format always carries a delta, even
/// for single mutations.
pub(crate) fn apply_update_admin_list_app_data_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateAdminListIntentData,
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
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

    let ((proposal_msg, bundle), staged_commit, group_epoch) = generate_prepared_commit(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_app_data_propose_and_commit(
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
        payloads_to_publish: vec![
            proposal_msg.tls_serialize_detached()?,
            commit.tls_serialize_detached()?,
        ],
        staged_commit,
        post_commit_action: None,
        should_send_push_notification,
        group_epoch,
    })
}

/// Stage the `AppDataUpdate` commit for an `UpdatePermission` intent on
/// a migrated group. Action policies and their registry mirrors change
/// in the same commit. Metadata changes update both insert and update policies.
pub(crate) fn apply_update_permission_app_data_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdatePermissionIntentData,
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
    if matches!(
        intent_data.update_type,
        PermissionUpdateType::AddAdmin | PermissionUpdateType::RemoveAdmin
    ) && intent_data.policy_option == PermissionPolicyOption::Allow
    {
        return Err(MetadataPermissionsError::InvalidPermissionUpdate.into());
    }
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

    // The commit includes pending proposals from all members. Read their
    // accumulated state before replacing a whole registry entry, so an edit
    // to one policy field cannot undo a pending edit to another field.
    let pending: BTreeMap<_, _> = pending_app_data_updates(openmls_group)?
        .into_iter()
        .flatten()
        .collect();
    let read = |id: ComponentId| {
        match pending.get(&id.as_u16()) {
            Some(value) => value.as_deref(),
            None => openmls_group
                .extensions()
                .app_data_dictionary()
                .and_then(|extension| extension.dictionary().get(&id.as_u16())),
        }
        .ok_or_else(|| ComponentSourceError::MalformedComponentValue {
            component_id: id,
            reason: "component is missing from pending state".into(),
        })
    };
    let registry =
        ComponentRegistry::from_bytes(read(ComponentId::COMPONENT_REGISTRY)?).map_err(|error| {
            ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("registry decode: {error}"),
            }
        })?;
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
        ComponentOp::Update => {
            perms.insert_policy = Some(new_policy.clone());
            perms.update_policy = Some(new_policy);
        }
        ComponentOp::Delete => perms.delete_policy = Some(new_policy),
    }
    metadata.permissions = Some(perms);

    let new_metadata_bytes = metadata.encode_to_vec();
    let delta =
        TlsMapDelta::<ComponentId, VLBytes>::new().update(target, VLBytes::new(new_metadata_bytes));
    let payload = <ComponentRegistryComponent as Component>::encode_mutation(&delta)
        .map_err(|e| GroupError::ComponentSource(ComponentSourceError::from(e)))?;

    let mut updates = vec![(ComponentId::COMPONENT_REGISTRY, payload)];
    if intent_data.update_type != PermissionUpdateType::UpdateMetadata {
        let id = ComponentId::GROUP_ACTION_POLICIES;
        let bytes = read(id)?;
        let mut actions = GroupActionPolicies::decode(bytes).map_err(|error| {
            ComponentSourceError::MalformedComponentValue {
                component_id: id,
                reason: format!("action policies decode: {error}"),
            }
        })?;
        match intent_data.update_type {
            PermissionUpdateType::AddMember => {
                actions.add_member = Some(
                    MembershipPolicies::from(intent_data.policy_option)
                        .to_proto()
                        .map_err(|_| MetadataPermissionsError::InvalidPermissionUpdate)?,
                )
            }
            PermissionUpdateType::RemoveMember => {
                actions.remove_member = Some(
                    MembershipPolicies::from(intent_data.policy_option)
                        .to_proto()
                        .map_err(|_| MetadataPermissionsError::InvalidPermissionUpdate)?,
                )
            }
            PermissionUpdateType::AddAdmin => {
                actions.add_admin = Some(
                    PermissionsPolicies::from(intent_data.policy_option)
                        .to_proto()
                        .map_err(|_| MetadataPermissionsError::InvalidPermissionUpdate)?,
                )
            }
            PermissionUpdateType::RemoveAdmin => {
                actions.remove_admin = Some(
                    PermissionsPolicies::from(intent_data.policy_option)
                        .to_proto()
                        .map_err(|_| MetadataPermissionsError::InvalidPermissionUpdate)?,
                )
            }
            PermissionUpdateType::UpdateMetadata => unreachable!(),
        }
        updates.push((id, actions.encode_to_vec()));
    }

    let ((proposal_messages, bundle), staged_commit, group_epoch) = generate_prepared_commit(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_app_data_proposals_and_commit(
                group, provider, &signer, updates,
            )?)
        },
    )?;

    let (commit, welcome, _group_info) = bundle.into_messages();
    debug_assert!(
        welcome.is_none(),
        "UpdatePermission via AppDataUpdate must not produce a welcome"
    );
    let mut payloads_to_publish = proposal_messages
        .iter()
        .map(Serialize::tls_serialize_detached)
        .collect::<Result<Vec<_>, _>>()?;
    payloads_to_publish.push(commit.tls_serialize_detached()?);
    Ok(PublishIntentData {
        payloads_to_publish,
        staged_commit,
        post_commit_action: None,
        should_send_push_notification,
        group_epoch,
    })
}

/// Stage the `AppDataUpdate(component_id, payload)` commit for a
/// generic [`AppDataUpdateIntentData`]. Two op flavors:
///
/// - **`Replace(bytes)`** — the bytes ARE the new wire-form value of the
///   component. The handler emits them directly via
///   [`stage_app_data_propose_and_commit`]. Last-writer-wins; idempotent
///   under replay. Covers every Bytes / String typed component
///   (EXTERNAL_COMMIT_POLICY, GROUP_NAME, GROUP_IMAGE_URL, APP_DATA,
///   COMMIT_LOG_SIGNER, MESSAGE_DISAPPEAR_*, MIN_SUPPORTED_PROTOCOL_VERSION).
///
/// - **`DeltaWithBase { pre, post }`** — for collection-typed components
///   (TlsMap / TlsSet) where the on-wire AppDataUpdate proposal carries
///   a *delta* against prior state. To replay correctly under concurrent
///   same-key writes, the dispatcher reads current state at commit time
///   and emits only the residual mutations. The residual computation is
///   shape-specific and lives in a per-component table — landing it for
///   each of the three collection shapes (`TlsSet<InboxId>`,
///   `TlsMap<InboxId, bytes>`, `TlsMap<ComponentId, ComponentMetadata>`)
///   is intentionally deferred to a follow-on PR that also migrates the
///   existing typed `UpdateAdminList` / `UpdatePermission` /
///   `UpdateGroupMembership` intents onto this generic shape.
///
///   The DeltaWithBase arm in this PR returns
///   `GroupError::ProposalsNotSupported` with a clear error message —
///   callers using L-0 today (only EXTERNAL_COMMIT_POLICY) only need
///   the Replace flavor. Surfacing the explicit error keeps the
///   forward-compat intent shape in place without silently downgrading
///   collection writes to last-writer-wins (which would corrupt the
///   collections' replay semantics).
pub(crate) fn apply_app_data_update_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: AppDataUpdateIntentData,
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
    let component_id = ComponentId::new(intent_data.component_id);
    let payload = intent_data.payload;

    let ((proposal_msg, bundle), staged_commit, group_epoch) = generate_prepared_commit(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_app_data_propose_and_commit(
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
        "AppDataUpdate intent must not produce a welcome"
    );
    Ok(PublishIntentData {
        payloads_to_publish: vec![
            proposal_msg.tls_serialize_detached()?,
            commit.tls_serialize_detached()?,
        ],
        staged_commit,
        post_commit_action: None,
        should_send_push_notification,
        group_epoch,
    })
}
