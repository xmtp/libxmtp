//! Proposal tests: edge cases.

use super::*;

// =============================================================================
// Edge Case Tests
// =============================================================================

/// Test that committing with no pending proposals handles gracefully.
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_with_no_pending_proposals() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    bo.sync_welcomes().await?;

    // Verify there are no pending proposals
    let has_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<bool, crate::groups::GroupError>(
                openmls_group.pending_proposals().next().is_some(),
            )
        })
        .await?;

    assert!(!has_pending, "Should have no pending proposals initially");

    // Try to commit with no pending proposals
    let db = alix_group.context.db();
    let commit_intent = db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::CommitPendingProposals,
        alix_group.group_id,
        CommitPendingProposalsIntentData::default().into(),
        false,
    ))?;

    // This should complete without error
    let result = alix_group
        .sync_until_intent_resolved(commit_intent.id)
        .await;

    tracing::info!("Commit with no proposals result: {:?}", result.is_ok());

    // Verify group state is unchanged
    let members = alix_group.members().await?;
    assert_eq!(members.len(), 2);
}

/// Test edge cases for proposing to add/remove members.
#[rstest]
#[case::add_existing_member(true)]
#[case::remove_nonexistent_member(false)]
#[xmtp_common::test]
async fn test_propose_invalid_member_operations(#[case] is_add: bool) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .unwrap();

    let bo_groups = bo.wait_for_welcomes().await.unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();

    let members = alix_group.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Enable proposals

    bo_group.sync().await.unwrap();

    let db = alix_group.context.db();

    let (kind, intent_bytes) = if is_add {
        // Try to add bo who is already in the group
        (
            IntentKind::ProposeMemberUpdate,
            ProposeMemberUpdateIntentData::new(vec![bo.inbox_id().to_string()], vec![])
                .try_into()
                .unwrap(),
        )
    } else {
        // Try to remove caro who is not in the group
        (
            IntentKind::ProposeMemberUpdate,
            ProposeMemberUpdateIntentData::new(vec![], vec![caro.inbox_id().to_string()])
                .try_into()
                .unwrap(),
        )
    };

    let propose_intent = db
        .insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            kind,
            alix_group.group_id,
            intent_bytes,
            false,
        ))
        .unwrap();

    // Execute - the system should handle this gracefully
    let result = alix_group
        .sync_until_intent_resolved(propose_intent.id)
        .await;

    tracing::info!(
        "Invalid member operation (is_add={}) result: {:?}",
        is_add,
        result.is_ok()
    );

    // Group should still be functional with same members
    let members_after = alix_group.members().await.unwrap();
    assert_eq!(members_after.len(), 2);
}

/// Test that sending a message when there are pending proposals auto-commits them.
/// This verifies that the SendMessage handler automatically queues a CommitPendingProposals
/// intent and retries, ensuring seamless messaging even with pending proposals.
#[xmtp_common::test(unwrap_try = true)]
async fn test_message_auto_commits_pending_proposals() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Send message before proposal - this should work
    alix_group
        .send_message(b"Before proposal", SendMessageOpts::default())
        .await?;

    bo_group.sync().await?;
    let messages = bo_group.find_messages(&Default::default())?;
    let has_message = messages
        .iter()
        .any(|m| m.decrypted_message_bytes == b"Before proposal");
    assert!(has_message);

    // Enable proposals

    bo_group.sync().await?;

    // Alix proposes to add caro
    let db = alix_group.context.db();
    let propose_intent = db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        alix_group.group_id,
        ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;

    alix_group
        .sync_until_intent_resolved(propose_intent.id)
        .await?;

    // Verify pending proposals exist
    let alix_has_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<bool, crate::groups::GroupError>(
                openmls_group.pending_proposals().next().is_some(),
            )
        })
        .await?;

    assert!(
        alix_has_pending,
        "Alix should have pending proposals before sending message"
    );

    // Sending a message with pending proposals should auto-commit the proposals
    // The SendMessage handler queues a CommitPendingProposals intent and retries
    let send_result = alix_group
        .send_message(b"After proposal", SendMessageOpts::default())
        .await;

    assert!(
        send_result.is_ok(),
        "Sending messages should succeed - auto-commits pending proposals: {:?}",
        send_result.err()
    );

    // After auto-commit, pending proposals should be cleared
    let alix_has_pending_after = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<bool, crate::groups::GroupError>(
                openmls_group.pending_proposals().next().is_some(),
            )
        })
        .await?;

    assert!(
        !alix_has_pending_after,
        "Pending proposals should be committed"
    );

    // Caro should have received a welcome (from the auto-committed add proposal)
    let caro_groups = caro.sync_welcomes().await?;
    assert!(
        !caro_groups.is_empty(),
        "Caro should have received a welcome"
    );

    // Verify Caro is now a member
    alix_group.sync().await?;
    let members = alix_group.members().await?;
    let caro_is_member = members.iter().any(|m| m.inbox_id == caro.inbox_id());
    assert!(
        caro_is_member,
        "Caro should be a group member after auto-commit"
    );
}
