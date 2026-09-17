//! Batched proposals, sequence ids, and app-data dictionary migration.

use crate::{
    context::XmtpSharedContext,
    groups::{
        EnableProposalsOptions,
        intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData, QueueIntent},
        send_message_opts::SendMessageOpts,
    },
    tester,
};
use xmtp_db::{group_intent::IntentKind, prelude::*};
use xmtp_mls_common::app_data::{
    component_id::ComponentId,
    components::metadata_attributes::{
        AppDataComponent, GroupDescriptionComponent, GroupImageUrlComponent, GroupNameComponent,
        MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
        MAX_GROUP_NAME_LENGTH,
    },
};

// =============================================================================
// Batched Proposal Tests
// =============================================================================

/// Test that add_members uses the batched proposal path when proposals are enabled.
/// When proposals_enabled is true, UpdateGroupMembership should create Add proposals + GCE + commit
/// in a single publish, rather than a direct commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_add_members_batched_when_proposals_enabled() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Create group with alix + bo
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Enable proposals on the group
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Verify proposals are enabled
    let proposals_enabled = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(proposals_enabled, "Proposals should be enabled");

    // Add caro via add_members — this should use the batched proposal path
    alix_group.add_members(&[caro.inbox_id()]).await?;

    // Bo syncs to see the proposals and commit
    bo_group.sync().await?;

    // Caro should receive a welcome
    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(
        caro_groups.len(),
        1,
        "Caro should receive exactly one welcome"
    );

    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify all members see 3 members
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    let caro_members = caro_group.members().await?;
    assert_eq!(alix_members.len(), 3, "Alix should see 3 members");
    assert_eq!(bo_members.len(), 3, "Bo should see 3 members");
    assert_eq!(caro_members.len(), 3, "Caro should see 3 members");

    // Verify no pending proposals remain
    let pending = bo_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        pending, 0,
        "Should have no pending proposals after batched commit"
    );
}

/// Test that add_members still works with the direct commit path when proposals are disabled.
#[xmtp_common::test(unwrap_try = true)]
async fn test_add_members_direct_commit_when_proposals_disabled() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Create group with alix + bo (proposals NOT enabled)
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Verify proposals are NOT enabled
    let proposals_enabled = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(!proposals_enabled, "Proposals should NOT be enabled");

    // Add caro via add_members — this should use the direct commit path
    alix_group.add_members(&[caro.inbox_id()]).await?;

    // Bo syncs
    bo_group.sync().await?;

    // Caro receives welcome
    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(
        caro_groups.len(),
        1,
        "Caro should receive exactly one welcome"
    );

    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify all members see 3 members
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    let caro_members = caro_group.members().await?;
    assert_eq!(alix_members.len(), 3, "Alix should see 3 members");
    assert_eq!(bo_members.len(), 3, "Bo should see 3 members");
    assert_eq!(caro_members.len(), 3, "Caro should see 3 members");
}

/// Test that commit_pending_proposals batches GCE and commit when proposals come from
/// a different member (Bob proposes, Alice commits).
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_pending_proposals_batches_gce_and_commit() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Create group with alix + bo
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Bo proposes to add Caro
    let bo_db = bo_group.context.db();
    let propose_intent = bo_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        bo_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    bo_group
        .sync_until_intent_resolved(propose_intent.id)
        .await?;

    // Alix syncs to receive Bo's proposal
    alix_group.sync().await?;

    // Verify Alix has pending proposals
    let pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert!(pending > 0, "Alix should have pending proposals from Bo");

    // Alix commits all pending proposals — should batch GCE + commit in one operation
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

    // Bo syncs to see the commit
    bo_group.sync().await?;

    // Caro should receive a welcome
    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(
        caro_groups.len(),
        1,
        "Caro should receive exactly one welcome"
    );

    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify all members see 3 members
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    let caro_members = caro_group.members().await?;
    assert_eq!(alix_members.len(), 3, "Alix should see 3 members");
    assert_eq!(bo_members.len(), 3, "Bo should see 3 members");
    assert_eq!(caro_members.len(), 3, "Caro should see 3 members");

    // Verify no pending proposals remain
    let pending_after = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert_eq!(
        pending_after, 0,
        "Should have no pending proposals after commit"
    );
}

// =============================================================================
// Sequence ID Update (No Membership Change) Tests
// =============================================================================

/// Test that a sequence ID bump (new installation) without any add/remove triggers a GCE update
/// when proposals are enabled and add_missing_installations is called.
///
/// This exercises the extension change detection fix: comparing the full GroupMembership
/// (including sequence IDs) rather than just the members map keys.
#[xmtp_common::test(unwrap_try = true)]
async fn test_sequence_id_bump_triggers_gce_with_proposals_enabled() {
    use crate::groups::validated_commit::extract_group_membership;

    tester!(alix);
    tester!(bo);

    // Create group with alix + bo
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Capture bo's sequence ID before the bump
    let bo_seq_before = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            Ok::<Option<u64>, crate::groups::GroupError>(membership.get(bo.inbox_id()).copied())
        })
        .await?;

    // Bo creates a second installation — this bumps bo's identity sequence ID on the network
    tester!(_bo2, from: bo);

    // Alix calls add_missing_installations, which detects the bumped sequence ID
    // and queues an UpdateGroupMembership intent with the new sequence ID
    alix_group.add_missing_installations().await?;

    // Capture bo's sequence ID after the update
    let bo_seq_after = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            Ok::<Option<u64>, crate::groups::GroupError>(membership.get(bo.inbox_id()).copied())
        })
        .await?;

    // The sequence ID should have been bumped
    assert!(
        bo_seq_after > bo_seq_before,
        "Bo's sequence ID should have increased after adding a new installation. Before: {:?}, After: {:?}",
        bo_seq_before,
        bo_seq_after,
    );

    // Bo syncs to see the updated membership
    bo_group.sync().await?;

    // Verify member count is unchanged (no adds/removes, just a sequence ID bump)
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    assert_eq!(alix_members.len(), 2, "Should still have 2 members");
    assert_eq!(bo_members.len(), 2, "Should still have 2 members");
}

