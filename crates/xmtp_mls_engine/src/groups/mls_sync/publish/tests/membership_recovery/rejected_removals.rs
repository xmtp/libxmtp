//! Membership recovery publication tests: rejected removals.

use super::*;

#[rstest::rstest]
#[case::live(false, false)]
#[case::reopened(true, false)]
#[case::concurrent_adds(false, true)]
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_removal_with_new_installation_does_not_block_publication(
    #[case] reopen: bool,
    #[case] concurrent_adds: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::persistent().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), eve.inbox_id()],
            None,
            None,
        )
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
    bo_group.receive().await?;
    caro_group.receive().await?;
    let committed_epoch = group.epoch().await?;
    let committed_authenticator = group.epoch_authenticator().await?;

    // The removal also discovers a valid new installation for another member.
    // Publish and process this attempt before any later installation refresh.
    let bo2 = bo.new_installation().await;
    // Caro is not an admin, so receivers reject the removal.
    let result = caro_group.remove_members(&[eve.inbox_id()]).await;
    let Err(GroupError::Sync(summary)) = result else {
        panic!("expected a permission rejection, got {result:?}");
    };
    assert!(summary.process.errored.iter().any(|(_, error)| matches!(
        error,
        GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
            CommitRuleError::InsufficientPermissions
        ))
    )));
    // Refresh leaf changes are inline. Seed the accepted standalone Add
    // explicitly so this test still exercises pending-proposal recovery.
    publish_installation_add(
        &caro_group,
        bo2.context.installation_id().as_slice(),
        &[&group, &bo_group, &caro_group],
    )
    .await?;
    for peer in [&group, &bo_group, &caro_group] {
        receive_completed_prefix(&caro_group, peer).await?;
        assert_eq!(peer.members().await?.len(), 4);
        assert_eq!(peer.epoch().await?, committed_epoch);
        assert_eq!(peer.epoch_authenticator().await?, committed_authenticator);
        peer.with_group_snapshot(|mls_group| {
            let proposals: Vec<_> = mls_group.pending_proposals().collect();
            assert_eq!(proposals.len(), 1, "only the valid Add must remain");
            let Proposal::Add(add) = proposals[0].proposal() else {
                panic!("expected the new installation Add");
            };
            assert_eq!(
                add.key_package().leaf_node().signature_key().as_slice(),
                bo2.context.installation_id().as_slice()
            );
            assert!(
                !mls_group
                    .members()
                    .any(|member| member.signature_key == bo2.context.installation_id().as_slice())
            );
            Ok(())
        })?;
    }

    let retained_package = group.with_group_snapshot(|mls_group| {
        mls_group
            .pending_proposals()
            .find_map(|proposal| match proposal.proposal() {
                Proposal::Add(add)
                    if add.key_package().leaf_node().signature_key().as_slice()
                        == bo2.context.installation_id().as_slice() =>
                {
                    Some(add.key_package().clone())
                }
                _ => None,
            })
            .ok_or(GroupError::UninitializedResult)
    })?;
    let retired_hash = xmtp_mls_core::identity::serialize_key_package_hash_ref(
        &retained_package,
        &XmtpOpenMlsProviderRef::new(bo2.context.mls_storage()),
    )?;
    let retired = bo2
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(retired_hash)?;

    // verifies: JOIN-074
    // Model completed retirement. The old pending Add remains,
    // but the recipient can only open a Welcome for the current package.
    bo2.rotate_and_upload_key_package().await?;
    xmtp_mls_identity::key_package_maintenance::delete_key_package(
        &bo2.context,
        retired.key_package_hash_ref,
        retired.post_quantum_public_key,
    )?;
    let current_package =
        wait_for_rotated_package(&group, bo2.context.installation_id(), &retained_package).await;
    assert_ne!(retained_package, current_package);
    if concurrent_adds {
        for inbox in [bo.inbox_id(), dave.inbox_id()] {
            let proposal = QueueIntent::propose_member_update()
                .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                    vec![inbox.to_string()],
                    vec![],
                ))?)
                .queue(&bo_group)?;
            bo_group.sync_until_intent_resolved(proposal.id).await?;
        }
        for peer in [&group, &caro_group] {
            receive_completed_prefix(&bo_group, peer).await?;
        }
        group.with_group_snapshot(|mls_group| {
            let adds: Vec<_> = mls_group
                .pending_proposals()
                .filter_map(|proposal| match proposal.proposal() {
                    Proposal::Add(add) => {
                        Some(add.key_package().leaf_node().signature_key().as_slice())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                adds.iter()
                    .filter(|key| **key == bo2.context.installation_id().as_slice())
                    .count(),
                2
            );
            assert!(adds.contains(&dave.context.installation_id().as_slice()));
            Ok(())
        })?;
    }
    let pending_before_reopen = group.with_group_snapshot(PreparedBase::capture)?;
    let group = if reopen {
        use xmtp_db::StorageOption;

        assert!(
            xmtp_common::wait_for_some(|| async {
                alix.context
                    .incoming_runtime()
                    .coordinator
                    .lock()
                    .is_none()
                    .then_some(())
            })
            .await
            .is_some()
        );
        let database = match alix.context.store().opts() {
            StorageOption::Persistent(path) => path.clone(),
            StorageOption::Ephemeral => panic!("restart requires a persistent database"),
        };
        let group_id = group.group_id;
        let owner = alix.owner.clone();
        drop(group);
        drop(alix);
        let restarted = EngineTestPeer::open_persistent(
            owner,
            &database,
            xmtp_mls_identity::identity_lifecycle::IdentityStrategy::CachedOnly,
            |_| {},
        )
        .await;
        restarted.group(&group_id)?
    } else {
        group
    };
    let pending_before = group.with_group_snapshot(PreparedBase::capture)?;
    assert_eq!(
        pending_before, pending_before_reopen,
        "reopening must retain the exact accepted proposal references"
    );
    let refresh = QueueIntent::update_group_membership()
        .data(group.get_membership_update_intent(&[], &[]).await?)
        .queue(&group)?;
    if !reopen && !concurrent_adds {
        use xmtp_db::diesel::connection::SimpleConnection;
        let crypto_before = group
            .context
            .db()
            .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?;
        group.context.db().raw_query(|conn| conn.batch_execute(
            "CREATE TRIGGER fail_membership_preparation BEFORE UPDATE OF prepared_envelopes ON group_intents WHEN NEW.prepared_envelopes IS NOT NULL BEGIN SELECT RAISE(ABORT, 'injected preparation failure'); END;",
        ))?;
        let failed = prepare_kind(&group, IntentKind::UpdateGroupMembership).await;
        assert!(
            matches!(failed, Err(GroupError::Storage(_))),
            "the storage fault must fail preparation: {failed:?}"
        );
        assert_eq!(
            group.with_group_snapshot(PreparedBase::capture)?,
            pending_before
        );
        assert!(group.context.db().prepared_envelopes(refresh.id)?.is_none());
        assert_eq!(
            group
                .context
                .db()
                .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?,
            crypto_before,
            "failed preparation changed stored crypto state"
        );
        let intent: StoredGroupIntent = group.context.db().fetch(&refresh.id)?.unwrap();
        assert_eq!(intent.state, IntentState::ToPublish);
        group
            .context
            .db()
            .raw_query(|conn| conn.batch_execute("DROP TRIGGER fail_membership_preparation;"))?;
    }
    let (_, prepared) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    let prepared_intent: StoredGroupIntent = group.context.db().fetch(&refresh.id)?.unwrap();
    let staged = decode_staged_commit(prepared_intent.staged_commit.as_deref().unwrap())?;
    let selected = staged
        .add_proposals()
        .find(|proposal| {
            proposal
                .add_proposal()
                .key_package()
                .leaf_node()
                .signature_key()
                .as_slice()
                == bo2.context.installation_id().as_slice()
        })
        .unwrap();
    assert_eq!(selected.add_proposal().key_package(), &current_package);
    assert_eq!(
        group.with_group_snapshot(PreparedBase::capture)?,
        pending_before
    );
    // Model a lost publish response. The retry must keep its exact ciphertext,
    // including the selected Welcome key package.
    group
        .context
        .api()
        .send_group_messages(vec![prepared.publish_unit(group.context.api().limits())?])
        .await?;
    group.publish_intents().await?;
    let retried =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(refresh.id)?.unwrap())?;
    assert_eq!(retried.envelopes, prepared.envelopes);
    group.sync_until_intent_resolved(refresh.id).await?;

    let dave_group = if concurrent_adds {
        dave.sync_welcomes().await?;
        Some(dave.group(&group.group_id)?)
    } else {
        None
    };

    // Every member must be able to complete the valid refresh and send again.
    for (sender, body) in [
        (&group, b"after rejection alix".as_slice()),
        (&bo_group, b"after rejection bo"),
        (&caro_group, b"after rejection caro"),
    ] {
        sender
            .send_message(body, SendMessageOpts::default())
            .await?;
    }
    if let Some(dave_group) = &dave_group {
        dave_group
            .send_message(b"after rejection dave", SendMessageOpts::default())
            .await?;
    }
    bo2.sync_welcomes().await?;
    let bo2_group = bo2.group(&group.group_id)?;
    bo2_group
        .send_message(b"after rejection bo2", SendMessageOpts::default())
        .await?;
    let epoch = group.epoch().await?;
    group.add_members(&[bo.inbox_id()]).await?;
    assert_eq!(
        group.epoch().await?,
        epoch,
        "an existing installation must not be added again"
    );
    let target = bo2_group
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
            group.group_id,
        ))?
        .processed;
    for peer in [&group, &bo_group, &caro_group, &bo2_group]
        .into_iter()
        .chain(dave_group.as_ref())
    {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(Topic::new_group_message(group.group_id), target)].into(),
            None,
        )
        .await
        .expect("every member must process the last confirmed send");
        assert_eq!(
            peer.members().await?.len(),
            if concurrent_adds { 5 } else { 4 }
        );
        assert_eq!(
            peer.epoch_authenticator().await?,
            group.epoch_authenticator().await?
        );
        let messages = peer.find_messages(&MsgQueryArgs::default())?;
        let mut expected_messages = vec![
            b"after rejection alix".as_slice(),
            b"after rejection bo",
            b"after rejection caro",
            b"after rejection bo2",
        ];
        if concurrent_adds {
            expected_messages.push(b"after rejection dave");
        }
        for body in expected_messages {
            assert!(
                messages
                    .iter()
                    .any(|message| message.decrypted_message_bytes == body)
            );
        }
    }
    Ok(())
}

