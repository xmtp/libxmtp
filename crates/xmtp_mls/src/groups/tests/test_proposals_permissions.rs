//! Admin and permission enforcement for proposals.

use crate::{
    context::XmtpSharedContext,
    groups::{
        EnableProposalsOptions,
        intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData},
    },
    tester,
};
use xmtp_db::{group_intent::IntentKind, prelude::*};

use super::test_proposals::assert_insufficient_permissions;

/// Test that proposals from non-admins are rejected when received in admin-only groups.
/// Pattern: Alix (admin) creates admin-only group, adds Bo (non-admin), Bo proposes to add Caro,
/// When Alix syncs, Bo's proposal should be rejected because Bo doesn't have permission.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_admin_proposal_rejected_in_admin_only_group() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates an admin-only group (only admins can add members)
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;

    // Alix adds Bo as a regular member (not admin)
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo receives the welcome
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Verify Bo is not an admin
    let bo_members = bo_group.members().await?;
    let bo_member = bo_members
        .iter()
        .find(|m| m.inbox_id == bo.inbox_id())
        .expect("Bo should be in the group");
    assert!(
        matches!(
            bo_member.permission_level,
            crate::groups::members::PermissionLevel::Member
        ),
        "Bo should be a regular member, not an admin"
    );

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Bo (non-admin) attempts to propose adding Caro
    // This proposal should be created locally but rejected when Alix receives it
    let bo_db = bo_group.context.db();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        bo_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;

    // Bo's published proposal is rejected by the same validation policy.
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs - the proposal should be rejected during validation
    // We sync and check that Alix doesn't have the proposal in their pending proposals
    let sync_result = alix_group.sync().await;

    // The sync might error because the proposal validation failed
    // Either way, Alix should not have pending proposals from Bo
    if let Err(e) = &sync_result {
        tracing::info!("Sync returned error as expected: {:?}", e);
    }

    // Check that Alix doesn't have any pending proposals (Bo's was rejected)
    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;

    assert_eq!(
        alix_pending, 0,
        "Alix should have no pending proposals (Bo's was rejected)"
    );

    tracing::info!(
        "Non-admin proposal correctly rejected. Alix pending proposals: {}",
        alix_pending
    );
}

/// Test that proposals from admins are accepted in admin-only groups.
/// Pattern: Alix (admin) creates admin-only group, proposes to add Caro,
/// Bo receives the proposal without error (validation passes for admin proposals).
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_proposal_accepted_in_admin_only_group() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates an admin-only group
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;

    // Alix adds Bo (so there's someone to receive the proposal)
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Alix (admin) proposes to add Caro
    let alix_db = alix_group.context.db();
    let propose_intent =
        alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            alix_group.group_id,
            ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![])
                .try_into()?,
            false,
        ))?;
    alix_group
        .sync_until_intent_resolved(propose_intent.id)
        .await?;

    // Bo syncs to receive the proposal - should succeed (Alix is admin)
    // This verifies that the proposal validation passes for admin proposals
    let sync_result = bo_group.sync().await;
    assert!(
        sync_result.is_ok(),
        "Bo should successfully receive Alix's proposal (admin proposal accepted): {:?}",
        sync_result.err()
    );

    tracing::info!("Admin proposal correctly accepted and validated.");
}

// =============================================================================
// Committer vs Proposer Permission Tests
// =============================================================================

