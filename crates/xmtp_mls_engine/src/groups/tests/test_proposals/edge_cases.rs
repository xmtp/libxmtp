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

/// A member publishes a standalone `GroupContextExtensions` proposal, a
/// commit that carries one inline, and a proposal far beyond the forward
/// window. None of them holds the group head: every other member rejects
/// all three, a super admin removes the member, and the rest of the group
/// keeps receiving messages.
// verifies: GMOD-059, PROC-061
#[xmtp_common::test(unwrap_try = true)]
async fn member_attack_messages_cannot_hold_the_group() {
    use crate::groups::{
        GroupError, mls_ext::commit_without_group_info, mls_sync::generate_prepared_commit,
    };
    use openmls::prelude::{LeafNodeParameters, tls_codec::Serialize as _};
    use openmls_traits::OpenMlsProvider as _;
    use xmtp_db::{
        TransactionOutcome::Continue,
        XmtpOpenMlsProviderRef,
        group_message::MsgQueryArgs,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
        local_commit_log::QueryLocalCommitLog,
        remote_commit_log::CommitResult,
    };
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_proto::types::Topic;

    // Far enough past the generations the members processed.
    const FAR_FORWARD_PROPOSALS: u32 = 1_100;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = super::super::receive_group_invite(&bo).await;
    let caro_group = super::super::receive_group_invite(&caro).await;
    // A rejected commit right after the Welcome is not logged.
    alix_group.update_group_name("before".into()).await?;
    bo_group.receive().await?;
    caro_group.receive().await?;

    let signer = bo.context.identity().installation_keys.clone();
    let messages = state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let extensions = mls.extensions().clone();
            let (standalone, _) = mls
                .propose_group_context_extensions(&provider, extensions.clone(), &signer)
                .map_err(GroupError::Proposal)?;
            let (commit, _, _) = generate_prepared_commit(storage, mls, |mls, provider| {
                // Not by reference to the standalone proposal, which members
                // reject, or the commit fails as a missing proposal instead.
                let bundle = commit_without_group_info(mls, provider.storage(), |builder| {
                    builder
                        .consume_proposal_store(false)
                        .propose_group_context_extensions(extensions)
                        .unwrap()
                })
                .unwrap()
                .build(provider.rand(), provider.crypto(), &signer, |_| true)
                .unwrap()
                .stage_commit(provider)
                .unwrap();
                bundle.into_messages().0.tls_serialize_detached()
            })?;
            let mut far_forward = None;
            for _ in 0..FAR_FORWARD_PROPOSALS {
                let (message, _) = mls
                    .propose_self_update(&provider, &signer, LeafNodeParameters::default())
                    .unwrap();
                far_forward = Some(message.tls_serialize_detached()?);
            }
            mls.clear_pending_proposals(storage)?;
            Ok::<_, GroupError>(Continue([
                standalone.tls_serialize_detached()?,
                commit,
                far_forward.expect("at least one proposal"),
            ]))
        })
    })?
    .into_continued();
    let units = bo_group.prepare_group_messages(
        messages
            .iter()
            .map(|message| (message.as_slice(), false))
            .collect(),
    )?;
    let receipts = bo.context.api().send_group_messages(units).await?;
    let cursor = |index: usize| {
        xmtp_api_backend::envelope::metadata(
            &receipts[index],
            Topic::new_group_message(bo_group.group_id).kind(),
        )
        .unwrap()
        .1
    };
    let (commit_cursor, far_forward_cursor) = (cursor(1), cursor(2));

    alix_group.remove_members(&[bo.inbox_id()]).await?;
    alix_group
        .send_message(b"after the attack", SendMessageOpts::default())
        .await?;
    caro_group.sync().await?;

    let caro_messages = caro_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(
        caro_messages.last().unwrap().decrypted_message_bytes,
        b"after the attack"
    );
    assert!(
        !caro_group
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id())
    );
    let mut commit_logs = Vec::new();
    for (peer, group) in [(&alix, &alix_group), (&caro, &caro_group)] {
        let db = peer.context.db();
        let topic = StreamTopic::group(group.group_id);
        // The far-forward proposal is the last rejection; the removal and the
        // message after it applied, so every earlier head was resolved.
        let rejection = db.read_last_rejection(&topic)?.unwrap();
        assert_eq!(rejection.sequence_id, far_forward_cursor);
        assert_eq!(rejection.code, "far_forward_generation");
        assert!(db.topic_progress(&topic)?.processed > far_forward_cursor);
        let log = db
            .get_group_logs(&group.group_id)?
            .into_iter()
            .find(|log| log.commit_sequence_id == commit_cursor.0 as i64)
            .expect("the rejected commit is logged");
        assert_eq!(log.commit_result, CommitResult::Invalid);
        assert_eq!(
            log.last_epoch_authenticator,
            log.applied_epoch_authenticator
        );
        commit_logs.push((log.commit_result, log.applied_epoch_authenticator));
        assert!(!group.debug_info().await?.maybe_forked);
    }
    assert_eq!(commit_logs[0], commit_logs[1]);
}