// verifies: GMOD-038
#[rstest::rstest]
#[case::new_installation_update(false)]
#[case::request_satisfied_by_competing_commit(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn own_membership_delta_and_pending_removal_use_the_same_selected_state(
    #[case] already_satisfied: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
    .expect("Bo must join the initial group");
    let caro_group = xmtp_common::wait_for_ok(|| async {
        caro.sync_welcomes()
            .await
            .and_then(|_| caro.group(&group.group_id))
    })
    .await
    .expect("Caro must join the initial group");
    let bo2 = bo.new_installation().await;
    let own_data = group.get_membership_update_intent(&[], &[]).await?;
    assert!(own_data.membership_updates.contains_key(bo.inbox_id()));
    if already_satisfied {
        // The request survives a competing commit that already applies its S2 value.
        caro_group.add_missing_installations().await?;
        receive_completed_prefix(&caro_group, &group).await?;
    }
    let removal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![],
            vec![bo.inbox_id().to_string()],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(removal.id).await?;
    group.with_group_snapshot(|mls| {
        assert!(
            mls.pending_proposals()
                .any(|proposal| { matches!(proposal.proposal(), Proposal::Remove(_)) })
        );
        assert!(
            mls.pending_proposals()
                .any(|proposal| { matches!(proposal.proposal(), Proposal::AppDataUpdate(_)) })
        );
        Ok(())
    })?;
    let own = QueueIntent::update_group_membership()
        .data(own_data)
        .queue(&group)?;
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    group.sync_until_intent_resolved(own.id).await?;
    receive_completed_prefix(&group, &caro_group).await?;
    assert_eq!(
        group
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id()),
        !already_satisfied,
        "only the actual own delta may supersede the pending removal"
    );
    assert_eq!(
        group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    if !already_satisfied {
        let attempt =
            PreparedAttempt::decode(&group.context.db().prepared_envelopes(own.id)?.unwrap())?;
        let welcome_topic = Topic::new_welcome_message(bo2.installation_public_key());
        let receipts = attempt.welcomes.unwrap().receipts.unwrap();
        let target = receipts
            .iter()
            .map(|bytes| xmtp_proto::backend_v1::EnvelopeMeta::decode(bytes.as_slice()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|receipt| {
                receipt.topic.as_ref().is_some_and(|topic| {
                    topic.topic.as_slice() == AsRef::<[u8]>::as_ref(&welcome_topic)
                })
            })
            .map(|receipt| Cursor::from(receipt.cursor.unwrap()))
            .max()
            .expect("the selected new installation needs a Welcome receipt");
        crate::subscriptions::barrier::wait_through(
            &bo2.context,
            [(welcome_topic, target)].into(),
            None,
        )
        .await?;
        let bo2_group = bo2.group(&group.group_id)?;
        receive_completed_prefix(&group, &bo_group).await?;
        assert_eq!(
            group.epoch_authenticator().await?,
            bo_group.epoch_authenticator().await?
        );
        assert_eq!(
            group.epoch_authenticator().await?,
            bo2_group.epoch_authenticator().await?
        );
        bo2_group
            .send_message(b"selected installation joined", SendMessageOpts::default())
            .await?;
        receive_completed_prefix(&bo2_group, &group).await?;
        receive_completed_prefix(&bo2_group, &caro_group).await?;
    }
    group
        .send_message(b"after own delta selection", SendMessageOpts::default())
        .await?;
    receive_completed_prefix(&group, &caro_group).await?;
    assert!(
        caro_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| { message.decrypted_message_bytes == b"after own delta selection" })
    );
    Ok(())
}