/// Test that a non-admin can commit admin-proposed adds in an admin-only group,
/// and that the admin can then perform admin-only metadata updates.
///
/// This exercises the committer vs proposer distinction in permission evaluation:
/// - Add proposals are validated against the **proposer** (admin) not the committer (non-admin)
/// - Metadata changes (group name) are validated against the **committer** (actor)
///
/// It also verifies that `extract_committer_and_proposers` correctly identifies the committer
/// from the path update leaf node when multiple proposals are pending.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_admin_commits_admin_proposals_in_admin_group() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    tester!(alix);
    tester!(bo);
    tester!(caro);
    tester!(dave);
    tester!(eve);

    // Alix creates an admin-only group (only admins can add/remove members)
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;

    // Alix adds Bo and Caro as regular members
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.sync_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify Bo is not an admin
    let members = bo_group.members().await?;
    let bo_member = members
        .iter()
        .find(|m| m.inbox_id == bo.inbox_id())
        .expect("Bo should be in group");
    assert!(
        matches!(
            bo_member.permission_level,
            crate::groups::members::PermissionLevel::Member
        ),
        "Bo should be a regular member"
    );

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Alix (admin) proposes adding Dave
    let alix_db = alix_group.context.db();
    let propose_dave = alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        alix_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![dave.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    alix_group
        .sync_until_intent_resolved(propose_dave.id)
        .await?;

    // Alix (admin) proposes adding Eve
    let propose_eve = alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        alix_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![eve.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    alix_group
        .sync_until_intent_resolved(propose_eve.id)
        .await?;

    // Bo syncs to receive both proposals (should pass validation since Alix is admin)
    bo_group.sync().await?;

    // Verify Bo has pending proposals
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
    // This tests that add permissions are checked against the proposer (Alix, admin),
    // not the committer (Bo, non-admin)
    let bo_db = bo_group.context.db();
    let commit_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::CommitPendingProposals,
        bo_group.group_id,
        CommitPendingProposalsIntentData::default().into(),
        false,
    ))?;
    bo_group
        .sync_until_intent_resolved(commit_intent.id)
        .await?;

    // Sync everyone
    alix_group.sync().await?;
    caro_group.sync().await?;

    // Dave and Eve should receive welcomes
    let dave_groups = dave.sync_welcomes().await?;
    let eve_groups = eve.sync_welcomes().await?;
    assert!(
        !dave_groups.is_empty(),
        "Dave should have received a welcome"
    );
    assert!(!eve_groups.is_empty(), "Eve should have received a welcome");

    // Verify all members see the full group
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    assert_eq!(
        alix_members.len(),
        5,
        "Alix should see 5 members (alix, bo, caro, dave, eve)"
    );
    assert_eq!(
        bo_members.len(),
        5,
        "Bo should see 5 members (alix, bo, caro, dave, eve)"
    );

    // Now test admin-only metadata operation: Alix updates the group name
    // This exercises the commit.actor path for metadata permission checks
    alix_group
        .update_group_name("New Admin Group Name".to_string())
        .await?;

    // Bo syncs to receive the metadata update
    bo_group.sync().await?;
    let bo_group_name = bo_group.group_name()?;
    assert_eq!(
        bo_group_name, "New Admin Group Name",
        "Bo should see the updated group name"
    );
}

/// Test that multiple non-admin proposers + admin committer works correctly.
/// This is the inverse scenario: multiple non-admins propose (in a default-permissions group),
/// and the admin commits. Verifies that:
/// 1. extract_committer_and_proposers correctly identifies the admin as committer
/// 2. Each add is validated against its proposer, not the committer
/// 3. The admin can then perform admin-only operations (group name update)
#[xmtp_common::test(unwrap_try = true)]
async fn test_multiple_non_admin_proposers_with_admin_committer() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    tester!(dave);
    tester!(eve);

    // Alix creates a default-permissions group (anyone can add members)
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.sync_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    let initial_members = alix_group.members().await?;
    assert_eq!(initial_members.len(), 3);

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Bo (non-admin) proposes adding Dave
    let bo_db = bo_group.context.db();
    let bo_propose = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        bo_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![dave.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    bo_group.sync_until_intent_resolved(bo_propose.id).await?;

    // Caro (non-admin) proposes adding Eve
    let caro_db = caro_group.context.db();
    let caro_propose = caro_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        caro_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![eve.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    caro_group
        .sync_until_intent_resolved(caro_propose.id)
        .await?;

    // Alix syncs to receive both proposals
    alix_group.sync().await?;

    // Verify Alix has pending proposals from Bo and Caro
    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert!(
        alix_pending >= 2,
        "Alix should have at least 2 pending proposals (from Bo and Caro)"
    );

    // Alix (admin) commits all pending proposals
    // extract_committer_and_proposers should identify:
    //   committer = Alix (from path update leaf node)
    //   proposers = [Bo, Caro] (from proposal senders)
    let alix_db = alix_group.context.db();
    let commit_intent = alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::CommitPendingProposals,
        alix_group.group_id,
        CommitPendingProposalsIntentData::default().into(),
        false,
    ))?;
    alix_group
        .sync_until_intent_resolved(commit_intent.id)
        .await?;

    // Sync everyone
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Dave and Eve should receive welcomes
    let dave_groups = dave.sync_welcomes().await?;
    let eve_groups = eve.sync_welcomes().await?;
    assert!(
        !dave_groups.is_empty(),
        "Dave should have received a welcome"
    );
    assert!(!eve_groups.is_empty(), "Eve should have received a welcome");

    // Verify all 5 members
    let members = alix_group.members().await?;
    assert_eq!(
        members.len(),
        5,
        "Should have 5 members after committing proposals from multiple proposers"
    );

    // Now Alix (admin) updates the group name - admin-only metadata operation
    // This verifies that the committer (actor) is correctly used for metadata checks
    alix_group
        .update_group_name("Updated by Admin".to_string())
        .await?;

    bo_group.sync().await?;
    let bo_name = bo_group.group_name()?;
    assert_eq!(
        bo_name, "Updated by Admin",
        "Group name should be updated by admin"
    );

    caro_group.sync().await?;
    let caro_name = caro_group.group_name()?;
    assert_eq!(
        caro_name, "Updated by Admin",
        "Group name should be updated for all members"
    );
}