/// Test that after a sequence ID bump (new installation), the proposal-based add-member path
/// still works correctly: the batched proposal path produces a GCE with the current sequence IDs.
///
/// This verifies that compute_publish_data_for_proposal_based_update correctly compares
/// the full GroupMembership (including sequence IDs) when deciding whether a GCE is needed.
#[xmtp_common::test(unwrap_try = true)]
async fn test_add_member_after_sequence_id_bump_with_proposals_enabled() {
    use crate::groups::validated_commit::extract_group_membership;

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Create group with alix + bo
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Bo creates a second installation, bumping his sequence ID
    tester!(_bo2, from: bo);

    // Alix processes the sequence ID bump via add_missing_installations
    alix_group.add_missing_installations().await?;

    // Capture the membership state after the bump
    let membership_after_bump = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            Ok::<(Option<u64>, Option<u64>), crate::groups::GroupError>((
                membership.get(alix.inbox_id()).copied(),
                membership.get(bo.inbox_id()).copied(),
            ))
        })
        .await?;

    // Now add caro via the proposal-based path (add_members uses batched proposals
    // when proposals are enabled)
    alix_group.add_members(&[caro.inbox_id()]).await?;

    // Verify the GCE in the commit preserved the bumped sequence IDs
    let membership_after_add = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            Ok::<(Option<u64>, Option<u64>, Option<u64>), crate::groups::GroupError>((
                membership.get(alix.inbox_id()).copied(),
                membership.get(bo.inbox_id()).copied(),
                membership.get(caro.inbox_id()).copied(),
            ))
        })
        .await?;

    // Sequence IDs for existing members should be >= what they were after the bump
    assert!(
        membership_after_add.0 >= membership_after_bump.0,
        "Alix sequence ID should not regress"
    );
    assert!(
        membership_after_add.1 >= membership_after_bump.1,
        "Bo sequence ID should not regress"
    );
    // Caro should now be in the membership
    assert!(
        membership_after_add.2.is_some(),
        "Caro should be in the membership"
    );

    // Bo syncs
    bo_group.sync().await?;

    // Caro receives welcome and syncs
    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(caro_groups.len(), 1, "Caro should receive a welcome");
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // All members should see 3 members
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;
    let caro_members = caro_group.members().await?;
    assert_eq!(alix_members.len(), 3, "Alix should see 3 members");
    assert_eq!(bo_members.len(), 3, "Bo should see 3 members");
    assert_eq!(caro_members.len(), 3, "Caro should see 3 members");
}

// =============================================================================
// Capability Advertisement Backwards Compatibility
// =============================================================================

/// Migration gate: AppDataDictionary capability is advertised on the
/// creator's KP unconditionally (so `all_members_support_proposals`
/// can pass), and after the bootstrap commit it's in
/// `RequiredCapabilities`. Replaces the older custom-extension
/// (`PROPOSAL_SUPPORT_EXTENSION_ID`) signal with the standard MLS
/// mechanism.
#[xmtp_common::test(unwrap_try = true)]
async fn test_app_data_dictionary_capability_and_required() {
    use openmls::extensions::ExtensionType;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    // Pre-bootstrap: KP advertises AppDataDictionary, but it's NOT in
    // RequiredCapabilities — unmigrated groups don't carry the dict.
    alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let own_caps_exts = mls_group
                .own_leaf_node()
                .expect("group creator must have own leaf")
                .capabilities()
                .extensions()
                .to_vec();
            assert!(
                own_caps_exts.contains(&ExtensionType::AppDataDictionary),
                "creator KP capabilities must advertise AppDataDictionary, got: {own_caps_exts:?}",
            );

            let required = mls_group
                .extensions()
                .required_capabilities()
                .expect("required_capabilities must be set")
                .extension_types()
                .to_vec();
            assert!(
                !required.contains(&ExtensionType::AppDataDictionary),
                "Pre-bootstrap RequiredCapabilities must NOT require AppDataDictionary, got: {required:?}",
            );
            Ok::<(), crate::groups::GroupError>(())
        })
        .await?;

    // Run the bootstrap commit.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    // Post-bootstrap: AppDataDictionary IS in RequiredCapabilities.
    alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let required = mls_group
                .extensions()
                .required_capabilities()
                .expect("required_capabilities must be set after bootstrap")
                .extension_types()
                .to_vec();
            assert!(
                required.contains(&ExtensionType::AppDataDictionary),
                "Post-bootstrap RequiredCapabilities MUST require AppDataDictionary, got: {required:?}",
            );
            Ok::<(), crate::groups::GroupError>(())
        })
        .await?;
}

/// Backwards-compat invariant for the `AppDataUpdate` proposal capability flip.
///
/// The creator advertises `AppDataUpdate` on its own leaf node (so the new
/// commit-with-inline-AppDataUpdate-proposal path works), but the group's
/// `RequiredCapabilities` must NOT require it. Required-but-not-universally-
/// advertised would break OpenMLS's RequiredCapabilities check for any
/// installation whose leaf node only advertises the legacy proposal set,
/// stranding every unmigrated client at join time.
#[xmtp_common::test(unwrap_try = true)]
async fn test_app_data_update_advertised_but_not_required() {
    use openmls::messages::proposals::ProposalType;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let own_proposals = mls_group
                .own_leaf_node()
                .expect("group creator must have own leaf")
                .capabilities()
                .proposals()
                .to_vec();
            assert!(
                own_proposals.contains(&ProposalType::AppDataUpdate),
                "creator leaf must advertise AppDataUpdate, got: {own_proposals:?}",
            );

            let required = mls_group
                .extensions()
                .required_capabilities()
                .expect("required_capabilities must be set")
                .proposal_types()
                .to_vec();
            assert!(
                required.contains(&ProposalType::GroupContextExtensions),
                "GroupContextExtensions must be required, got: {required:?}",
            );
            assert!(
                !required.contains(&ProposalType::AppDataUpdate),
                "AppDataUpdate must NOT be required — would break backwards compat \
                 with legacy leaf nodes. got: {required:?}",
            );

            Ok::<(), crate::groups::GroupError>(())
        })
        .await?;
}

