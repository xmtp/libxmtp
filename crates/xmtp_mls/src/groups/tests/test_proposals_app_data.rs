//! Batched proposals, sequence ids, and app-data dictionary updates.

use crate::{
    context::XmtpSharedContext,
    groups::intents::{
        CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData, QueueIntent,
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

#[xmtp_common::test(unwrap_try = true)]
async fn test_permission_updates_preserve_pending_fields() {
    use crate::groups::{
        GroupError, UpdateAdminListType,
        app_data::sender_intents::apply_update_permission_app_data_intent,
        intents::{PermissionPolicyOption, PermissionUpdateType, UpdatePermissionIntentData},
    };
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{Kind, MetadataBasePolicy};

    for other_member in [false, true] {
        tester!(alix);
        tester!(bo);
        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await
            .unwrap();
        let bo_groups = bo.sync_welcomes().await.unwrap();
        let bo_group = bo_groups.first().unwrap();
        alix_group
            .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
            .await
            .unwrap();
        bo_group.sync().await.unwrap();

        let author = if other_member { bo_group } else { &alix_group };
        // Use the permission writer, then publish only its proposals. This models
        // a prepared permission intent whose commit has not reached the group.
        let mut payloads = crate::state_tx::state_write(author.context.mls_storage(), |tx| {
            tx.with_group(author.group_id, |group, storage| {
                let publish = apply_update_permission_app_data_intent(
                    storage,
                    group,
                    UpdatePermissionIntentData::new(
                        PermissionUpdateType::AddMember,
                        PermissionPolicyOption::Deny,
                        None,
                    ),
                    &author.context.identity().installation_keys,
                    false,
                )
                .unwrap();
                Ok::<_, GroupError>(xmtp_db::TransactionOutcome::Continue(
                    publish.payloads_to_publish,
                ))
            })
        })
        .unwrap()
        .into_continued();
        payloads
            .pop()
            .expect("permission writer must produce a commit");
        let messages = author
            .prepare_group_messages(
                payloads
                    .iter()
                    .map(|payload| (payload.as_slice(), false))
                    .collect(),
            )
            .unwrap();
        author
            .context
            .api()
            .send_group_messages(messages)
            .await
            .unwrap();
        alix_group.sync().await.unwrap();
        let pending = alix_group
            .with_group_snapshot(|group| Ok::<_, GroupError>(group.pending_proposals().count()))
            .unwrap();
        assert!(pending > 0, "the first permission update must stay pending");

        alix_group
            .update_permission_policy(
                PermissionUpdateType::RemoveMember,
                PermissionPolicyOption::SuperAdminOnly,
                None,
            )
            .await
            .unwrap();
        bo_group.sync().await.unwrap();

        for group in [&alix_group, bo_group] {
            let permissions = group
                .with_group_snapshot(|group| {
                    Ok::<_, GroupError>(
                        crate::groups::app_data::load_component_registry(group)
                            .unwrap()
                            .get(&ComponentId::GROUP_MEMBERSHIP)
                            .expect("valid membership entry")
                            .expect("membership entry")
                            .permissions
                            .expect("membership permissions"),
                    )
                })
                .unwrap();
            assert_eq!(
                permissions.insert_policy.unwrap().kind,
                Some(Kind::Base(MetadataBasePolicy::Deny as i32)),
                "the pending AddMember restriction must survive the RemoveMember update",
            );
            assert_eq!(
                permissions.delete_policy.unwrap().kind,
                Some(Kind::Base(MetadataBasePolicy::AllowIfSuperAdmin as i32)),
            );
        }
    }
}

/// Test that add_members uses the batched proposal path on a dictionary-native group.
/// UpdateGroupMembership should create Add proposals + GCE + commit
/// in a single publish, rather than a direct commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_add_members_batched_on_dictionary_group() {
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

    bo_group.sync().await?;

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
/// on a dictionary-native group when add_missing_installations is called.
///
/// This exercises the extension change detection fix: comparing the full GroupMembership
/// (including sequence IDs) rather than just the members map keys.
#[xmtp_common::test(unwrap_try = true)]
async fn test_sequence_id_bump_triggers_gce_on_dictionary_group() {
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
async fn test_add_member_after_sequence_id_bump_on_dictionary_group() {
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
// These tests exercise AppDataUpdate on dictionary-native groups. They confirm
// that updates replicate end-to-end and read accessors return the new value.

/// `update_group_name` on a dictionary-native group should:
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

    // Creation writes the dictionary with the registry, immutable seeds,
    // and admin lists.

    bo_group.sync().await?;

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

/// `update_group_description` on a dictionary-native group should also
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

    // Tighten GROUP_NAME's update policy to Deny so
    // any subsequent update_group_name is rejected by the validator.

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
/// Creation writes a component registry and later updates can contain
/// several AppDataUpdate proposals in one batch.
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
// `IntentKind::UpdatePermission`. New groups use the AppDataUpdate path.
// =============================================================================

/// `update_admin_list(Add, bo)` on a dictionary-native group should publish an
/// `AppDataUpdate(ADMIN_LIST, Update(TlsSetDelta::insert(bo)))` proposal,
/// apply the new admin list into the OpenMLS AppData dictionary, and
/// surface bo as an admin to peers via `mutable_metadata().admin_list`.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_list_add_via_app_data_path() {
    use crate::groups::UpdateAdminListType;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Creation already writes the registry, immutable seeds, and admin lists.

    bo_group.sync().await?;

    // Promote bo to admin via the host-facing API. Internally queues
    // `IntentKind::UpdateAdminList` which routes through the
    // AppDataUpdate path on this dictionary-native group.
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

/// Round-trip: add then remove on a dictionary-native group.
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_list_remove_via_app_data_path() {
    use crate::groups::UpdateAdminListType;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

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
async fn test_super_admin_list_add_via_app_data_path() {
    use crate::groups::UpdateAdminListType;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

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
/// on a dictionary-native group should publish an
/// `AppDataUpdate(COMPONENT_REGISTRY, Update(TlsMapDelta::update(GROUP_NAME, …)))`
/// proposal that mutates the affected component's metadata in the
/// registry. Verify by re-reading the registry post-commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_permission_update_via_app_data_path() {
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

    bo_group.sync().await?;

    // Tighten GROUP_NAME's update_policy from `Allow` (the default
    // written at creation) to `AdminOnly`.
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