// =============================================================================
// Proposal Validation Rejection Tests (validate_proposal paths)
// =============================================================================

/// Test that remove proposals are rejected in admin-only groups when the proposer lacks permission.
/// Scenario A: Non-admin proposes removing a regular member → rejected.
/// Scenario B: Non-admin proposes removing the super admin → rejected.
#[xmtp_common::test(unwrap_try = true)]
async fn test_remove_proposal_validation_in_admin_group() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates an admin-only group and adds Bo and Caro
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.sync_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
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
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(remove_alix_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (cannot remove super admin)
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

/// Test that an admin can propose removing a member and a non-admin can commit it.
/// This is the remove counterpart to test_non_admin_commits_admin_proposals_in_admin_group.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_proposes_remove_committed_by_non_admin() {
    use crate::groups::group_permissions::PreconfiguredPolicies;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates an admin-only group and adds Bo and Caro
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.sync_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
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

/// Test that GCE proposals modifying metadata are rejected when the proposer lacks permission.
/// Scenario A: Non-admin proposes changing group name → rejected.
/// Scenario B: Propose removing the mutable metadata extension entirely → rejected.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_admin_gce_metadata_proposal_rejected() {
    use crate::groups::{
        build_extensions_for_metadata_update, group_permissions::PreconfiguredPolicies,
        intents::ProposeGroupContextExtensionsIntentData,
    };
    use openmls::prelude::tls_codec::Serialize;
    use xmtp_mls_common::group_mutable_metadata::MetadataField;

    tester!(alix);
    tester!(bo);

    // Alix creates an admin-only group and adds Bo
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // This test exercises legacy GCE-proposal validation. We don't
    // call `enable_proposals()` here because that fires the AppData-
    // migration bootstrap, which strips MUTABLE_METADATA from the
    // extension set — `build_extensions_for_metadata_update` (used
    // below) reads that extension as its starting point and would
    // surface `Mutable(MissingExtension)` against a migrated group.
    // The legacy GCE-validation path applies to unmigrated groups,
    // which is the only state this test needs to cover.

    // Scenario A: Bo (non-admin) proposes changing the group name via GCE
    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let extensions = build_extensions_for_metadata_update(
                &mls_group,
                MetadataField::GroupName.to_string(),
                "hacked".to_string(),
            )?;
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let bo_db = bo_group.context.db();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (Bo is not admin, can't change metadata)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Non-admin metadata change proposal should be rejected"
    );

    // Scenario B: Bo proposes removing the mutable metadata extension entirely
    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let mut extensions = mls_group.extensions().clone();
            extensions.remove(openmls::extensions::ExtensionType::Unknown(
                xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID,
            ));
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (cannot remove mutable metadata extension)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Removing mutable metadata extension should be rejected"
    );

    // Verify group name is unchanged
    let name = alix_group.group_name()?;
    assert_ne!(name, "hacked", "Group name should not have changed");
}

