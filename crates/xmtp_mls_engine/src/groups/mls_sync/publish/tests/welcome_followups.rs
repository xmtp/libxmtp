//! Publish tests: welcome followups.

use super::*;

// verifies: SEND-006, SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_followup_retries_exact_bytes_after_restart() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    group.publish_intents().await?;
    assert!(!group.receive().await?.is_errored());
    let prepared = group.prepare_required_welcomes(intent.id)?.unwrap();
    let welcomes = prepared.welcomes.as_ref().unwrap();
    group
        .context
        .api()
        .publish_units(
            welcomes
                .units(group.context.api().limits())?
                .into_iter()
                .collect::<Option<_>>()?,
        )
        .await?;

    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    let restarted = alix.from_snapshot(&snapshot, |_| {}).await;
    drop(alix);
    let group = restarted.group(&group_id)?;
    let before: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(before.state, IntentState::Committed);
    let summary = group.sync_until_last_intent_resolved().await?;
    assert!(!summary.is_errored());

    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Processed);
    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    let after = after.welcomes.unwrap();
    assert_eq!(after.envelopes, welcomes.envelopes);
    assert_eq!(
        after.receipts.as_ref().unwrap().len(),
        after.envelopes.len()
    );
    let received = group
        .context
        .api()
        .query_welcome_messages(bo.installation_public_key())
        .await?;
    assert_eq!(received.len(), 1, "retry created a second Welcome");
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_followup_requires_ordered_commit_cursor() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.storage()
            .db()
            .set_group_intent_committed(intent.id, Cursor(0))?;
        Ok::<_, GroupError>(Continue(()))
    })?;
    assert!(matches!(
        group.prepare_required_welcomes(intent.id),
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::InvalidPreparedAttempt
        ))
    ));
    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert!(after.welcomes.is_none());
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Committed);
}

// verifies: GMOD-018
#[xmtp_common::test(unwrap_try = true)]
async fn pending_adds_welcome_every_new_leaf_after_commit_readback() {
    use crate::groups::intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData};
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes().await?;
        bo.group(&group.group_id)
    })
    .await?;
    bo_group.receive().await?;
    let bo2 = bo.new_installation().await;
    let caro = EngineTestPeer::new().await;
    let caro2 = caro.new_installation().await;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![bo.inbox_id().into(), caro.inbox_id().into()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    let pending = group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?;
    assert!(pending >= 3);
    xmtp_common::wait_for_some(|| async {
        bo_group.receive().await.ok()?;
        (bo_group
            .with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))
            .ok()?
            == pending)
            .then_some(())
    })
    .await?;
    let intent = QueueIntent::commit_pending_proposals()
        .data(Vec::<u8>::from(CommitPendingProposalsIntentData::default()))
        .queue(&bo_group)?;
    prepare_kind(&bo_group, IntentKind::CommitPendingProposals).await?;
    bo_group.publish_intents().await?;
    let published: StoredGroupIntent = bo.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(published.state, IntentState::Published);
    assert!(bo_group.prepare_required_welcomes(intent.id)?.is_none());
    for joiner in [&bo2, &caro, &caro2] {
        assert!(
            bo.context
                .api()
                .query_welcome_messages(joiner.installation_public_key())
                .await?
                .is_empty()
        );
    }
    bo_group.sync_until_intent_resolved(intent.id).await?;
    let committed: StoredGroupIntent = bo.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(committed.state, IntentState::Processed);
    let cursor = Cursor(committed.sequence_id.unwrap() as u64);
    assert!(cursor.0 > 0);
    assert!(
        bo.context
            .db()
            .topic_progress(&StreamTopic::group(group.group_id))?
            .processed
            >= cursor
    );
    for joiner in [&bo2, &caro, &caro2] {
        let joined = xmtp_common::wait_for_ok(|| async {
            joiner.sync_welcomes().await?;
            joiner.group(&group.group_id)
        })
        .await?;
        assert_eq!(
            joiner
                .context
                .db()
                .topic_progress(&StreamTopic::group(group.group_id))?
                .processed,
            cursor
        );
        assert_eq!(
            joined.epoch_authenticator().await?,
            bo_group.epoch_authenticator().await?
        );
        assert_eq!(joined.members().await?.len(), 3);
        assert_eq!(
            joined.with_group_snapshot(|mls| Ok(mls.members().count()))?,
            5
        );
    }
}