/// Key-package rotation preserves the `AppDataDictionary` capability
/// advertisement. Without this property a member whose KP rotates
/// (e.g. via the periodic 30-day rotation) would lose the capability
/// and become unable to join migrated groups or be added by existing
/// members of one. The advertisement is constructed inside
/// `Identity::new_key_package` from compile-time-known capability
/// extensions, so the property holds by construction — this test
/// pins it against accidental refactors that move the advertisement
/// out of the rotation path.
#[xmtp_common::test(unwrap_try = true)]
async fn test_key_package_rotation_preserves_app_data_dictionary_capability() {
    use openmls::extensions::ExtensionType;

    tester!(alix);
    let installation_id = alix.context.installation_id().to_vec();

    // Fetch the initial KP — confirm AppDataDictionary is advertised
    // (the baseline before rotation).
    let initial = alix
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await?;
    let initial_kp = initial
        .get(&installation_id)
        .expect("initial KP must be present")
        .as_ref()
        .expect("initial KP must verify");
    assert!(
        initial_kp
            .inner
            .leaf_node()
            .capabilities()
            .extensions()
            .contains(&ExtensionType::AppDataDictionary),
        "initial KP must advertise AppDataDictionary"
    );
    // Rotate the key package. (On a fresh test client this may be a
    // no-op — rotation runs only when due — but the capability check
    // below is the contract: whether the function emits a new KP or
    // returns the existing one, the result must still advertise
    // AppDataDictionary.)
    alix.rotate_and_upload_key_package().await?;

    // Fetch the post-rotation KP — must still advertise AppDataDictionary.
    let rotated = alix
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await?;
    let rotated_kp = rotated
        .get(&installation_id)
        .expect("post-rotation KP must be present")
        .as_ref()
        .expect("post-rotation KP must verify");
    assert!(
        rotated_kp
            .inner
            .leaf_node()
            .capabilities()
            .extensions()
            .contains(&ExtensionType::AppDataDictionary),
        "post-rotation KP must still advertise AppDataDictionary"
    );
}

// =============================================================================
// AppDataUpdate Path Tests
// =============================================================================
//
// These tests exercise the AppDataUpdate flow that activates after
// `enable_proposals()` fires the bootstrap commit. They confirm that:
// 1. `update_group_name` and friends still work end-to-end (sender → receiver)
// 2. The capability-gated read accessors return the new value
// 3. The legacy path is unchanged for unmigrated groups
//
// `TEST_REGISTRY_OVERRIDE` stays in `app_data/mod.rs` for synthetic-
// registry unit tests but no integration test in this file needs it —
// bootstrap writes a real `COMPONENT_REGISTRY` entry.

/// `update_group_name` on a group with `proposals_enabled` should:
/// - publish a commit containing an `AppDataUpdate(GROUP_NAME)` proposal,
/// - apply the new name into the OpenMLS AppDataDictionary,
/// - and surface it to peers through the capability-gated read accessor.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_name_via_app_data_update() {
    use xmtp_mls_common::group_mutable_metadata::MetadataField;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Run the real bootstrap commit so the dict carries the
    // registry, immutable seeds, and admin lists.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Sanity check the flag actually flipped from both sides.
    let alix_flag = alix_group
        .load_mls_group_with_lock_async(async |g| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&g))
        })
        .await?;
    assert!(alix_flag, "alix proposals_enabled should be true");
    let bo_flag = bo_group
        .load_mls_group_with_lock_async(async |g| {
            Ok::<bool, crate::groups::GroupError>(bo_group.proposals_enabled(&g))
        })
        .await?;
    assert!(bo_flag, "bo proposals_enabled should be true");

    alix_group
        .update_group_name("AppData Group Name".to_string())
        .await?;

    bo_group.sync().await?;
    assert_eq!(
        bo_group.group_name()?,
        "AppData Group Name",
        "Bo should see the new group name written through the AppData path"
    );
    assert_eq!(
        alix_group.group_name()?,
        "AppData Group Name",
        "Alix should see her own update reflected through the read accessor"
    );

    // The capability-gated `mutable_metadata()` accessor should also surface
    // the new value (it backs `group_name()`, but we exercise it directly to
    // pin the merge-into-GMM path).
    let bo_meta = bo_group.mutable_metadata()?;
    assert_eq!(
        bo_meta
            .attributes
            .get(MetadataField::GroupName.as_str())
            .map(String::as_str),
        Some("AppData Group Name")
    );
}