/// Test that GCE proposals modifying admin lists are rejected when the proposer lacks permission.
/// Scenario A: Non-admin proposes adding an admin → rejected.
/// Scenario B: Non-super-admin proposes modifying super admin list → rejected.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_admin_gce_admin_list_proposal_rejected() {
    use crate::groups::{
        build_extensions_for_admin_lists_update,
        group_permissions::PreconfiguredPolicies,
        intents::{
            AdminListActionType, ProposeGroupContextExtensionsIntentData, UpdateAdminListIntentData,
        },
    };
    use openmls::prelude::tls_codec::Serialize;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates an admin-only group and adds Bo and Caro
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Legacy-validation test: see comment in
    // `test_non_admin_gce_metadata_proposal_rejected` for why we don't
    // fire `enable_proposals()` here (the AppData-migration bootstrap
    // would strip the legacy MUTABLE_METADATA extension this test's
    // helpers depend on).

    // Scenario A: Bo proposes adding Caro as admin via GCE
    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let extensions = build_extensions_for_admin_lists_update(
                &mls_group,
                UpdateAdminListIntentData::new(
                    AdminListActionType::Add,
                    caro.inbox_id().to_string(),
                ),
            )?;
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let bo_db = bo_group.context.db();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (Bo is not super admin, can't add admins)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Non-super-admin adding admin proposal should be rejected"
    );

    // Scenario B: Bo proposes adding himself to the super admin list via GCE
    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let extensions = build_extensions_for_admin_lists_update(
                &mls_group,
                UpdateAdminListIntentData::new(
                    AdminListActionType::AddSuper,
                    bo.inbox_id().to_string(),
                ),
            )?;
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (only super admins can modify super admin list)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Super admin list modification by non-super-admin should be rejected"
    );

    // Scenario C: Bo proposes removing Caro from the admin list via GCE
    // First, Alix (super admin) promotes Caro to admin so there's someone to remove
    alix_group
        .update_admin_list(
            crate::groups::UpdateAdminListType::Add,
            caro.inbox_id().to_string(),
        )
        .await?;
    bo_group.sync().await?;

    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let extensions = build_extensions_for_admin_lists_update(
                &mls_group,
                UpdateAdminListIntentData::new(
                    AdminListActionType::Remove,
                    caro.inbox_id().to_string(),
                ),
            )?;
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (Bo is not super admin, can't remove admins)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Non-super-admin removing admin proposal should be rejected"
    );
}

/// Test that GCE proposals changing permissions are rejected when the proposer is not a super admin.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_super_admin_gce_permission_change_rejected() {
    use crate::groups::{
        build_extensions_for_permissions_update,
        group_permissions::PreconfiguredPolicies,
        intents::{
            PermissionPolicyOption, PermissionUpdateType, ProposeGroupContextExtensionsIntentData,
            UpdatePermissionIntentData,
        },
    };
    use openmls::prelude::tls_codec::Serialize;

    tester!(alix);
    tester!(bo);

    // Alix creates an admin-only group and adds Bo
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let alix_group = alix.create_group(policy_set, None)?;
    alix_group.sync().await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Legacy-validation test: see comment in
    // `test_non_admin_gce_metadata_proposal_rejected` for why we don't
    // fire `enable_proposals()` here (the AppData-migration bootstrap
    // would strip the legacy GROUP_PERMISSIONS extension this test's
    // helpers depend on).

    // Bo (non-super-admin) proposes changing AddMember policy to Allow via GCE
    let extensions_bytes = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let extensions = build_extensions_for_permissions_update(
                &mls_group,
                UpdatePermissionIntentData::new(
                    PermissionUpdateType::AddMember,
                    PermissionPolicyOption::Allow,
                    None,
                ),
            )?;
            Ok::<Vec<u8>, crate::groups::GroupError>(extensions.tls_serialize_detached()?)
        })
        .await?;

    let intent_data = ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
    let intent_bytes: Vec<u8> = intent_data.into();
    let bo_db = bo_group.context.db();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeGroupContextExtensions,
        bo_group.group_id,
        intent_bytes,
        false,
    ))?;
    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(propose_intent.id)
            .await
            .unwrap_err(),
    );

    // Alix syncs — proposal rejected (only super admins can change permissions)
    let _ = alix_group.sync().await;

    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        alix_pending, 0,
        "Permission change by non-super-admin should be rejected"
    );
}

