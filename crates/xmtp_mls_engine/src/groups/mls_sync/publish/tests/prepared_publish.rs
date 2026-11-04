//! Publish tests: prepared publish.

use super::*;

// verifies: GMOD-036
#[rstest::rstest]
#[case::name_first(true)]
#[case::description_first(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn competing_metadata_attempts_from_one_epoch_preserve_both_updates(
    #[case] name_first: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_db::group_message::MsgQueryArgs;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id))
    })
    .await
    .expect("the published Welcome must become visible");
    let caro_group = xmtp_common::wait_for_ok(|| async {
        caro.sync_welcomes()
            .await
            .and_then(|_| caro.group(&group.group_id))
    })
    .await
    .expect("the published Welcome must become visible");
    bo_group.receive().await?;
    caro_group.receive().await?;
    let name = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(
            UpdateMetadataIntentData::new_update_group_name("competing name".into()),
        ))
        .queue(&group)?;
    let description = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(
            UpdateMetadataIntentData::new_update_group_description("competing description".into()),
        ))
        .queue(&bo_group)?;

    // Hold both durable attempts before either can reach backend order.
    let (_, first) = prepare_kind(&group, IntentKind::MetadataUpdate).await?;
    let (_, second) = prepare_kind(&bo_group, IntentKind::MetadataUpdate).await?;
    assert_eq!(first.base, second.base);
    assert_eq!(group.epoch().await?, first.base.epoch);
    assert_eq!(bo_group.epoch().await?, first.base.epoch);
    assert_ne!(first.envelopes, second.envelopes);
    if name_first {
        group.publish_intents().await?;
        bo_group.publish_intents().await?;
    } else {
        bo_group.publish_intents().await?;
        group.publish_intents().await?;
    }
    group.sync_until_intent_resolved(name.id).await?;
    bo_group.sync_until_intent_resolved(description.id).await?;
    let topic = Topic::new_group_message(group.group_id);
    let key = xmtp_db::incoming_envelope::StreamTopic::group(group.group_id);
    let target = group
        .context
        .db()
        .topic_progress(&key)?
        .processed
        .max(bo_group.context.db().topic_progress(&key)?.processed);
    for peer in [&group, &bo_group, &caro_group] {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), target)].into(),
            None,
        )
        .await
        .expect("all members must process both completed metadata changes");
        assert_eq!(peer.group_name()?, "competing name");
        assert_eq!(peer.group_description()?, "competing description");
        assert_eq!(peer.epoch().await?, first.base.epoch + 2);
        assert_eq!(
            peer.epoch_authenticator().await?,
            group.epoch_authenticator().await?
        );
    }
    for (sender, body) in [
        (&group, b"from alix".as_slice()),
        (&bo_group, b"from bo"),
        (&caro_group, b"from caro"),
    ] {
        sender
            .send_message(body, SendMessageOpts::default())
            .await?;
    }
    let target = caro_group.context.db().topic_progress(&key)?.processed;
    for peer in [&group, &bo_group, &caro_group] {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), target)].into(),
            None,
        )
        .await
        .expect("all members must process the last confirmed send");
        let messages = peer.find_messages(&MsgQueryArgs::default())?;
        for body in [b"from alix".as_slice(), b"from bo", b"from caro"] {
            assert_eq!(
                messages
                    .iter()
                    .filter(|message| message.decrypted_message_bytes == body)
                    .count(),
                1
            );
        }
    }
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn prepared_proposals_keep_wire_order_after_reload() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let data: Vec<u8> =
        ProposeMemberUpdateIntentData::new(vec![bo.inbox_id().to_string()], vec![]).try_into()?;
    QueueIntent::propose_member_update()
        .data(data)
        .queue(&group)?;
    let (_, attempt) = prepare_kind(&group, IntentKind::ProposeMemberUpdate).await?;
    assert_eq!(attempt.proposals.len(), 2);
    for (index, bytes) in attempt.envelopes.iter().enumerate() {
        let envelope = ClientEnvelope::decode(bytes.as_slice())?;
        let Some(Payload::GroupMessage(message)) = envelope.payload else {
            panic!("expected group message");
        };
        let proposal = attempt
            .proposal_for_payload(&sha256(&message.data))?
            .unwrap();
        match index {
            0 => assert!(matches!(proposal.proposal(), Proposal::Add(_))),
            1 => assert!(matches!(proposal.proposal(), Proposal::AppDataUpdate(_))),
            _ => panic!("unexpected proposal"),
        }
    }
    let pending = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, _| {
            Ok::<_, GroupError>(Continue(mls_group.pending_proposals().count()))
        })
    })?
    .into_continued();
    assert_eq!(pending, 0, "unreceived proposals entered committed state");
}

