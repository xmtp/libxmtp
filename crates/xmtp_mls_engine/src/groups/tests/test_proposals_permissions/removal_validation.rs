//! Proposal permission tests: removal validation.

use super::*;

// =============================================================================
// Proposal Validation Rejection Tests (validate_proposal paths)
// =============================================================================

/// Test that remove proposals are rejected in admin-only groups when the proposer lacks permission.
/// Scenario A: Non-admin proposes removing a regular member → rejected.
/// Scenario B: Non-admin proposes removing the super admin → rejected.
#[xmtp_common::test(unwrap_try = true)]
async fn test_remove_proposal_validation_in_admin_group() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // Alix creates an admin-only group and adds Bo and Caro
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.wait_for_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Enable proposals
    bo_group.sync().await?;

    // Scenario A: Bo (non-admin) proposes removing Caro → should be rejected by Alix
    let bo_db = bo_group.context.db();
    let remove_caro_intent =
        bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            bo_group.group_id,
            ProposeMemberUpdateIntentData::new(vec![], vec![caro.inbox_id().to_string()])
                .try_into()?,
            false,
        ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(remove_caro_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (Bo is not admin)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Non-admin remove proposal should be rejected"
    );

    // Scenario B: Bo (non-admin) proposes removing Alix (super admin) → should be rejected
    let remove_alix_intent =
        bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            bo_group.group_id,
            ProposeMemberUpdateIntentData::new(vec![], vec![alix.inbox_id().to_string()])
                .try_into()?,
            false,
        ))?;
    // Bo rejects this before publishing; see
    // test_admin_removing_super_admin_publishes_nothing.
    bo_group
        .sync_until_intent_resolved(remove_alix_intent.id)
        .await
        .map(|_| panic!("a super admin must not be removed"))
        .unwrap_or_else(assert_insufficient_permissions_before_publish);

    // Alix syncs — nothing to store (cannot remove super admin)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Remove super admin proposal should be rejected"
    );

    // Verify group membership is unchanged (all 3 members still present)
    alix_group.sync().await?;
    let members = alix_group.members().await?;
    assert_eq!(members.len(), 3, "All members should still be in the group");
}

/// An admin may remove members but not a super admin. Receivers once
/// rejected the Remove proposal but stored the paired membership proposal,
/// so every later send failed to commit pending proposals. Receivers now
/// reject both, and the sender publishes neither.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_removing_super_admin_publishes_nothing() {
    use crate::groups::UpdateAdminListType;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    let caro_group = caro.wait_for_welcomes().await?.first()?.clone();
    bo_group.sync().await?;
    caro_group.sync().await?;

    bo_group
        .remove_members(&[alix.inbox_id()])
        .await
        .map(|_| panic!("an admin must not remove a super admin"))
        .unwrap_or_else(assert_insufficient_permissions_before_publish);

    for group in [&caro_group, &alix_group, &bo_group] {
        group.sync().await?;
        group.send_message(b"after", Default::default()).await?;
    }
    assert_eq!(caro_group.members().await?.len(), 3);
    // Caro saw nothing to reject: Bo published neither proposal.
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(caro_group.group_id);
    assert!(caro.context.db().read_last_rejection(&topic)?.is_none());
}