/// App-data action policies are hardcoded super-admin-only components. A member
/// must not be able to replace the stored declaration, even if its bytes are a
/// valid `GroupActionPolicies` value.
#[xmtp_common::test(unwrap_try = true)]
async fn test_non_super_admin_app_data_action_policies_change_rejected() {
    use crate::groups::{
        app_data::component_source::ComponentSourceError,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use xmtp_mls_common::app_data::component_id::ComponentId;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Re-use the current valid value. This isolates the authorization check
    // from protobuf decoding and value validation.
    let payload = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            mls_group
                .extensions()
                .app_data_dictionary()
                .and_then(|dictionary| {
                    dictionary
                        .dictionary()
                        .get(&ComponentId::GROUP_ACTION_POLICIES.as_u16())
                })
                .map(|bytes| bytes.to_vec())
                .ok_or_else(|| {
                    crate::groups::GroupError::ComponentSource(
                        ComponentSourceError::MalformedComponentValue {
                            component_id: ComponentId::GROUP_ACTION_POLICIES,
                            reason: "action policies are missing after bootstrap".into(),
                        },
                    )
                })
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::GROUP_ACTION_POLICIES.as_u16(),
            payload,
        )))
        .queue(bo_group)?;

    assert_insufficient_permissions(
        bo_group
            .sync_until_intent_resolved(intent.id)
            .await
            .unwrap_err(),
    );

    // The receiver also discards the proposal. It must not remain pending.
    let _ = alix_group.sync().await;
    let pending = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<usize, crate::groups::GroupError>(mls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        pending, 0,
        "non-super-admin action-policy proposal was retained"
    );
}

/// A super admin can queue a raw action-policy replacement that bypasses the
/// sender's paired registry update. The receiver must reject the commit so the
/// declaration cannot differ from the policy that enforces admin-list writes.
#[xmtp_common::test(unwrap_try = true)]
async fn test_receiver_rejects_raw_action_policy_registry_divergence() {
    use crate::groups::intents::{
        AppDataUpdateIntentData, PermissionPolicyOption, PermissionUpdateType, QueueIntent,
    };
    use prost::Message as _;
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_proto::xmtp::mls::message_contents::{
        PermissionsUpdatePolicy,
        permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.first()?.clone();
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    let before_permissions = bo_group.permissions()?;
    let before_epoch = bo_group.epoch().await?;
    let before_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after bootstrap")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    let payload = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let bytes = mls_group
                .extensions()
                .app_data_dictionary()
                .and_then(|dictionary| {
                    dictionary
                        .dictionary()
                        .get(&ComponentId::GROUP_ACTION_POLICIES.as_u16())
                })
                .expect("action policies are present after bootstrap");
            let mut actions =
                xmtp_proto::xmtp::mls::message_contents::GroupActionPolicies::decode(bytes)?;
            actions.add_admin = Some(PermissionsUpdatePolicy {
                kind: Some(PermissionsPolicyKind::Base(
                    PermissionsBasePolicy::Deny as i32,
                )),
            });
            Ok::<_, crate::groups::GroupError>(actions.encode_to_vec())
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::GROUP_ACTION_POLICIES.as_u16(),
            payload,
        )))
        .queue(&alix_group)?;

    assert!(
        alix_group
            .sync_until_intent_resolved(intent.id)
            .await
            .is_err(),
        "a raw action-policy write that diverges from ADMIN_LIST must be rejected"
    );

    // A safe rejection can return a successful summary. The terminal record
    // proves that the receiver advanced past the rejected commit.
    let _ = bo_group.sync().await;
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let rejection = bo
        .context
        .db()
        .read_last_rejection(&topic)?
        .expect("receiver did not reject divergent action policies");
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        rejection.sequence_id
    );
    assert_eq!(bo_group.epoch().await?, before_epoch);
    assert_eq!(bo_group.permissions()?, before_permissions);
    let after_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after rejection")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    assert_eq!(after_dictionary, before_dictionary);

    // A later valid paired update must still commit. This proves the safe
    // rejection did not leave the group at a pending commit head.
    alix_group
        .update_permission_policy(
            PermissionUpdateType::AddAdmin,
            PermissionPolicyOption::SuperAdminOnly,
            None,
        )
        .await?;
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, before_epoch + 1);
}

