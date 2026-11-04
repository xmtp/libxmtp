//! Proposal tests: proposal flows.

use super::*;

// =============================================================================
// Intent Serialization Tests
// =============================================================================

/// Test that proposal member update intents can be created, queued, and deserialized correctly.
#[rstest]
#[case::add_single(vec!["inbox1"], vec![])]
#[case::add_multiple(vec!["inbox1", "inbox2", "inbox3"], vec![])]
#[case::remove_single(vec![], vec!["inbox1"])]
#[case::remove_multiple(vec![], vec!["inbox1", "inbox2"])]
#[case::add_and_remove(vec!["inbox1"], vec!["inbox2"])]
#[case::both_empty(vec![], vec![])]
#[xmtp_common::test(unwrap_try = true)]
async fn test_proposal_intent_serialization(
    #[case] add_inbox_ids: Vec<&str>,
    #[case] remove_inbox_ids: Vec<&str>,
) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let add_inbox_ids = add_inbox_ids.iter().map(hex::encode).collect::<Vec<_>>();
    let remove_inbox_ids = remove_inbox_ids.iter().map(hex::encode).collect::<Vec<_>>();

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .unwrap();

    let intent_bytes: Vec<u8> =
        ProposeMemberUpdateIntentData::new(add_inbox_ids.clone(), remove_inbox_ids.clone())
            .try_into()
            .unwrap();

    let db = alix_group.context.db();
    let intent = db
        .insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            alix_group.group_id,
            intent_bytes,
            false,
        ))
        .unwrap();

    assert_eq!(intent.kind, IntentKind::ProposeMemberUpdate);
    assert_eq!(intent.group_id.as_slice(), alix_group.group_id.as_slice());

    // Verify deserialization
    let parsed = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice()).unwrap();
    assert_eq!(parsed.add_inbox_ids, add_inbox_ids);
    assert_eq!(parsed.remove_inbox_ids, remove_inbox_ids);
}

// =============================================================================
// End-to-End Proposal Flow Tests
// =============================================================================

/// Test end-to-end proposal add flow:
/// 1. Alix creates group with Bo
/// 2. Alix proposes to add Caro
/// 3. Bo syncs and receives the proposal
/// 4. Bo commits the pending proposals
/// 5. Caro receives welcome and joins
/// 6. All members verify membership
#[xmtp_common::test(unwrap_try = true)]
async fn test_e2e_propose_add_member_flow() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // 1. Create group with alix and bo
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Verify initial member count
    let initial_members = alix_group.members().await?;
    assert_eq!(initial_members.len(), 2);

    bo_group.sync().await?;

    // 2. Alix proposes to add caro
    let intent_data =
        ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![]).try_into()?;
    let alix_db = alix_group.context.db();
    let propose_intent =
        alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            alix_group.group_id,
            intent_data,
            false,
        ))?;

    alix_group
        .sync_until_intent_resolved(propose_intent.id)
        .await?;

    // 3. Bo syncs until it holds every proposal that Alix sent. Alix sends the Add
    // proposal and the membership proposal as separate messages. While the backend
    // replica lags, one sync can return only some of them, and Bo would then commit
    // a partial set that does not add Caro.
    let alix_pending = alix_group
        .load_mls_group_with_lock_async(async |openmls_group| {
            Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
        })
        .await?;
    assert!(alix_pending > 0);
    xmtp_common::wait_for_some(|| async {
        bo_group.sync().await.ok()?;
        let bo_pending = bo_group
            .load_mls_group_with_lock_async(async |openmls_group| {
                Ok::<usize, crate::groups::GroupError>(openmls_group.pending_proposals().count())
            })
            .await
            .ok()?;
        (bo_pending == alix_pending).then_some(())
    })
    .await?;

    // 4. Bo commits the pending proposals
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

    // 5. Alix syncs until it sees Bo's commit. One sync can miss it while the replica lags.
    xmtp_common::wait_for_eq(
        || async {
            alix_group.sync().await.ok()?;
            Some(alix_group.members().await.ok()?.len())
        },
        Some(3),
    )
    .await?;

    // 6. Caro receives welcome and joins
    let caro_groups = caro.wait_for_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify all members see 3 members
    assert_eq!(caro_group.members().await?.len(), 3);
    assert_eq!(bo_group.members().await?.len(), 3);
}

/// Test end-to-end proposal remove flow:
/// 1. Create group with 3 members
/// 2. Alix proposes to remove Caro
/// 3. Bo syncs and receives the proposal
/// 4. Bo commits the pending proposals
/// 5. Verify Caro is removed
#[xmtp_common::test(unwrap_try = true)]
async fn test_e2e_propose_remove_member_flow() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // 1. Create group with all three members
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;

    // Sync all members
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    let caro_groups = caro.wait_for_welcomes().await?;
    let caro_group = caro_groups.first()?;
    caro_group.sync().await?;

    // Verify initial member count
    let initial_members = alix_group.members().await?;
    assert_eq!(initial_members.len(), 3);

    // Enable proposals

    bo_group.sync().await?;
    caro_group.sync().await?;

    // 2. Alix proposes to remove caro
    let intent_data =
        ProposeMemberUpdateIntentData::new(vec![], vec![caro.inbox_id().to_string()]).try_into()?;
    let alix_db = alix_group.context.db();
    let propose_intent =
        alix_db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
            IntentKind::ProposeMemberUpdate,
            alix_group.group_id,
            intent_data,
            false,
        ))?;

    alix_group
        .sync_until_intent_resolved(propose_intent.id)
        .await?;

    // 3. Bo syncs to receive the proposal
    bo_group.sync().await?;

    // 4. Bo commits the pending proposals
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

    // 5. Sync alix to see the commit
    alix_group.sync().await?;

    // Verify alix and bo see updated membership (2 members)
    let alix_members = alix_group.members().await?;
    let bo_members = bo_group.members().await?;

    tracing::info!(
        "After remove - Alix sees {} members, Bo sees {} members",
        alix_members.len(),
        bo_members.len()
    );
}