/// A modified client can skip the send-side check and publish a standalone
/// membership delete of the super admin. Every receiver must reject it, or
/// it stays pending and fails each later commit of pending proposals.
// verifies: GMOD-019
#[xmtp_common::test(unwrap_try = true)]
async fn test_receivers_reject_raw_membership_delete_of_super_admin() {
    use crate::groups::{GroupError, UpdateAdminListType};
    use openmls::{messages::proposals::AppDataUpdateOperation, prelude::tls_codec::Serialize};
    use tls_codec::VLBytes;
    use xmtp_db::{TransactionOutcome::Continue, XmtpOpenMlsProviderRef};
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_mls_group_state::{
        app_data::{
            component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
            typed::Component,
        },
        inbox_id::InboxId,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    let caro_group = caro.wait_for_welcomes().await?.first()?.clone();
    bo_group.sync().await?;
    caro_group.sync().await?;

    let delta = TlsMapDelta::<InboxId, VLBytes>::new().delete(
        InboxId::from_hex(alix.inbox_id())
            .map_err(|error| GroupError::ComponentSource(error.into()))?,
    );
    let payload = <GroupMembershipComponent as Component>::encode_mutation(&delta)
        .map_err(|error| GroupError::ComponentSource(error.into()))?;
    let message = state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &bo.context.identity().installation_keys,
                    ComponentId::GROUP_MEMBERSHIP.as_u16(),
                    AppDataUpdateOperation::Update(payload.clone().into()),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(message.as_slice(), false)])?;
    bo.context.api().send_group_messages(messages).await?;

    for group in [&alix_group, &caro_group] {
        group.sync().await?;
        assert_eq!(
            group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0,
            "a receiver must not keep the delete"
        );
        group.send_message(b"after", Default::default()).await?;
    }
}

/// A modified client can publish an empty `DM_MEMBERS` delta in a group,
/// standalone or inline in its own commit. Every receiver must reject it:
/// a `DM_MEMBERS` value outside a DM fails every metadata read, and
/// nothing can remove it.
// verifies: META-030, PERM-011
#[xmtp_common::test(unwrap_try = true)]
async fn test_receivers_reject_empty_dm_members_proposal() {
    use crate::groups::{
        GroupError, app_data::accumulate_app_data_updates, mls_ext::commit_without_group_info,
        mls_sync::generate_prepared_commit,
    };
    use openmls::{
        messages::proposals::{AppDataUpdateOperation, AppDataUpdateProposal},
        prelude::{Proposal, tls_codec::Serialize},
    };
    use openmls_traits::OpenMlsProvider as _;
    use xmtp_db::{TransactionOutcome::Continue, XmtpOpenMlsProviderRef};
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_mls_group_state::{app_data::component_id::ComponentId, inbox_id::InboxId};
    use xmtp_mls_tls_collections::tls_set::TlsSetDelta;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    let caro_group = caro.wait_for_welcomes().await?.first()?.clone();
    caro_group.sync().await?;

    let id = ComponentId::DM_MEMBERS;
    let empty = TlsSetDelta::<InboxId>::new().tls_serialize_detached()?;
    let operation = || AppDataUpdateOperation::Update(empty.clone().into());
    for inline in [false, true] {
        // Build at the epoch the receivers' last commit left.
        bo_group.sync().await?;
        let message = state_write(bo.context.mls_storage(), |tx| {
            tx.with_group(bo_group.group_id, |mls, storage| {
                let signer = &bo.context.identity().installation_keys;
                if !inline {
                    let (message, _) = mls
                        .propose_app_data_update(
                            &XmtpOpenMlsProviderRef::new(storage),
                            signer,
                            id.as_u16(),
                            operation(),
                        )
                        .map_err(GroupError::Proposal)?;
                    return Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?));
                }
                let (message, _, _) = generate_prepared_commit(storage, mls, |mls, provider| {
                    let updates =
                        accumulate_app_data_updates(mls, [(id.as_u16(), &operation())]).unwrap();
                    let proposal = Proposal::AppDataUpdate(Box::new(
                        AppDataUpdateProposal::update(id.as_u16(), empty.clone()),
                    ));
                    let mut stage = commit_without_group_info(mls, provider.storage(), |builder| {
                        builder.consume_proposal_store(false).add_proposal(proposal)
                    })
                    .unwrap();
                    stage.with_app_data_dictionary_updates(updates);
                    let bundle = stage
                        .build(provider.rand(), provider.crypto(), signer, |_| true)
                        .unwrap()
                        .stage_commit(provider)
                        .unwrap();
                    bundle.into_messages().0.tls_serialize_detached()
                })?;
                Ok(Continue(message))
            })
        })?
        .into_continued();
        let messages = bo_group.prepare_group_messages(vec![(message.as_slice(), false)])?;
        bo.context.api().send_group_messages(messages).await?;

        for group in [&alix_group, &caro_group] {
            group.sync().await?;
            assert_eq!(
                group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
                0,
                "inline: {inline}: a receiver must not keep the proposal"
            );
            group.metadata().await?;
            group.send_message(b"after", Default::default()).await?;
            group.update_group_name(format!("renamed {inline}")).await?;
        }
    }
}