/// A raw AppData intent bypasses `update_group_name`'s friendly length
/// check. The receiver still rejects it through the component invariant.
#[xmtp_common::test(unwrap_try = true)]
async fn test_receiver_rejects_overlong_metadata_from_raw_app_data_intent() {
    use crate::groups::intents::AppDataUpdateIntentData;

    tester!(alix);
    tester!(bo);

    for (component_id, max_length) in [
        (ComponentId::GROUP_NAME, MAX_GROUP_NAME_LENGTH),
        (ComponentId::GROUP_DESCRIPTION, MAX_GROUP_DESCRIPTION_LENGTH),
        (ComponentId::GROUP_IMAGE_URL, MAX_GROUP_IMAGE_URL_LENGTH),
        (ComponentId::APP_DATA, MAX_APP_DATA_LENGTH),
    ] {
        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        let bo_group = bo.sync_welcomes().await?.first()?.clone();
        alix_group
            .enable_proposals(EnableProposalsOptions::test_default())
            .await?;
        bo_group.sync().await?;
        let before = match component_id {
            ComponentId::GROUP_NAME => bo_group.read_single_component::<GroupNameComponent>()?,
            ComponentId::GROUP_DESCRIPTION => {
                bo_group.read_single_component::<GroupDescriptionComponent>()?
            }
            ComponentId::GROUP_IMAGE_URL => {
                bo_group.read_single_component::<GroupImageUrlComponent>()?
            }
            ComponentId::APP_DATA => bo_group.read_single_component::<AppDataComponent>()?,
            _ => unreachable!(),
        };

        let data: Vec<u8> =
            AppDataUpdateIntentData::new(component_id.as_u16(), vec![b'x'; max_length + 1]).into();
        let intent = QueueIntent::app_data_update()
            .data(data)
            .queue(&alix_group)?;
        let result = alix_group.sync_until_intent_resolved(intent.id).await;
        assert!(
            result.is_err(),
            "{component_id} must reject an overlong raw intent"
        );

        // Safe rejections can return a successful sync summary, so the
        // durable terminal-rejection record is the receiver-side proof.
        let _ = bo_group.sync().await;
        let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
        let rejection = bo
            .context
            .db()
            .read_last_rejection(&topic)?
            .unwrap_or_else(|| panic!("receiver did not reject overlong {component_id}"));
        assert_eq!(
            bo.context.db().topic_progress(&topic)?.processed,
            rejection.sequence_id,
            "receiver did not durably advance past rejected {component_id}"
        );
        let after = match component_id {
            ComponentId::GROUP_NAME => bo_group.read_single_component::<GroupNameComponent>()?,
            ComponentId::GROUP_DESCRIPTION => {
                bo_group.read_single_component::<GroupDescriptionComponent>()?
            }
            ComponentId::GROUP_IMAGE_URL => {
                bo_group.read_single_component::<GroupImageUrlComponent>()?
            }
            ComponentId::APP_DATA => bo_group.read_single_component::<AppDataComponent>()?,
            _ => unreachable!(),
        };
        assert_eq!(
            after, before,
            "receiver state changed after rejected {component_id}"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receiver_rejects_last_super_admin_removal_from_raw_app_data_intent() {
    use crate::groups::intents::AppDataUpdateIntentData;
    use tls_codec::Serialize as _;
    use xmtp_mls_common::tls_set::TlsSetDelta;

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

    let payload = TlsSetDelta::new()
        .remove(xmtp_mls_common::inbox_id::InboxId::from_hex(
            alix.inbox_id(),
        )?)
        .tls_serialize_detached()?;
    let data: Vec<u8> =
        AppDataUpdateIntentData::new(ComponentId::SUPER_ADMIN_LIST.as_u16(), payload).into();
    let intent = QueueIntent::app_data_update()
        .data(data)
        .queue(&alix_group)?;
    let result = alix_group.sync_until_intent_resolved(intent.id).await;
    assert!(result.is_err(), "last super-admin removal must be rejected");

    // A safe receiver rejection can produce an `Ok` sync summary; require
    // the durable record rather than relying on that return value.
    let _ = bo_group.sync().await;
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let rejection = bo
        .context
        .db()
        .read_last_rejection(&topic)?
        .expect("receiver did not reject last-super-admin removal");
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        rejection.sequence_id,
        "receiver did not durably advance past last-super-admin removal"
    );
    let super_admins = bo_group.super_admin_list()?;
    assert_eq!(super_admins, vec![alix.inbox_id().to_string()]);
}

/// `update_group_description` on a `proposals_enabled` group should also
/// flow through the AppData path. This catches any per-field hardcoding
/// (e.g. forgetting to map `Description` → `GROUP_DESCRIPTION`).
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_description_via_app_data_update() {
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

    alix_group
        .update_group_description("AppData Description".to_string())
        .await?;

    bo_group.sync().await?;
    assert_eq!(
        bo_group.group_description()?,
        "AppData Description",
        "Bo should see the new group description through the AppData path"
    );
}

/// Disappearing-message settings MUST survive the bootstrap commit.
///
/// Pins the bug fixed by routing `get_message_expire_at_ns` through
/// the capability-aware `extract_group_mutable_metadata_capability_aware`
/// helper: before the fix, the static
/// `extract_legacy_group_mutable_metadata` swallowed `MissingExtension`
/// on migrated groups, so `get_message_expire_at_ns` returned `None`
/// and every message stored post-bootstrap had `expire_at_ns = None` —
/// no expiry, silently disabling disappearing messages.
#[xmtp_common::test(unwrap_try = true)]
async fn test_disappearing_settings_survive_bootstrap() {
    use xmtp_db::group_message::MsgQueryArgs;
    use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Configure disappearing messages on the unmigrated group. Both
    // `from_ns` and `in_ns` must be > 0 —
    // `MessageDisappearingSettings::is_enabled` is the gate that flips
    // on the expire_at_ns plumbing. `from_ns = 1` is a sentinel-low
    // value (not a real "from" timestamp); the test only needs `> 0`
    // to satisfy `is_enabled`.
    const DISAPPEAR_IN_NS: i64 = 3_600_000_000_000; // 1 hour
    let settings = MessageDisappearingSettings::new(1, DISAPPEAR_IN_NS);
    alix_group
        .update_conversation_message_disappearing_settings(settings)
        .await?;
    bo_group.sync().await?;

    // Send a pre-bootstrap message. Stored expire_at_ns should be set
    // to roughly `now_ns + DISAPPEAR_IN_NS`.
    let pre_send_ns = xmtp_common::time::now_ns();
    alix_group
        .send_message(b"before-bootstrap", SendMessageOpts::default())
        .await?;
    bo_group.sync().await?;
    let bo_pre_msgs = bo_group.find_messages(&MsgQueryArgs::default())?;
    let pre_msg = bo_pre_msgs
        .iter()
        .find(|m| m.decrypted_message_bytes == b"before-bootstrap")
        .expect("bo should have decrypted the pre-bootstrap message");
    let pre_expire = pre_msg.expire_at_ns.expect(
        "pre-bootstrap message should carry an expire_at_ns derived from disappearing settings",
    );
    assert!(
        pre_expire > pre_send_ns,
        "pre-bootstrap expire_at_ns ({pre_expire}) must be in the future of send ts ({pre_send_ns})"
    );
    assert!(
        pre_expire < pre_send_ns + 2 * DISAPPEAR_IN_NS,
        "pre-bootstrap expire_at_ns ({pre_expire}) must be within 2x the disappear window of send ts ({pre_send_ns}); \
         catches a regression that stores `Some(now_ns())` (zero-duration) or `Some(garbage)`"
    );

    // Run the real bootstrap commit — strips the legacy GMM extension.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Send a post-bootstrap message. Before the capability-aware fix,
    // `get_message_expire_at_ns` returned None here because it read
    // the (now-absent) legacy GMM extension. After the fix, the dict
    // overlay supplies the disappearing settings and expire_at_ns is
    // populated.
    let post_send_ns = xmtp_common::time::now_ns();
    alix_group
        .send_message(b"after-bootstrap", SendMessageOpts::default())
        .await?;
    bo_group.sync().await?;
    let bo_post_msgs = bo_group.find_messages(&MsgQueryArgs::default())?;
    let post_msg = bo_post_msgs
        .iter()
        .find(|m| m.decrypted_message_bytes == b"after-bootstrap")
        .expect("bo should have decrypted the post-bootstrap message");
    let post_expire = post_msg.expire_at_ns.expect(
        "post-bootstrap message MUST carry an expire_at_ns — disappearing settings must survive the legacy GMM strip",
    );
    assert!(
        post_expire > post_send_ns,
        "post-bootstrap expire_at_ns ({post_expire}) must be in the future of send ts ({post_send_ns})"
    );
    assert!(
        post_expire < post_send_ns + 2 * DISAPPEAR_IN_NS,
        "post-bootstrap expire_at_ns ({post_expire}) must be within 2x the disappear window of send ts ({post_send_ns}); \
         catches a regression that stores `Some(now_ns())` or `Some(garbage)`"
    );
}

/// XIP §3.2: a libxmtp client running below the migrator's pkg_version
/// MUST land in `paused_for_version` rather than fork or fail when the
/// migrator calls `enable_proposals()`. The two-step bootstrap in
/// `enable_proposals()` makes this work by writing
/// MIN_SUPPORTED_PROTOCOL_VERSION to legacy GMM **before** the
/// bootstrap commit strips that extension — old clients can still read
/// the version-bump from the legacy GCE path, pause on it, and never
/// process the (legacy-extension-stripping) bootstrap commit they
/// wouldn't understand.
///
/// ## What this test covers and doesn't cover
///
/// Covered: Bo (running the SAME binary as Alix but at the older
/// pkg_version) processes step A's legacy GCE bump, hits the
/// version-mismatch arm of `validate_one_commit`, and lands in
/// `paused_for_version`. He never applies step B.
///
/// NOT covered: a TRULY pre-AppData binary processing step B and
/// failing because the bootstrap commit strips extensions it requires.
/// That code path is impossible to exercise in-tree (the only client
/// is the current binary), so the test confirms the pause hint is
/// reachable via the legacy reader — that's the contract that lets
/// a pre-AppData binary pause without ever opening step B.
#[xmtp_common::test(unwrap_try = true)]
async fn test_enable_proposals_pauses_old_client_via_legacy_gmm_bump() {
    use crate::builder::ClientBuilder;
    use crate::groups::tests::increment_patch_version;
    use crate::utils::VersionInfo;
    use xmtp_cryptography::utils::generate_local_wallet;

    let mut alix_version = VersionInfo::default();
    alix_version.test_update_version(
        increment_patch_version(alix_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let alix_pkg_version = alix_version.pkg_version().to_string();
    // Alix is on the newer version; bo is on the default (older).
    let alix =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), alix_version).await;

    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;

    // Bo joins the group at his current (older) version. No min-version
    // requirement yet, so the welcome itself doesn't pause him.
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;
    assert!(
        bo_group.paused_for_version()?.is_none(),
        "Bo should not be paused before alix calls enable_proposals"
    );
    let before = bo_group.epoch_authenticator().await?;
    let db_topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let processed = bo.context.db().topic_progress(&db_topic)?.processed;

    // Alix migrates. The two-step bootstrap publishes:
    //   1. A legacy GCE commit bumping MIN_SUPPORTED_PROTOCOL_VERSION
    //      in the still-present legacy GMM extension.
    //   2. The bootstrap commit (strips legacy extensions, seeds dict).
    // Pass alix's pkg_version as the floor explicitly: the test
    // default's "0.0.0" floor would skip the step-A pause hint that's
    // the whole subject of this test.
    alix_group
        .enable_proposals(EnableProposalsOptions {
            force: false,
            min_version: Some(alix_pkg_version.clone()),
        })
        .await?;

    // The version bump stays pending. Neither bootstrap commit may apply.
    super::assert_version_sync_blocked(
        bo_group.sync().await.unwrap_err(),
        &xmtp_proto::types::Topic::new_group_message(bo_group.group_id),
        processed,
    );
    assert_eq!(bo_group.epoch_authenticator().await?, before);
    assert_eq!(
        bo.context.db().topic_progress(&db_topic)?.processed,
        processed
    );

    let paused = bo_group.paused_for_version()?;
    assert_eq!(
        paused.as_deref(),
        Some(alix_pkg_version.as_str()),
        "Bo must be paused at alix's pkg_version — the legacy GMM bump is the pause hint old clients can read"
    );

    // Bo's group must NOT show as migrated. The bootstrap commit
    // strips legacy GMM and seeds the AppData dict; if Bo applied it
    // he'd be migrated but unable to ever read the pause hint.
    let bo_migrated = bo_group
        .load_mls_group_with_lock_async(async |g| {
            Ok::<bool, crate::groups::GroupError>(bo_group.proposals_enabled(&g))
        })
        .await?;
    assert!(
        !bo_migrated,
        "Bo must not have processed the bootstrap commit — it ships after the pause-triggering legacy bump"
    );

    // Alix is at the floor version, so she runs both commits and ends
    // up migrated as normal.
    let alix_migrated = alix_group
        .load_mls_group_with_lock_async(async |g| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&g))
        })
        .await?;
    assert!(
        alix_migrated,
        "Alix should be migrated post-enable_proposals"
    );
}