#[xmtp_common::test(unwrap_try = true)]
async fn lost_publish_reply_retries_exact_prepared_envelopes() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"lost publish reply", Default::default())?;
    let (intent, attempt) = prepare_message(&group).await?;
    assert_eq!(group.published_intent_target(intent.id)?, None);

    // The backend accepted the request, but the client did not save the reply.
    group
        .context
        .api()
        .send_group_messages(vec![attempt.publish_unit(group.context.api().limits())?])
        .await?;
    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    let restarted = alix.from_snapshot(&snapshot, |_| {}).await;
    drop(alix);
    let group = restarted.group(&group_id)?;
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    group.publish_intents().await?;
    let published_target = group.published_intent_target(intent.id)?.unwrap();

    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert_eq!(after.envelopes, PreparedAttempt::decode(&before)?.envelopes);
    assert!(after.receipts.is_some());
    let rows = group
        .context
        .api()
        .query_all(
            [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
            group.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let occurrences = rows
        .iter()
        .filter(|row| {
            row.envelope
                .as_ref()
                .is_some_and(|envelope| envelope.encode_to_vec() == attempt.envelopes[0])
        })
        .count();
    assert_eq!(occurrences, 1, "retry created another backend envelope");
    let accepted = rows
        .iter()
        .find(|row| {
            row.envelope
                .as_ref()
                .is_some_and(|envelope| envelope.encode_to_vec() == attempt.envelopes[0])
        })
        .unwrap();
    assert_eq!(
        published_target.0,
        accepted
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id
    );
}

// verifies: SEND-008
#[xmtp_common::test(unwrap_try = true)]
async fn late_publish_reply_cannot_update_replacement_attempt() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = group.send_message_optimistic(b"late publish reply", Default::default())?;
    let (old_intent, old_attempt) = prepare_message(&group).await?;
    let receipts = group
        .context
        .api()
        .send_group_messages(vec![
            old_attempt.publish_unit(group.context.api().limits())?,
        ])
        .await?;

    // Model ordered supersession while the first publish reply is delayed.
    xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.storage()
            .db()
            .set_group_intent_to_publish(old_intent.id)?;
        Ok::<_, GroupError>(Continue(()))
    })?;
    let (_, replacement) = prepare_message(&group).await?;
    assert_ne!(replacement.envelopes, old_attempt.envelopes);
    group.record_publish_receipts(&old_intent, &old_attempt, receipts)?;

    let current = PreparedAttempt::decode(
        &group
            .context
            .db()
            .prepared_envelopes(old_intent.id)?
            .unwrap(),
    )?;
    assert_eq!(current, replacement);
    let message: StoredGroupMessage = group.context.db().fetch(&message_id)?.unwrap();
    assert!(message.envelope_hash.is_none());
    assert!(message.expiry_ns.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn preparation_rejects_changed_proposal_refs_at_the_same_epoch()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![bo.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    let intent = QueueIntent::commit_pending_proposals().queue(&group)?;
    let requirements = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let intent = Fetch::<StoredGroupIntent>::fetch(&storage.db(), &intent.id)?
                .ok_or(GroupError::UninitializedResult)?;
            PublishRequirements::capture(mls, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let original = group.with_group_snapshot(PreparedBase::capture)?;
    let reference = group.with_group_snapshot(|mls| {
        let pending = mls.pending_proposals().collect::<Vec<_>>();
        assert!(pending.len() >= 2);
        Ok(pending[0].proposal_reference_ref().clone())
    })?;
    xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            mls.remove_pending_proposal(storage, &reference).unwrap();
            Ok::<_, GroupError>(Continue(()))
        })
    })?;
    let changed = group.with_group_snapshot(PreparedBase::capture)?;
    assert_eq!(changed.epoch, original.epoch);
    assert_eq!(changed.authenticator, original.authenticator);
    assert_ne!(changed, original);
    assert!(matches!(
        group.prepare_publish_attempt(&requirements, &mut dependencies),
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    assert_eq!(group.with_group_snapshot(PreparedBase::capture)?, changed);
    assert!(group.context.db().prepared_envelopes(intent.id)?.is_none());
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn preparation_fences_a_second_snapshot_of_the_same_intent() {
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
    let requirements = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let intent = storage
                .db()
                .find_group_intents(
                    group.group_id,
                    Some(vec![IntentState::ToPublish]),
                    Some(vec![IntentKind::UpdateGroupMembership]),
                )?
                .remove(0);
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut first_dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let mut second_dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let first = group
        .prepare_publish_attempt(&requirements, &mut first_dependencies)?
        .unwrap();
    let second = group.prepare_publish_attempt(&requirements, &mut second_dependencies);
    assert!(matches!(
        second,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    let current = PreparedAttempt::decode(
        &group
            .context
            .db()
            .prepared_envelopes(requirements.intent.id)?
            .unwrap(),
    )?;
    assert_eq!(current, first);

    let later = QueueIntent::update_group_membership()
        .data(requirements.intent.data.as_slice().to_vec())
        .queue(&group)?;
    assert!(matches!(
        prepare_kind(&group, IntentKind::UpdateGroupMembership).await,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    assert!(group.context.db().prepared_envelopes(later.id)?.is_none());
    assert_eq!(
        group
            .context
            .db()
            .find_group_intents(
                group.group_id,
                Some(vec![IntentState::Published]),
                Some(vec![IntentKind::UpdateGroupMembership]),
            )?
            .len(),
        1
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_completes_after_queued_state_changes() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let peer = bo.group(&group.group_id)?;
    let queued_changes = MAX_GROUP_SYNC_RETRIES + 1;
    for index in 0..queued_changes {
        QueueIntent::metadata_update()
            .data(Vec::<u8>::from(
                UpdateMetadataIntentData::new_update_group_name(format!("queued {index}")),
            ))
            .queue(&group)?;
    }

    group
        .send_message(b"after queued changes", Default::default())
        .await?;

    peer.sync().await?;
    let expected_name = format!("queued {}", queued_changes - 1);
    assert_eq!(group.group_name()?, expected_name);
    assert_eq!(peer.group_name()?, expected_name);
    assert_eq!(
        group.epoch_authenticator().await?,
        peer.epoch_authenticator().await?
    );
    let messages = peer.find_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after queued changes")
            .count(),
        1
    );
}