/// Test that an admin can propose removing a member and a non-admin can commit it.
/// This is the remove counterpart to test_non_admin_commits_admin_proposals_in_admin_group.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_proposes_remove_committed_by_non_admin() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // Alix creates an admin-only group and adds Bo and Caro
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.wait_for_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Enable proposals
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Alix (admin) proposes removing Caro
    let alix_db = alix_group.context.db();
    let remove_intent = alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        alix_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![], vec![caro.inbox_id().to_string()]).try_into()?,
        false,
    ))?;
    alix_group
        .sync_until_intent_resolved(remove_intent.id)
        .await?;

    // Bo syncs and receives the proposal (passes validation — Alix is admin)
    bo_group.sync().await?;

    let bo_pending = bo_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert!(
        bo_pending > 0,
        "Bo should have pending proposals from admin Alix"
    );

    // Bo (non-admin) commits the pending proposals
    let bo_db = bo_group.context.db();
    let commit_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::CommitPendingProposals,
        bo_group.group_id,
        CommitPendingProposalsIntentData::new().into(),
        false,
    ))?;
    bo_group
        .sync_until_intent_resolved(commit_intent.id)
        .await?;

    // Everyone syncs
    alix_group.sync().await?;
    caro_group.sync().await?;

    // Verify Caro was removed
    let alix_members = alix_group.members().await?;
    assert_eq!(
        alix_members.len(),
        2,
        "Group should have 2 members after removing Caro"
    );
    assert!(
        alix_members.iter().all(|m| m.inbox_id != caro.inbox_id()),
        "Caro should not be in the group"
    );

    // Verify Caro's group is inactive
    assert!(
        !caro_group.is_active()?,
        "Caro's group should be inactive after removal"
    );
}

