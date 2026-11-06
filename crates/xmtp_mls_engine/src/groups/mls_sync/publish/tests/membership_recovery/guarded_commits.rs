//! Membership recovery publication tests: guarded commits.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn repeated_membership_proposal_reuses_accepted_references()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .inspect_err(|error| {
            if let GroupError::Sync(summary) = error {
                tracing::error!(
                    publish_errors = ?summary.publish_errors,
                    process_errors = ?summary.process.errored,
                    "initial group membership setup failed"
                );
            }
        })?;
    let data = Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
        vec![caro.inbox_id().to_string()],
        vec![bo.inbox_id().to_string()],
    ))?;
    let first = QueueIntent::propose_member_update()
        .data(data.clone())
        .queue(&group)?;
    group.sync_until_intent_resolved(first.id).await?;
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let crypto_before = group
        .context
        .db()
        .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?;
    let repeated = QueueIntent::propose_member_update()
        .data(data)
        .queue(&group)?;
    group.sync_until_intent_resolved(repeated.id).await?;
    let completed: StoredGroupIntent = group.context.db().fetch(&repeated.id)?.unwrap();
    assert_eq!(completed.state, IntentState::Processed);
    assert!(
        group
            .context
            .db()
            .prepared_envelopes(repeated.id)?
            .is_none()
    );
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    assert_eq!(
        crypto_before,
        group
            .context
            .db()
            .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?
    );

    let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(commit.id).await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    assert_eq!(
        group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    let members = group.members().await?;
    assert!(
        members
            .iter()
            .any(|member| member.inbox_id == caro.inbox_id())
    );
    assert!(
        !members
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id())
    );
    group
        .send_message(b"after repeated proposal", SendMessageOpts::default())
        .await?;
    Ok(())
}