/// A raw registry mutation must not relax the constrained `ADMIN_LIST` entry.
/// The shared constrained-component check rejects it before publication.
#[xmtp_common::test(unwrap_try = true)]
async fn test_raw_registry_admin_list_allow_rejected_before_publish() {
    use crate::groups::{
        app_data::load_component_registry,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
            typed::Component,
        },
        tls_map::TlsMapDelta,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.first()?.clone();
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    let before_permissions = bo_group.permissions()?;
    let before_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after bootstrap")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    let payload = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let registry = load_component_registry(&mls_group)?;
            let mut metadata = registry
                .get(&ComponentId::ADMIN_LIST)?
                .expect("ADMIN_LIST is registered after bootstrap");
            metadata
                .permissions
                .as_mut()
                .expect("ADMIN_LIST has permissions")
                .insert_policy = Some(MetadataPolicy {
                kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
            });
            let delta = TlsMapDelta::new().update(
                ComponentId::ADMIN_LIST,
                VLBytes::new(metadata.encode_to_vec()),
            );
            <ComponentRegistryComponent as Component>::encode_mutation(&delta)
                .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            payload,
        )))
        .queue(&alix_group)?;

    assert!(
        alix_group
            .sync_until_intent_resolved(intent.id)
            .await
            .is_err(),
        "a raw ADMIN_LIST Allow policy must be rejected"
    );
    bo_group.sync().await?;
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    // The constrained-component check rejects this payload before publication.
    assert!(bo.context.db().read_last_rejection(&topic)?.is_none());
    assert_eq!(bo_group.permissions()?, before_permissions);
    let after_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after rejection")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    assert_eq!(after_dictionary, before_dictionary);
}

/// `AllowIfAdmin` is structurally valid for `ADMIN_LIST`, but it still cannot
/// diverge from the super-admin-only `add_admin` declaration in 0x800C.
#[xmtp_common::test(unwrap_try = true)]
async fn test_receiver_rejects_raw_registry_action_policy_divergence() {
    use crate::groups::{
        app_data::load_component_registry,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
            typed::Component,
        },
        tls_map::TlsMapDelta,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.first()?.clone();
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    let before_permissions = bo_group.permissions()?;
    let before_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after bootstrap")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    let payload = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let registry = load_component_registry(&mls_group)?;
            let mut metadata = registry
                .get(&ComponentId::ADMIN_LIST)?
                .expect("ADMIN_LIST is registered after bootstrap");
            metadata
                .permissions
                .as_mut()
                .expect("ADMIN_LIST has permissions")
                .insert_policy = Some(MetadataPolicy {
                kind: Some(MetadataPolicyKind::Base(
                    MetadataBasePolicy::AllowIfAdmin as i32,
                )),
            });
            let delta = TlsMapDelta::new().update(
                ComponentId::ADMIN_LIST,
                VLBytes::new(metadata.encode_to_vec()),
            );
            <ComponentRegistryComponent as Component>::encode_mutation(&delta)
                .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            payload,
        )))
        .queue(&alix_group)?;

    assert!(
        alix_group
            .sync_until_intent_resolved(intent.id)
            .await
            .is_err(),
        "a raw ADMIN_LIST policy that disagrees with 0x800C must be rejected"
    );
    let _ = bo_group.sync().await;
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    assert!(
        bo.context.db().read_last_rejection(&topic)?.is_some(),
        "receiver did not terminally reject an ADMIN_LIST/0x800C mismatch"
    );
    assert_eq!(bo_group.permissions()?, before_permissions);
    let after_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after rejection")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    assert_eq!(after_dictionary, before_dictionary);
}