/// Each removal below is valid against the committed two-super-admin state,
/// but the pair must be rejected when one commit would leave no super admin.
/// This exercises receiver validation over the proposal batch, rather than the
/// sequential sender API path.
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_removing_all_super_admins_is_rejected() {
    use crate::groups::{
        app_data::stage_app_data_proposals_and_commit, mls_sync::generate_prepared_commit,
    };
    use openmls::{messages::proposals::AppDataUpdateOperation, prelude::tls_codec::Serialize};
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_mls_group_state::{
        app_data::{
            component_id::ComponentId, components::inbox_id_set::SuperAdminListComponent,
            typed::Component,
        },
        inbox_id::InboxId,
    };
    use xmtp_mls_tls_collections::tls_set::TlsSetDelta;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;
    bo_group.sync().await?;

    // Establish two committed super admins. Each raw removal below is then
    // individually valid against the pre-commit dictionary.
    alix_group
        .update_admin_list(
            crate::groups::UpdateAdminListType::AddSuper,
            bo.inbox_id().to_string(),
        )
        .await?;
    bo_group.sync().await?;
    assert_eq!(alix_group.super_admin_list()?.len(), 2);
    assert_eq!(bo_group.super_admin_list()?.len(), 2);

    let remove = |inbox_id: &str| -> Result<AppDataUpdateOperation, crate::groups::GroupError> {
        let inbox_id = InboxId::from_hex(inbox_id)
            .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))?;
        let delta = TlsSetDelta::new().remove(inbox_id);
        let payload = <SuperAdminListComponent as Component>::encode_mutation(&delta)
            .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))?;
        Ok(AppDataUpdateOperation::Update(payload.into()))
    };
    let updates = vec![
        (ComponentId::SUPER_ADMIN_LIST, remove(alix.inbox_id())?),
        (ComponentId::SUPER_ADMIN_LIST, remove(bo.inbox_id())?),
    ];
    let storage = alix.context.mls_storage();
    let signer = &alix.context.identity().installation_keys;
    let payloads = state_write(storage, |tx| {
        tx.with_group(alix_group.group_id, |mls_group, storage| {
            let ((proposals, bundle), _staged_commit, _epoch) = generate_prepared_commit(
                storage,
                mls_group,
                |group, provider| -> Result<_, crate::groups::GroupError> {
                    Ok(stage_app_data_proposals_and_commit(
                        group,
                        provider,
                        signer,
                        &[],
                        &xmtp_configuration::LimitsConfiguration::default(),
                        updates,
                    )?)
                },
            )?;
            let (commit, welcome, _) = bundle.into_messages();
            assert!(welcome.is_none());
            let mut payloads = proposals
                .iter()
                .map(Serialize::tls_serialize_detached)
                .collect::<Result<Vec<_>, _>>()?;
            payloads.push(commit.tls_serialize_detached()?);
            Ok::<_, crate::groups::GroupError>(Continue(payloads))
        })
    })?
    .into_continued();
    let messages = alix_group.prepare_group_messages(
        payloads
            .iter()
            .map(|payload| (payload.as_slice(), false))
            .collect(),
    )?;
    alix.context.api().send_group_messages(messages).await?;

    // The commit is terminally rejected by Bo. Its pre-commit state remains
    // usable and retains both super admins.
    let _ = bo_group.sync().await;
    let super_admins = bo_group.super_admin_list()?;
    assert_eq!(super_admins.len(), 2);
    assert!(super_admins.contains(&alix.inbox_id().to_string()));
    assert!(super_admins.contains(&bo.inbox_id().to_string()));

    // A valid removal still succeeds after the rejected commit. This also
    // proves that the receiver did not leave its commit head pending.
    bo_group
        .update_admin_list(
            crate::groups::UpdateAdminListType::RemoveSuper,
            alix.inbox_id().to_string(),
        )
        .await?;
    assert_eq!(
        bo_group.super_admin_list()?,
        vec![bo.inbox_id().to_string()]
    );
}

/// An empty `DM_MEMBERS` delta in Alix's proposal store, as an older client
/// that accepted it would hold, is dropped by Alix's next commit, so every
/// member applies the commit and can still read the metadata. Alix is a
/// super admin; either the `DM_MEMBERS` rule or the empty-delta rule keeps
/// the delta out.
// verifies: META-030, PERM-011
#[xmtp_common::test(unwrap_try = true)]
async fn builder_drops_a_stored_empty_dm_members_proposal() {
    use crate::groups::GroupError;
    use openmls::{messages::proposals::AppDataUpdateOperation, prelude::tls_codec::Serialize};
    use xmtp_db::{TransactionOutcome::Continue, XmtpOpenMlsProviderRef};
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_mls_group_state::{app_data::component_id::ComponentId, inbox_id::InboxId};
    use xmtp_mls_tls_collections::tls_set::TlsSetDelta;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    let caro_group = caro.wait_for_welcomes().await?.first()?.clone();

    let empty = TlsSetDelta::<InboxId>::new().tls_serialize_detached()?;
    state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(alix_group.group_id, |mls, storage| {
            mls.propose_app_data_update(
                &XmtpOpenMlsProviderRef::new(storage),
                &alix.context.identity().installation_keys,
                ComponentId::DM_MEMBERS.as_u16(),
                AppDataUpdateOperation::Update(empty.into()),
            )
            .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(()))
        })
    })?;
    assert_eq!(
        alix_group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        1
    );

    alix_group.update_group_name("renamed".into()).await?;
    let epoch = alix_group.epoch().await?;
    alix_group.metadata().await?;
    for group in [&bo_group, &caro_group] {
        group.sync().await?;
        assert_eq!(group.epoch().await?, epoch);
        assert_eq!(group.group_name()?, "renamed");
        group.metadata().await?;
    }
}
