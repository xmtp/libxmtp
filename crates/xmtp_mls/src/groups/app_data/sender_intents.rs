//! AppDataUpdate-path helpers for the migrated-group sender intents.
//!
//! Each helper here corresponds to one `IntentKind` branch in
//! `mls_sync.rs::get_publish_intent_data`. The caller has already
//! loaded the dictionary-native group; these functions propose the
//! intent's `AppDataUpdate`s, stage one commit of them, and return the
//! resulting `PublishIntentData`.

use openmls::{
    group::MlsGroup as OpenMlsGroup, messages::proposals::AppDataUpdateOperation,
    prelude::tls_codec::Serialize,
};
use openmls_traits::signatures::Signer;
use prost::Message;
use std::collections::{HashMap, HashSet};

use openmls::extensions::AppDataDictionary;
use tls_codec::{Deserialize, VLBytes};
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentOp, ComponentRegistry},
        components::{
            inbox_id_set::{AdminListComponent, SuperAdminListComponent},
            tls_map_components::ComponentRegistryComponent,
        },
        fields::{FieldError, FieldSnapshot, FieldWrite},
        typed::Component,
    },
    group_mutable_metadata::GroupMutableMetadataError,
    inbox_id::InboxId,
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::{TlsSetDelta, TlsSetMutation},
};
use xmtp_mls_validation::commit::{
    AppDataUpdateInCommit, extract_commit_participant, read_committed_metadata,
    validate_app_data_update_sequence,
};
use xmtp_proto::xmtp::mls::message_contents::{
    MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::{load_component_registry, pending_dictionary, stage_app_data_proposals_and_commit};
use crate::groups::{
    AdminListActionType, GroupError,
    error::MetadataPermissionsError,
    intents::{
        AppDataUpdateIntentData, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdatePermissionIntentData,
    },
    mls_sync::{
        PublishIntentData, generate_prepared_commit,
        update_group_membership::welcome_post_commit_action,
    },
};
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_mls_common::app_data::component_source::{
    ComponentSourceError, metadata_field_to_component_id,
};

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
    catalogue: &[ApplicationComponentDefinition],
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

    stage_updates(
        storage,
        openmls_group,
        vec![(component_id, AppDataUpdateOperation::Update(payload.into()))],
        catalogue,
        signer,
        should_send_push_notification,
    )
}

/// Stage the `AppDataUpdate` commit for an `UpdatePermission` intent on
/// a migrated group. Action policies and their registry mirrors change
/// in the same commit. Metadata changes update both insert and update policies.
pub(crate) fn apply_update_permission_app_data_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdatePermissionIntentData,
    catalogue: &[ApplicationComponentDefinition],
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
    let pending = pending_dictionary(openmls_group)?;
    let read = |id: ComponentId| {
        pending
            .get(&id.as_u16())
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

    stage_updates(
        storage,
        openmls_group,
        vec![(
            ComponentId::COMPONENT_REGISTRY,
            AppDataUpdateOperation::Update(payload.into()),
        )],
        catalogue,
        signer,
        should_send_push_notification,
    )
}

/// Stage the commit of an [`AppDataUpdateIntentData`], or `None` when
/// its field writes change nothing.
///
/// A payload intent is proposed verbatim. Field writes are resolved
/// against the committed registry, which is the authority for their
/// types and policies, and against the dictionary the commit
/// starts from, which decides whether an own-key write is an insert or an
/// update. A field that is gone or changed type since the write was
/// encoded fails the intent.
// implements: META-071, META-073
pub(crate) fn apply_app_data_update_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: AppDataUpdateIntentData,
    own: InboxId,
    catalogue: &[ApplicationComponentDefinition],
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<Option<PublishIntentData>, GroupError> {
    let updates = match intent_data {
        AppDataUpdateIntentData::Payload {
            component_id,
            payload,
        } => vec![(
            ComponentId::new(component_id),
            AppDataUpdateOperation::Update(payload.into()),
        )],
        AppDataUpdateIntentData::Fields(writes) => {
            let Some(updates) = field_writes_commit(openmls_group, own, &writes)? else {
                return Ok(None);
            };
            updates
        }
    };
    stage_updates(
        storage,
        openmls_group,
        updates,
        catalogue,
        signer,
        should_send_push_notification,
    )
    .map(Some)
}