#[rstest::rstest]
#[case::metadata(IntentKind::MetadataUpdate, true, false)]
#[case::key_update_current(IntentKind::KeyUpdate, false, false)]
#[case::key_update_rotated(IntentKind::KeyUpdate, true, false)]
#[case::own_add(IntentKind::UpdateGroupMembership, true, false)]
#[case::own_add_remove(IntentKind::UpdateGroupMembership, true, true)]
#[xmtp_common::test(unwrap_try = true)]
async fn guarded_noop_rolls_back_replacement_keys_and_commit_welcomes_selected_adds(
    #[case] kind: IntentKind,
    #[case] rotate: bool,
    #[case] remove: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            if remove {
                vec![bo.inbox_id().to_string()]
            } else {
                vec![]
            },
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    let original_package = group.with_group_snapshot(|mls| {
        mls.pending_proposals()
            .find_map(|proposal| match proposal.proposal() {
                Proposal::Add(add) => Some(add.key_package().clone()),
                _ => None,
            })
            .ok_or(GroupError::UninitializedResult)
    })?;
    let current_package = if rotate {
        caro.rotate_and_upload_key_package().await?;
        wait_for_rotated_package(&group, caro.context.installation_id(), &original_package).await
    } else {
        original_package
    };
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let crypto_before = group
        .context
        .db()
        .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?;
    let mut data = UpdateMetadataIntentData::new_update_group_name("obsolete guarded name".into());
    data.expected_field_value = Some("a name that was never set".into());
    let intent = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(data))
        .queue(&group)?;
    let requirements =
        group.with_group_snapshot(|mls| PublishRequirements::capture(mls, &intent))?;
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    assert!(
        group
            .prepare_publish_attempt(&requirements, &mut dependencies)?
            .is_none()
    );
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    assert_eq!(
        crypto_before,
        group
            .context
            .db()
            .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?
    );
    assert_eq!(
        group
            .context
            .db()
            .fetch(&intent.id)?
            .map(|intent: StoredGroupIntent| intent.state),
        Some(IntentState::Superseded)
    );
    assert!(group.context.db().prepared_envelopes(intent.id)?.is_none());

    let intent = match kind {
        IntentKind::MetadataUpdate => QueueIntent::metadata_update()
            .data(Vec::<u8>::from(
                UpdateMetadataIntentData::new_update_group_name("selected add and metadata".into()),
            ))
            .queue(&group)?,
        IntentKind::KeyUpdate => QueueIntent::key_update().queue(&group)?,
        IntentKind::UpdateGroupMembership => QueueIntent::update_group_membership()
            .data(
                group
                    .get_membership_update_intent(
                        &[caro.inbox_id()],
                        &if remove { vec![bo.inbox_id()] } else { vec![] },
                    )
                    .await?,
            )
            .queue(&group)?,
        _ => unreachable!(),
    };
    prepare_kind(&group, kind).await?;
    let prepared_intent: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    let staged = decode_staged_commit(prepared_intent.staged_commit.as_deref().unwrap())?;
    let selected: Vec<_> = staged.add_proposals().collect();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].add_proposal().key_package(), &current_package);
    let action = PostCommitAction::from_bytes(
        prepared_intent
            .post_commit_data
            .as_deref()
            .expect("selected Add needs a Welcome action"),
    )?;
    let PostCommitAction::SendWelcomes(action) = action;
    assert_eq!(action.installations.len(), 1);
    assert_eq!(
        action.installations[0].installation_key,
        caro.context.installation_id()
    );
    group.sync_until_intent_resolved(intent.id).await?;
    let completed: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(completed.state, IntentState::Processed);
    let attempt =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    let welcomes = attempt
        .welcomes
        .expect("accepted Add needs a saved Welcome batch");
    assert!(!welcomes.envelopes.is_empty());
    assert_eq!(
        welcomes.receipts.as_ref().unwrap().len(),
        welcomes.envelopes.len()
    );
    let welcome_topic = Topic::new_welcome_message(caro.installation_public_key());
    let receipts = welcomes
        .receipts
        .as_ref()
        .unwrap()
        .iter()
        .map(|bytes| xmtp_proto::backend_v1::EnvelopeMeta::decode(bytes.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;
    let target = receipts
        .into_iter()
        .filter(|receipt| {
            receipt.topic.as_ref().is_some_and(|topic| {
                topic.topic.as_slice() == AsRef::<[u8]>::as_ref(&welcome_topic)
            })
        })
        .map(|receipt| Cursor::from(receipt.cursor.expect("Welcome receipt needs a cursor")))
        .max()
        .expect("saved receipts must include Caro's Welcome topic");
    // QueryNewest can lag a publish receipt. Use that receipt as the fixed target,
    // including when the batch also contains a pointee on another topic.
    let barrier = crate::subscriptions::barrier::wait_through(
        &caro.context,
        [(welcome_topic, target)].into(),
        None,
    )
    .await;
    let db_topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: caro.installation_public_key().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let progress = caro.context.db().topic_progress(&db_topic)?;
    let rejection = caro.context.db().read_last_rejection(&db_topic)?;
    assert!(
        barrier.is_ok(),
        "Welcome target={target:?}, progress={progress:?}, rejection={rejection:?}, barrier={barrier:?}"
    );
    let caro_group = caro.group(&group.group_id);
    assert!(
        caro_group.is_ok(),
        "Welcome target={target:?}, progress={progress:?}, rejection={rejection:?}, group_error={:?}",
        caro_group.as_ref().err()
    );
    let caro_group = caro_group?;
    let received = group
        .context
        .api()
        .query_welcome_messages(caro.installation_public_key())
        .await?;
    assert_eq!(
        received.len(),
        1,
        "committed Welcome must be readable from the backend"
    );
    if kind == IntentKind::MetadataUpdate {
        assert_eq!(caro_group.group_name()?, "selected add and metadata");
    }
    assert_eq!(
        group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    Ok(())
}

#[rstest::rstest]
#[case::current_key(false, false, false)]
#[case::rotated_key(true, false, false)]
#[case::missing_key(false, true, false)]
#[case::old_and_current_refs(true, false, true)]
#[xmtp_common::test(unwrap_try = true)]
async fn non_admin_commits_valid_subset_without_resigning_admin_add(
    #[case] rotate: bool,
    #[case] missing: bool,
    #[case] fresh_reference: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::group_permissions::PreconfiguredPolicies;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let erin = EngineTestPeer::new().await;
    let group = alix.create_group(
        Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
        None,
    )?;
    group.add_members(&[bo.inbox_id(), dave.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.receive().await?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![dave.inbox_id().to_string()],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    let original_package = bo_group.with_group_snapshot(|mls_group| {
        mls_group
            .pending_proposals()
            .find_map(|proposal| match proposal.proposal() {
                Proposal::Add(add)
                    if add.key_package().leaf_node().signature_key().as_slice()
                        == caro.context.installation_id().as_slice() =>
                {
                    Some(add.key_package().clone())
                }
                _ => None,
            })
            .ok_or(GroupError::UninitializedResult)
    })?;
    let current_package = if rotate {
        caro.rotate_and_upload_key_package().await?;
        wait_for_rotated_package(&group, caro.context.installation_id(), &original_package).await
    } else {
        original_package.clone()
    };
    if fresh_reference {
        let proposal = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                vec![caro.inbox_id().to_string()],
                vec![],
            ))?)
            .queue(&group)?;
        group.sync_until_intent_resolved(proposal.id).await?;
        receive_completed_prefix(&group, &bo_group).await?;
    }
    bo_group.with_group_snapshot(|mls_group| {
        let packages: Vec<_> = mls_group
            .pending_proposals()
            .filter_map(|proposal| match proposal.proposal() {
                Proposal::Add(add) => Some(add.key_package()),
                _ => None,
            })
            .collect();
        assert!(packages.contains(&&original_package));
        assert_eq!(
            packages.contains(&&current_package),
            !rotate || fresh_reference,
            "the accepted Add refs must match the intended package generation"
        );
        Ok(())
    })?;
    if missing {
        xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![caro.context.installation_id().to_vec()]),
        );
    }
    let omitted = (rotate || missing) && !fresh_reference;
    if omitted {
        // This independent signed Add must survive omission of Caro's obsolete
        // Add and the whole Caro/Dave membership delta.
        let independent = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                vec![erin.inbox_id().to_string()],
                vec![],
            ))?)
            .queue(&group)?;
        group.sync_until_intent_resolved(independent.id).await?;
        receive_completed_prefix(&group, &bo_group).await?;
    }
    let before = bo_group.with_group_snapshot(PreparedBase::capture)?;
    if rotate && !fresh_reference {
        let own_request = QueueIntent::update_group_membership()
            .data(
                bo_group
                    .get_membership_update_intent(&[caro.inbox_id()], &[])
                    .await?,
            )
            .queue(&bo_group)?;
        let error = bo_group.publish_intents().await.unwrap_err();
        assert!(
            matches!(
                error,
                GroupError::CommitValidation(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions
                ))
            ),
            "{error:?}"
        );
        let failed: StoredGroupIntent = bo_group.context.db().fetch(&own_request.id)?.unwrap();
        assert_eq!(failed.state, IntentState::Error);
        assert_eq!(before, bo_group.with_group_snapshot(PreparedBase::capture)?);
    }
    let intent = QueueIntent::commit_pending_proposals()
        .data(Vec::<u8>::from(CommitPendingProposalsIntentData::new()))
        .queue(&bo_group)?;
    let (_, attempt) = prepare_kind(&bo_group, IntentKind::CommitPendingProposals).await?;
    if missing {
        xmtp_mls_identity::identity_lifecycle::test_mocks::set_test_mode_upload_malformed_keypackage(
            false, None,
        );
    }
    assert!(
        attempt.proposals.is_empty(),
        "non-admin must not sign a replacement Add"
    );
    assert_eq!(before, bo_group.with_group_snapshot(PreparedBase::capture)?);
    let stored: StoredGroupIntent = bo_group.context.db().fetch(&intent.id)?.unwrap();
    let staged = decode_staged_commit(stored.staged_commit.as_ref().unwrap())?;
    assert_eq!(staged.add_proposals().count(), 1);
    assert_eq!(staged.remove_proposals().count(), usize::from(!omitted));
    bo_group.publish_intents().await?;
    bo_group.sync_until_intent_resolved(intent.id).await?;
    // The sampled network head can lag Bo's completed commit. Compare both
    // members only after Alix has processed the same known prefix.
    let target = bo_group
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
            group.group_id,
        ))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &group.context,
        [(Topic::new_group_message(group.group_id), target)].into(),
        None,
    )
    .await
    .expect("Alix must process Bo's completed membership commit");
    let members = group.members().await?;
    assert_eq!(members.len(), if omitted { 4 } else { 3 });
    assert_eq!(
        members
            .iter()
            .any(|member| member.inbox_id == caro.inbox_id()),
        !omitted
    );
    assert_eq!(
        members
            .iter()
            .any(|member| member.inbox_id == dave.inbox_id()),
        omitted
    );
    assert_eq!(
        members
            .iter()
            .any(|member| member.inbox_id == erin.inbox_id()),
        omitted,
        "unrelated valid pending membership work must be committed"
    );
    assert_eq!(
        group.epoch_authenticator().await?,
        bo_group.epoch_authenticator().await?
    );
    bo_group
        .send_message(
            b"after admin proposal selection",
            SendMessageOpts::default(),
        )
        .await?;
    group.receive().await?;
    if omitted {
        erin.sync_welcomes().await?;
        let erin_group = erin.group(&group.group_id)?;
        erin_group
            .send_message(
                b"independent pending Add joined",
                SendMessageOpts::default(),
            )
            .await?;
        receive_completed_prefix(&erin_group, &group).await?;
    }
    if omitted {
        // The original author can submit a new authorized request in the new epoch.
        group.add_members(&[caro.inbox_id()]).await?;
    }
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    caro_group
        .send_message(b"joined with current key", SendMessageOpts::default())
        .await?;
    group.receive().await?;
    bo_group.receive().await?;
    assert_eq!(
        group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    Ok(())
}