/// Each removal below is valid against the committed two-super-admin state,
/// but the pair must be rejected when one commit would leave no super admin.
/// This exercises receiver validation over the proposal batch, rather than the
/// sequential sender API path.
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_removing_all_super_admins_is_rejected() {
    use crate::{
        groups::{
            app_data::stage_app_data_proposals_and_commit, mls_sync::generate_prepared_commit,
        },
        state_tx::state_write,
    };
    use openmls::prelude::tls_codec::Serialize;
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId, components::inbox_id_set::SuperAdminListComponent,
            typed::Component,
        },
        inbox_id::InboxId,
        tls_set::TlsSetDelta,
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
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

    let remove = |inbox_id: &str| -> Result<Vec<u8>, crate::groups::GroupError> {
        let inbox_id = InboxId::from_hex(inbox_id)
            .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))?;
        let delta = TlsSetDelta::new().remove(inbox_id);
        <SuperAdminListComponent as Component>::encode_mutation(&delta)
            .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
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
                        group, provider, signer, updates,
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

/// A permissions update changes the declared action policy and its registry
/// enforcement mirror in one commit. Check all four action slots on both peers.
#[xmtp_common::test(unwrap_try = true)]
async fn test_migrated_action_permission_updates_mirror_registry() {
    use crate::groups::{
        group_permissions::{MembershipPolicies, PermissionsPolicies},
        intents::{PermissionPolicyOption, PermissionUpdateType},
    };
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
        Kind as MetadataPolicyKind, MetadataBasePolicy,
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    let mut expected = alix_group.permissions()?.policies;
    let updates = [
        (
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Deny,
        ),
        (
            PermissionUpdateType::RemoveMember,
            PermissionPolicyOption::Deny,
        ),
        (
            PermissionUpdateType::AddAdmin,
            PermissionPolicyOption::AdminOnly,
        ),
        (
            PermissionUpdateType::RemoveAdmin,
            PermissionPolicyOption::Deny,
        ),
    ];

    for (update_type, policy_option) in updates {
        let epoch = alix_group.epoch().await?;
        alix_group
            .update_permission_policy(update_type.clone(), policy_option.clone(), None)
            .await?;
        bo_group.sync().await?;
        assert_eq!(
            alix_group.epoch().await?,
            epoch + 1,
            "{update_type:?} did not commit both mirrored updates in one commit"
        );
        assert_eq!(
            bo_group.epoch().await?,
            epoch + 1,
            "receiver did not apply the mirrored update commit"
        );

        match &update_type {
            PermissionUpdateType::AddMember => {
                expected.add_member_policy = MembershipPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::RemoveMember => {
                expected.remove_member_policy = MembershipPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::AddAdmin => {
                expected.add_admin_policy = PermissionsPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::RemoveAdmin => {
                expected.remove_admin_policy = PermissionsPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::UpdateMetadata => unreachable!(),
        }

        for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
            assert_eq!(
                group.permissions()?.policies,
                expected,
                "{label} did not reconstruct the action policy declaration"
            );

            let (component_id, policy) = match &update_type {
                PermissionUpdateType::AddMember => (ComponentId::GROUP_MEMBERSHIP, "insert"),
                PermissionUpdateType::RemoveMember => (ComponentId::GROUP_MEMBERSHIP, "delete"),
                PermissionUpdateType::AddAdmin => (ComponentId::ADMIN_LIST, "insert"),
                PermissionUpdateType::RemoveAdmin => (ComponentId::ADMIN_LIST, "delete"),
                PermissionUpdateType::UpdateMetadata => unreachable!(),
            };
            let expected_base = match &policy_option {
                PermissionPolicyOption::Allow => MetadataBasePolicy::Allow,
                PermissionPolicyOption::Deny => MetadataBasePolicy::Deny,
                PermissionPolicyOption::AdminOnly => MetadataBasePolicy::AllowIfAdmin,
                PermissionPolicyOption::SuperAdminOnly => MetadataBasePolicy::AllowIfSuperAdmin,
            } as i32;
            let registry = group
                .load_mls_group_with_lock_async(async |mls_group| {
                    Ok::<_, crate::groups::GroupError>(
                        crate::groups::app_data::load_component_registry(&mls_group)?,
                    )
                })
                .await?;
            let metadata = registry
                .get(&component_id)?
                .unwrap_or_else(|| panic!("{label} registry missing {component_id:?}"));
            let permissions = metadata
                .permissions
                .as_ref()
                .unwrap_or_else(|| panic!("{label} registry entry missing permissions"));
            let mirrored_policy = match policy {
                "insert" => permissions.insert_policy.as_ref(),
                "delete" => permissions.delete_policy.as_ref(),
                _ => unreachable!(),
            }
            .and_then(|policy| policy.kind.as_ref())
            .unwrap_or_else(|| panic!("{label} registry entry missing {policy} policy"));
            assert!(
                matches!(mirrored_policy, MetadataPolicyKind::Base(base) if *base == expected_base),
                "{label} registry {policy} policy did not mirror the action policy: {mirrored_policy:?}"
            );
        }
    }
}

/// `Allow` would give every member authority to change the admin list. Reject
/// it before queueing an intent or advancing the group epoch.
#[xmtp_common::test(unwrap_try = true)]
async fn test_allow_admin_permission_updates_rejected_before_queueing() {
    use crate::groups::{
        GroupError,
        error::MetadataPermissionsError,
        intents::{PermissionPolicyOption, PermissionUpdateType},
    };

    tester!(alix);

    let group = alix.create_group(None, None)?;
    let epoch = group.epoch().await?;
    let intents = group.context.db().find_group_intents(
        group.group_id,
        None,
        Some(vec![IntentKind::UpdatePermission]),
    )?;

    for update_type in [
        PermissionUpdateType::AddAdmin,
        PermissionUpdateType::RemoveAdmin,
    ] {
        let error = group
            .update_permission_policy(update_type.clone(), PermissionPolicyOption::Allow, None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            GroupError::MetadataPermissionsError(MetadataPermissionsError::InvalidPermissionUpdate)
        ));
        assert_eq!(
            group.epoch().await?,
            epoch,
            "rejected update advanced epoch"
        );
        assert_eq!(
            group
                .context
                .db()
                .find_group_intents(
                    group.group_id,
                    None,
                    Some(vec![IntentKind::UpdatePermission]),
                )?
                .len(),
            intents.len(),
            "rejected {update_type:?} update queued an intent"
        );
    }
}

#[rstest::rstest]
#[case::default(
    crate::groups::group_permissions::PreconfiguredPolicies::Default,
    false,
    crate::groups::intents::PermissionPolicyOption::Allow
)]
#[case::admins_only(
    crate::groups::group_permissions::PreconfiguredPolicies::AdminsOnly,
    false,
    crate::groups::intents::PermissionPolicyOption::AdminOnly
)]
#[case::custom_admins(
    crate::groups::group_permissions::PreconfiguredPolicies::Default,
    true,
    crate::groups::intents::PermissionPolicyOption::Allow
)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_permissions_presets(
    #[case] preset: crate::groups::group_permissions::PreconfiguredPolicies,
    #[case] custom_admins: bool,
    #[case] add_member_policy: crate::groups::intents::PermissionPolicyOption,
) {
    use crate::groups::{group_permissions::PreconfiguredPolicies, intents::PermissionUpdateType};

    tester!(alix);
    let mut expected = preset.to_policy_set();
    if custom_admins {
        use crate::groups::group_permissions::PermissionsPolicies;
        expected.add_admin_policy = PermissionsPolicies::allow_if_actor_admin();
        expected.remove_admin_policy = PermissionsPolicies::allow_if_actor_admin();
    }
    let group = alix.create_group(Some(expected.clone()), None).unwrap();
    assert_eq!(group.permissions().unwrap().policies, expected);
    group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await
        .unwrap();
    group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            crate::groups::intents::PermissionPolicyOption::Deny,
            None,
        )
        .await
        .unwrap();
    assert!(
        PreconfiguredPolicies::from_policy_set(&group.permissions().unwrap().policies).is_err()
    );
    group
        .update_permission_policy(PermissionUpdateType::AddMember, add_member_policy, None)
        .await
        .unwrap();
    let actual = group.permissions().unwrap().policies;
    assert_eq!(actual, expected);
    if custom_admins {
        assert!(PreconfiguredPolicies::from_policy_set(&actual).is_err());
    } else {
        assert_eq!(
            PreconfiguredPolicies::from_policy_set(&actual).unwrap(),
            preset
        );
    }
}