/// The `AppDataUpdate` operations of `writes` by `own` in a commit built
/// now. Field types and policies come from the committed registry; current
/// values and membership from the committed dictionary with pending
/// proposals applied. A write the policies deny is refused here, as every
/// receiver would refuse it, so it is never published.
pub(crate) fn resolve_field_writes(
    openmls_group: &OpenMlsGroup,
    own: InboxId,
    writes: &[FieldWrite],
) -> Result<Vec<(ComponentId, AppDataUpdateOperation)>, GroupError> {
    let committed = openmls_group
        .extensions()
        .app_data_dictionary()
        .map(|extension| extension.dictionary());
    let values = pending_dictionary(openmls_group)?;
    let updates = FieldSnapshot::new(committed, &[])?.resolve_writes(Some(&values), own, writes)?;
    authorize_updates(openmls_group, &values, &updates)?;
    Ok(updates)
}

/// The operations of the commit that carries out `writes` by `own`, or
/// `None` when no commit is needed. The operations are empty when pending
/// proposals already carry out the writes but have changed a component
/// they name: the value is then pending, not committed, and the commit of
/// the pending proposals commits it.
pub(crate) fn field_writes_commit(
    openmls_group: &OpenMlsGroup,
    own: InboxId,
    writes: &[FieldWrite],
) -> Result<Option<Vec<(ComponentId, AppDataUpdateOperation)>>, GroupError> {
    let updates = resolve_field_writes(openmls_group, own, writes)?;
    if !updates.is_empty() {
        return Ok(Some(updates));
    }
    let committed = openmls_group
        .extensions()
        .app_data_dictionary()
        .map(|extension| extension.dictionary());
    let pending = pending_dictionary(openmls_group)?;
    let uncommitted = writes.iter().any(|write| {
        let id = write.component_id.as_u16();
        committed.and_then(|values| values.get(&id)) != pending.get(&id)
    });
    Ok(uncommitted.then_some(updates))
}

/// Check `updates` by this client, in order, against the committed
/// registry's policies as a receiver would, starting from `values`.
///
/// The `GROUP_MEMBERSHIP` of `values` is the commit's post-commit
/// membership: no field is `GROUP_MEMBERSHIP`, and membership upkeep only
/// cleans up after removals already pending, never changing membership.
// implements: META-071, META-073
fn authorize_updates(
    openmls_group: &OpenMlsGroup,
    values: &AppDataDictionary,
    updates: &[(ComponentId, AppDataUpdateOperation)],
) -> Result<(), GroupError> {
    let registry = load_component_registry(openmls_group)?;
    let (immutable, mutable) = read_committed_metadata(openmls_group)?;
    let own = extract_commit_participant(
        &openmls_group.own_leaf_index(),
        openmls_group,
        &immutable,
        &mutable,
    )?;
    let value = |id: ComponentId| values.get(&id.as_u16()).map(<[u8]>::to_vec);
    let members = value(ComponentId::GROUP_MEMBERSHIP)
        .and_then(|bytes| TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes).ok())
        .map(|map| map.keys().copied().collect::<HashSet<_>>());
    let mut states = HashMap::new();
    for (id, operation) in updates {
        let update = AppDataUpdateInCommit {
            component_id: *id,
            operation,
            actor: (&own).into(),
            proposer_inbox_id: &own.inbox_id,
        };
        let post = validate_app_data_update_sequence(
            [update],
            |id| states.get(&id).cloned().unwrap_or_else(|| value(id)),
            &registry,
            immutable.dm_members.as_ref(),
            members.as_ref(),
        )
        .map_err(|_| FieldError::Denied(*id))?;
        states.extend(post);
    }
    Ok(())
}

/// Propose `updates` and stage one commit of them, publishing the
/// proposals before the commit.
fn stage_updates(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    updates: Vec<(ComponentId, AppDataUpdateOperation)>,
    catalogue: &[ApplicationComponentDefinition],
    signer: impl Signer,
    should_send_push_notification: bool,
) -> Result<PublishIntentData, GroupError> {
    let ((proposal_messages, bundle), staged_commit, group_epoch) = generate_prepared_commit(
        storage,
        openmls_group,
        move |group, provider| -> Result<_, GroupError> {
            Ok(stage_app_data_proposals_and_commit(
                group, provider, &signer, catalogue, updates,
            )?)
        },
    )?;

    let (commit, welcome, _group_info) = bundle.into_messages();
    let post_commit_action = welcome_post_commit_action(welcome, staged_commit.as_deref())?;
    let mut payloads_to_publish = proposal_messages
        .iter()
        .map(Serialize::tls_serialize_detached)
        .collect::<Result<Vec<_>, _>>()?;
    payloads_to_publish.push(commit.tls_serialize_detached()?);
    Ok(PublishIntentData {
        payloads_to_publish,
        staged_commit,
        post_commit_action,
        should_send_push_notification,
        group_epoch,
    })
}