// Two areas still rely on indirect coverage:
//
// 1. **Standalone-proposal `validate_proposal` arm.** PR-C (standalone
//    proposal-by-reference flow) now publishes `AppDataUpdate` proposals
//    as separate MLS messages preceding the commit, so the
//    `Proposal::AppDataUpdate` arm of `validate_proposal` (the path
//    that handles a proposal received *outside* a commit) is reachable
//    by any update via `update_group_name` / `update_admin_list` /
//    `update_permissions`. The end-to-end tests above exercise it via
//    the receiver's normal commit-processing pipeline, which routes
//    standalone proposals into the same `validate_one_app_data_update`
//    helper as the inline-bundled path; a regression that broke
//    permission enforcement would trip either entry point.
//
// 2. **`RemoveByHash` resolution through the validator.** No production
//    code path currently emits `RemoveByHash` (admin-list paths use
//    explicit `Remove(inbox_id)` mutations). Unit coverage for the
//    resolver lives in
//    `crates/xmtp_mls/src/groups/app_data/component_source.rs` under
//    `test_expand_remove_by_hash_*`; revisit if a future caller starts
//    emitting hash-based deletes.
/// Sanity check the legacy path: a group with `proposals_enabled = false`
/// (the default for fresh groups) should still produce a normal GCE commit
/// for `update_group_name`, with no AppDataUpdate involvement. Confirms
/// that introducing the new branch hasn't accidentally affected unmigrated
/// groups.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_name_uses_legacy_path_when_proposals_disabled() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Sanity: proposals_enabled is false on a fresh group.
    let flag = alix_group
        .load_mls_group_with_lock_async(async |g| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&g))
        })
        .await?;
    assert!(
        !flag,
        "Fresh groups should not have proposals_enabled set by default"
    );

    alix_group
        .update_group_name("Legacy Path Name".to_string())
        .await?;
    bo_group.sync().await?;

    assert_eq!(bo_group.group_name()?, "Legacy Path Name");
    assert_eq!(alix_group.group_name()?, "Legacy Path Name");
}

