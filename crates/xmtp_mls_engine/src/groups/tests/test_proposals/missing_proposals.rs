//! Proposal tests: missing proposals.

use super::*;

// =============================================================================
// Proposal Permission Validation Tests
// =============================================================================

/// A commit whose proposal the local store lacks is rejected only when the
/// complete prefix never delivered that proposal. The same reference, delivered
/// and then lost from local state, is held without advancing `P`, and an
/// independent topic still completes. A local-state loss must not become a
/// permanent skip of a commit every other member applies.
// verifies: PROC-011, PROC-012
#[xmtp_common::test(unwrap_try = true)]
async fn missing_proposal_absent_vs_lost() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dan = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = super::super::receive_group_invite(&bo).await;
    let caro_group = super::super::receive_group_invite(&caro).await;
    bo_group.receive().await?;
    caro_group.receive().await?;
    let topic = StreamTopic::group(bo_group.group_id);
    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let bo_before = bo.context.db().topic_progress(&topic)?.received;
    let caro_before = caro.context.db().topic_progress(&topic)?.received;
    assert_eq!(bo_before, caro_before);

    // A membership update is proposals by reference, then their commit.
    let epoch = alix_group.epoch().await? as i64;
    alix_group.add_members(&[dan.inbox_id()]).await?;
    assert!(
        alix.context
            .db()
            .received_proposals(alix_group.group_id, epoch)?
            .is_empty(),
        "a merged commit clears the evidence of the proposals it consumed"
    );
    let mut envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), bo_before)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let commit = envelopes.pop().expect("the commit is on the network");
    assert!(!envelopes.is_empty(), "the commit has proposal envelopes");
    let commit_cursor = xmtp_proto::types::Cursor(
        commit
            .meta
            .as_ref()
            .and_then(|meta| meta.cursor.as_ref())
            .expect("the backend supplies a cursor")
            .sequence_id,
    );
    let limits = bo
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(topic.kind);

    // Bo receives the proposals, then local state loses them.
    let mut prefix = envelopes.clone();
    prefix.push(commit.clone());
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic.clone(),
            after: bo_before,
            envelopes: prefix.into_iter().map(Into::into).collect(),
        },
        limits,
    )?;
    for _ in &envelopes {
        assert!(matches!(
            bo_group.process_pending_group_head(None)?,
            GroupHeadOutcome::Progress { result: Ok(_), .. }
        ));
    }
    let proposals_handled = bo.context.db().topic_progress(&topic)?.processed;
    xmtp_mls_core::state_tx::state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let references: Vec<_> = mls
                .pending_proposals()
                .map(|proposal| proposal.proposal_reference_ref().clone())
                .collect();
            assert!(!references.is_empty());
            for reference in references {
                mls.remove_pending_proposal(storage, &reference).unwrap();
            }
            Ok::<_, crate::groups::GroupError>(Continue(()))
        })
    })?;
    let GroupHeadOutcome::Waiting {
        cursor,
        code,
        blocked,
    } = bo_group.process_pending_group_head(None)?
    else {
        panic!("a commit whose delivered proposal was lost must be held");
    };
    assert_eq!(cursor, commit_cursor);
    assert_eq!(code, "lost_proposal_state");
    assert!(blocked);
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        proposals_handled
    );
    assert_eq!(
        bo.context
            .db()
            .first_pending_envelope(&topic)?
            .map(|head| head.sequence_id as u64),
        Some(commit_cursor.0)
    );
    assert!(bo.context.db().read_last_rejection(&topic)?.is_none());

    // Caro's complete prefix never carried the proposals: the same reference is terminal.
    caro.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: caro_before,
            envelopes: vec![commit.into()],
        },
        limits,
    )?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = caro_group.process_pending_group_head(None)?
    else {
        panic!("a commit naming an absent proposal must be rejected");
    };
    assert_eq!(cursor, commit_cursor);
    assert!(error.is_missing_proposal(), "{error:?}");
    assert_eq!(
        caro.context.db().topic_progress(&topic)?.processed,
        commit_cursor
    );
    assert_eq!(
        caro.context
            .db()
            .read_last_rejection(&topic)?
            .map(|rejection| rejection.sequence_id),
        Some(commit_cursor)
    );

    // The held head does not stop Bo's other topics.
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    dm.send_message(b"independent", SendMessageOpts::default())
        .await?;
    bo.sync_welcomes().await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.sync().await?;
    assert!(
        bo_dm
            .find_messages(&Default::default())?
            .iter()
            .any(|message| message.decrypted_message_bytes == b"independent")
    );
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        proposals_handled
    );
}
