//! AppDataUpdate-path helpers for the migrated-group sender intents.
//!
//! Each helper here corresponds to one `IntentKind` branch in
//! `mls_sync.rs::get_publish_intent_data`. The caller has already
//! confirmed `is_migrated_group(openmls_group)` is true; these
//! functions stage the inline `AppDataUpdate` commit and return the
//! resulting `PublishIntentData`.

use openmls::{group::MlsGroup as OpenMlsGroup, prelude::tls_codec::Serialize};
use openmls_traits::signatures::Signer;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        components::inbox_id_set::{AdminListComponent, SuperAdminListComponent},
        typed::Component,
    },
    inbox_id::InboxId,
    tls_set::{TlsSetDelta, TlsSetMutation},
};

use super::component_source::ComponentSourceError;
use super::stage_inline_app_data_commit;
use crate::{
    context::XmtpSharedContext,
    groups::{
        AdminListActionType, GroupError,
        intents::UpdateAdminListIntentData,
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