// `test_update_group_name_uses_legacy_path_when_registry_is_empty`
// removed: its premise was that flipping `enable_proposals()` left
// the AppData dictionary empty so the per-component sender gate
// `proposals_enabled && !registry.is_empty()` would still route
// through the legacy GCE path. With `enable_proposals()` now firing
// the bootstrap migration end-to-end, the registry is always
// populated post-flip and the dict always carries the seeded
// components — the "empty registry, proposals_enabled on" state the
// test checked is no longer reachable. The two gates the test was
// pinning are still covered:
//   - `proposals_enabled` defaults to false: `test_proposals_enabled_default_false`.
//   - Pre-flip groups stay on the legacy GCE path:
//     `test_update_group_name_uses_legacy_path_when_proposals_disabled`.

/// An inline update must obey the registry policy and leave the group unchanged.
/// The failed intent must return its exact permission cause in the sync summary.
#[xmtp_common::test(unwrap_try = true)]
async fn test_inline_app_data_update_denied_by_registry_policy() {
    use crate::groups::{
        GroupError,
        intents::{PermissionPolicyOption, PermissionUpdateType},
        mls_sync::GroupMessageProcessingError,
        validated_commit::CommitValidationError,
    };
    use xmtp_mls_common::group_mutable_metadata::MetadataField;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Bootstrap and tighten GROUP_NAME's update policy to Deny so
    // any subsequent update_group_name is rejected by the validator.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;
    alix_group
        .update_permission_policy(
            PermissionUpdateType::UpdateMetadata,
            PermissionPolicyOption::Deny,
            Some(MetadataField::GroupName),
        )
        .await?;
    bo_group.sync().await?;

    // Capture the pre-update group name so we can assert it didn't change.
    let original = alix_group.group_name()?;

    // Attempt the update. The validator should reject the AppDataUpdate
    // proposal because GROUP_NAME's update_policy is now `Deny`.
    // Matching `Sync(_)` is tighter than `.is_err()` — it rules out
    // Api, Storage, Client, and wrong-epoch failures.
    let result = alix_group
        .update_group_name("Should Be Rejected".to_string())
        .await;
    let Err(GroupError::Sync(summary)) = result else {
        panic!("expected Err(GroupError::Sync(_)), got {result:?}");
    };

    // The non-retryable own-commit validation failure must be preserved in the
    // summary rather than swallowed: the intent flips to Error, but the typed
    // CommitValidationError now rides out through process.errored so the cause
    // is reportable instead of surfacing as a misleading "0 failed" success.
    assert!(
        summary.process.errored.iter().any(|(_, e)| matches!(
            e,
            GroupMessageProcessingError::CommitValidation(
                CommitValidationError::InsufficientPermissions
            )
        )),
        "summary should carry the CommitValidation cause, got: {summary}"
    );

    // Group name unchanged because the rejected commit never made
    // it past validation.
    assert_eq!(
        alix_group.group_name()?,
        original,
        "group name should be unchanged after the rejected update"
    );
}

