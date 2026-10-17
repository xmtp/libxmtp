use super::*;
use crate::groups::intents::{
    AdminListActionType, PermissionPolicyOption, PermissionUpdateType,
    ReaddInstallationsIntentData, UpdateAdminListIntentData, UpdateGroupMembershipIntentData,
    UpdatePermissionIntentData,
};

#[xmtp_common::test(unwrap_try = true)]
async fn test_no_gce_on_any_intent() {
    for kind in [
        IntentKind::MetadataUpdate,
        IntentKind::UpdateAdminList,
        IntentKind::UpdatePermission,
        IntentKind::UpdateGroupMembership,
        IntentKind::ReaddInstallations,
        IntentKind::KeyUpdate,
    ] {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&group.group_id)?;
        bo_group.receive().await?;

        let (mut queue, data): (_, Vec<u8>) = match kind {
            IntentKind::MetadataUpdate => (
                QueueIntent::metadata_update(),
                UpdateMetadataIntentData::new_update_group_name("dictionary group".into()).into(),
            ),
            IntentKind::UpdateAdminList => (
                QueueIntent::update_admin_list(),
                UpdateAdminListIntentData::new(AdminListActionType::Add, bo.inbox_id().into())
                    .into(),
            ),
            IntentKind::UpdatePermission => (
                QueueIntent::update_permission(),
                UpdatePermissionIntentData::new(
                    PermissionUpdateType::AddMember,
                    PermissionPolicyOption::AdminOnly,
                    None,
                )
                .into(),
            ),
            IntentKind::UpdateGroupMembership => (
                QueueIntent::update_group_membership(),
                UpdateGroupMembershipIntentData::new(
                    Default::default(),
                    vec![bo.inbox_id().into()],
                    vec![],
                )
                .into(),
            ),
            IntentKind::ReaddInstallations => (
                QueueIntent::readd_installations(),
                ReaddInstallationsIntentData::new(vec![bo.context.installation_id().to_vec()])
                    .into(),
            ),
            IntentKind::KeyUpdate => (QueueIntent::key_update(), vec![]),
            _ => unreachable!("each case names a creation-native intent"),
        };
        let intent = queue.data(data).queue(&group)?;
        let (_, attempt) = prepare_kind(&group, kind).await?;
        for proposal in &attempt.proposals {
            let queued = attempt.proposal_for_payload(&proposal.payload_hash)??;
            assert!(!matches!(
                queued.proposal(),
                Proposal::GroupContextExtensions(_)
            ));
        }
        let stored = Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &intent.id)??;
        let staged =
            crate::groups::mls_sync::decode_staged_commit(stored.staged_commit.as_deref()?)?;
        assert!(
            staged
                .queued_proposals()
                .all(|queued| !matches!(queued.proposal(), Proposal::GroupContextExtensions(_)))
        );
        group.publish_intents().await?;
        group.sync_until_intent_resolved(intent.id).await?;
    }
}