// After bootstrap, a group's legacy GMM extension is removed entirely —
// so a Layer-4 "dict-wins-over-legacy" overlay test no longer fits the
// post-migration model. The dict is now the *only* source of truth for
// migrated groups, and `test_update_group_name_via_app_data_update`
// already exercises the dict→read path end-to-end. The underlying
// merge-on-conflict logic stays around as defense-in-depth for any
// transitional state but isn't reachable through the public API once
// `enable_proposals()` does the full bootstrap.

/// Pin the intra-batch chaining invariant in
/// [`super::super::app_data::accumulate_app_data_updates`]: when two
/// proposals target the same `ComponentId` inside one batch, the second
/// proposal's payload must be applied *on top of* the first proposal's
/// synthesized new value — not against the stale pre-batch dict state.
///
/// We target `ADMIN_LIST` with two `TlsSetDelta::insert` deltas so the
/// final serialized value is observably different depending on whether
/// the chaining happened:
///
/// - With chaining (correct): `{alice, bob}`
/// - Without chaining (bug): `{bob}` — the second insert's `old_value`
///   would be the empty pre-batch set, overwriting Alice's entry.
///
/// This is the invariant the upcoming bootstrap commit (which emits
/// many `AppDataUpdate(COMPONENT_REGISTRY, …)` in a row) relies on.
#[xmtp_common::test(unwrap_try = true)]
async fn test_accumulate_app_data_updates_chains_intra_batch() {
    use crate::groups::app_data::{accumulate_app_data_updates, component_source};
    use openmls::messages::proposals::AppDataUpdateOperation;
    use tls_codec::Deserialize;
    use xmtp_mls_common::{
        app_data::component_id::ComponentId, inbox_id::InboxId, tls_set::TlsSet,
    };

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let alice = hex::encode([0x01u8; 32]);
    let bob = hex::encode([0x02u8; 32]);

    let alice_insert = component_source::encode_app_data_update_payload(
        &component_source::ComponentMutation::AdminListAdd { inbox_id: &alice },
    )?;
    let bob_insert = component_source::encode_app_data_update_payload(
        &component_source::ComponentMutation::AdminListAdd { inbox_id: &bob },
    )?;
    let op_alice = AppDataUpdateOperation::Update(alice_insert.into());
    let op_bob = AppDataUpdateOperation::Update(bob_insert.into());
    let openmls_id = ComponentId::ADMIN_LIST.as_u16();

    let updates: openmls::group::AppDataUpdates = alix_group
        .load_mls_group_with_lock_async(async |g| {
            let out =
                accumulate_app_data_updates(&g, [(openmls_id, &op_alice), (openmls_id, &op_bob)])
                    .map_err(crate::groups::GroupError::from)?;
            Ok::<openmls::group::AppDataUpdates, crate::groups::GroupError>(
                out.expect("at least one update should be returned"),
            )
        })
        .await?;

    // Pull the final bytes back out of AppDataUpdates and deserialize as a
    // TlsSet to assert both inbox ids made it through.
    let mut final_bytes: Option<Vec<u8>> = None;
    for (id, value) in updates {
        if id == openmls_id {
            final_bytes = value;
        }
    }
    let bytes = final_bytes.expect("ADMIN_LIST entry should be Some (Insert, not Remove)");
    let set = TlsSet::<InboxId>::tls_deserialize_exact(&bytes)?;
    assert_eq!(
        set.len(),
        2,
        "second insert was dropped — batching did not chain"
    );
    assert!(
        set.contains(&InboxId::from_bytes([0x01; 32])),
        "Alice missing from final set"
    );
    assert!(
        set.contains(&InboxId::from_bytes([0x02; 32])),
        "Bob missing from final set"
    );
}

// =============================================================================
// Sender-path tests for `IntentKind::UpdateAdminList` and
// `IntentKind::UpdatePermission`. The AppDataUpdate path activates on
// migrated groups (`is_migrated_group(...)` true). Tests use
// `with_permissive_registry` (the `TEST_REGISTRY_OVERRIDE` helper) so
// we can exercise the path without running a full bootstrap commit.
// =============================================================================

/// `update_admin_list(Add, bo)` on a migrated group should publish an
/// `AppDataUpdate(ADMIN_LIST, Update(TlsSetDelta::insert(bo)))` proposal,
/// apply the new admin list into the OpenMLS AppData dictionary, and
/// surface bo as an admin to peers via `mutable_metadata().admin_list`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_list_add_via_app_data_path_after_migration() {
    use crate::groups::UpdateAdminListType;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Run the real bootstrap commit. After this the dict carries
    // the registry, the immutable seeds, and the admin lists, and
    // the legacy XMTP extensions are gone.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    bo_group.sync().await?;

    // Promote bo to admin via the host-facing API. Internally queues
    // `IntentKind::UpdateAdminList` which routes through the
    // AppDataUpdate path on this migrated group.
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    bo_group.sync().await?;

    // Both peers should see bo in admin_list and bo NOT in
    // super_admin_list (Add must not have routed to the wrong
    // list). super_admin_list still contains alix (creator); we
    // only assert bo isn't there, not that it's empty. Asserting
    // per peer catches consensus drift (one peer sees the update,
    // the other doesn't).
    for (label, meta) in [
        ("alix", alix_group.mutable_metadata()?),
        ("bo", bo_group.mutable_metadata()?),
    ] {
        assert!(
            meta.admin_list.contains(&bo.inbox_id().to_string()),
            "{label} should see bo as admin, admin_list={:?}",
            meta.admin_list,
        );
        assert!(
            !meta.super_admin_list.contains(&bo.inbox_id().to_string()),
            "{label} super_admin_list should not contain bo after Add, got {:?}",
            meta.super_admin_list,
        );
    }
}

/// Round-trip: add then remove. With the real bootstrap commit the
/// immutable seeds are in the dict, so the second `update_admin_list`
/// call's `metadata()` read works on the migrated group.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_list_remove_via_app_data_path_after_migration() {
    use crate::groups::UpdateAdminListType;

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

    alix_group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    alix_group
        .update_admin_list(UpdateAdminListType::Remove, bo.inbox_id().to_string())
        .await?;
    bo_group.sync().await?;

    for (label, meta) in [
        ("alix", alix_group.mutable_metadata()?),
        ("bo", bo_group.mutable_metadata()?),
    ] {
        assert!(
            !meta.admin_list.contains(&bo.inbox_id().to_string()),
            "{label} admin_list should not contain bo after remove, got {:?}",
            meta.admin_list,
        );
    }
}

/// `update_admin_list(AddSuper, bo)` should target the SUPER_ADMIN_LIST
/// component rather than ADMIN_LIST. Confirms the action→component
/// mapping in the sender's match arm.
#[xmtp_common::test(unwrap_try = true)]
async fn test_super_admin_list_add_via_app_data_path_after_migration() {
    use crate::groups::UpdateAdminListType;

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

    // AddSuper targets SUPER_ADMIN_LIST per the sender's mapping.
    alix_group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    bo_group.sync().await?;

    // Both peers should see bo in SUPER_ADMIN_LIST and ADMIN_LIST
    // untouched. The `is_empty()` check on ADMIN_LIST is stronger
    // than `!contains(bo)` — admin_list starts empty on a fresh
    // group, so the weaker check passes even if AddSuper routed to
    // the wrong list with a different inbox.
    for (label, meta) in [
        ("alix", alix_group.mutable_metadata()?),
        ("bo", bo_group.mutable_metadata()?),
    ] {
        assert!(
            meta.super_admin_list.contains(&bo.inbox_id().to_string()),
            "{label} should see bo as super admin, super_admin_list={:?}",
            meta.super_admin_list,
        );
        assert!(
            meta.admin_list.is_empty(),
            "{label} AddSuper should not have touched ADMIN_LIST, got {:?}",
            meta.admin_list,
        );
    }
}

/// `update_permission_policy(UpdateMetadata, GROUP_NAME, AdminOnly)`
/// on a migrated group should publish an
/// `AppDataUpdate(COMPONENT_REGISTRY, Update(TlsMapDelta::update(GROUP_NAME, …)))`
/// proposal that mutates the affected component's metadata in the
/// registry. Verify by re-reading the registry post-commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_permission_update_via_app_data_path_after_migration() {
    use crate::groups::intents::{PermissionPolicyOption, PermissionUpdateType};
    use xmtp_mls_common::{
        app_data::component_id::ComponentId, group_mutable_metadata::MetadataField,
    };
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

    // Tighten GROUP_NAME's update_policy from `Allow` (the default
    // synthesized at bootstrap) to `AdminOnly`.
    alix_group
        .update_permission_policy(
            PermissionUpdateType::UpdateMetadata,
            PermissionPolicyOption::AdminOnly,
            Some(MetadataField::GroupName),
        )
        .await?;
    bo_group.sync().await?;

    // Both sides should see the registry entry mutated.
    for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
        let registry = group
            .load_mls_group_with_lock_async(async |mls_group| {
                Ok::<_, crate::groups::GroupError>(
                    crate::groups::app_data::load_component_registry(&mls_group)?,
                )
            })
            .await?;
        let entry = registry
            .get(&ComponentId::GROUP_NAME)
            .unwrap()
            .unwrap_or_else(|| panic!("{label} registry missing GROUP_NAME entry"));
        let perms = entry
            .permissions
            .as_ref()
            .unwrap_or_else(|| panic!("{label} GROUP_NAME entry missing permissions"));
        let update_kind = perms
            .update_policy
            .as_ref()
            .and_then(|p| p.kind.as_ref())
            .unwrap_or_else(|| panic!("{label} GROUP_NAME missing update_policy.kind"));
        let insert_kind = perms
            .insert_policy
            .as_ref()
            .and_then(|p| p.kind.as_ref())
            .unwrap_or_else(|| panic!("{label} GROUP_NAME missing insert_policy.kind"));
        match update_kind {
            MetadataPolicyKind::Base(base) => assert_eq!(
                *base,
                MetadataBasePolicy::AllowIfAdmin as i32,
                "{label} GROUP_NAME update_policy not tightened, got base={base}"
            ),
            other => panic!("{label} GROUP_NAME update_policy unexpected variant: {other:?}"),
        }
        match insert_kind {
            MetadataPolicyKind::Base(base) => assert_eq!(
                *base,
                MetadataBasePolicy::AllowIfAdmin as i32,
                "{label} GROUP_NAME insert_policy not tightened, got base={base}"
            ),
            other => panic!("{label} GROUP_NAME insert_policy unexpected variant: {other:?}"),
        }
    }
}

/// Sanity check: on an *unmigrated* group, the same admin-list update
/// API still works through the legacy GCE path and produces the same
/// observable state. Catches a regression where the dual-routing gate
/// might mis-fire on unmigrated groups.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_list_add_unchanged_on_unmigrated_group() {
    use crate::groups::UpdateAdminListType;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Note: NO `enable_proposals()` and NO `with_permissive_registry()`.
    // The dual-routing gate is closed; the legacy GCE path runs.
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    bo_group.sync().await?;

    let alix_meta = alix_group.mutable_metadata()?;
    assert!(
        alix_meta.admin_list.contains(&bo.inbox_id().to_string()),
        "legacy GCE admin-list update broke, admin_list={:?}",
        alix_meta.admin_list,
    );
    let bo_meta = bo_group.mutable_metadata()?;
    assert!(
        bo_meta.admin_list.contains(&bo.inbox_id().to_string()),
        "bo should see himself as admin via legacy GMM, admin_list={:?}",
        bo_meta.admin_list,
    );
}
