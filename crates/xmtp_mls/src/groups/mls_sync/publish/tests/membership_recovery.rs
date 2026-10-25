//! Publication after rejected membership changes.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_db::{
    ConnectionExt, group_message::MsgQueryArgs, key_package_history::QueryKeyPackageHistory,
};
use xmtp_mls_validation::commit::CommitRuleError;

// Receive the prefix that the sender processed, including rejected commits.
pub(super) async fn receive_completed_prefix(
    sender: &crate::utils::TestMlsGroup,
    receiver: &crate::utils::TestMlsGroup,
) -> Result<(), GroupError> {
    let target = sender
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
            sender.group_id,
        ))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &receiver.context,
        [(Topic::new_group_message(sender.group_id), target)].into(),
        None,
    )
    .await
    .expect("receiver must process the sender's completed prefix");
    Ok(())
}

// A publication receipt can precede QueryNewest visibility.
async fn wait_for_rotated_package(
    group: &crate::utils::TestMlsGroup,
    installation: xmtp_proto::types::InstallationId,
    previous: &KeyPackage,
) -> KeyPackage {
    xmtp_common::wait_for_some(|| async {
        let fetched = group
            .context
            .api()
            .fetch_key_packages(&[installation])
            .await
            .expect("fetch the rotated key package");
        let package = fetched.get(&installation)?.as_ref()?;
        let current = xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(
            &openmls_rust_crypto::RustCrypto::default(),
            &package.key_package_tls_serialized,
        )
        .expect("verify the rotated key package")
        .inner;
        (&current != previous).then_some(current)
    })
    .await
    .expect("the rotated key package must become visible")
}

#[rstest::rstest]
#[case::only_requested_add(false)]
#[case::independent_pending_add(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn requested_add_cannot_succeed_after_its_last_package_becomes_unusable(
    #[case] independent_add: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dave, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    for inbox in std::iter::once(caro.inbox_id()).chain(independent_add.then_some(dave.inbox_id()))
    {
        let proposal = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
                vec![inbox.to_string()],
                vec![],
            ))?)
            .queue(&group)?;
        group.sync_until_intent_resolved(proposal.id).await?;
    }
    // The caller's initial fetch succeeds. The publication fetch then loses
    // the only usable package for the requested inbox.
    let request = QueueIntent::update_group_membership()
        .data(
            group
                .get_membership_update_intent(&[caro.inbox_id()], &[])
                .await?,
        )
        .queue(&group)?;
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let crypto_before = group
        .context
        .db()
        .raw_query(xmtp_db::sql_key_store::OpenMlsKeyValue::hash_all)?;
    crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![caro.installation_id.to_vec()]),
    );
    let result = group.publish_intents().await;
    crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(false, None);
    assert!(
        matches!(result, Err(GroupError::InvalidPublicKeys(_))),
        "{result:?}"
    );
    let rejected: StoredGroupIntent = group.context.db().fetch(&request.id)?.unwrap();
    assert_eq!(rejected.state, IntentState::Error);
    assert!(group.context.db().prepared_envelopes(request.id)?.is_none());
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
    let members = group.members().await?;
    assert!(
        members
            .iter()
            .any(|member| member.inbox_id == caro.inbox_id())
    );
    assert_eq!(
        members
            .iter()
            .any(|member| member.inbox_id == dave.inbox_id()),
        independent_add
    );
    group
        .send_message(
            b"after rejected unusable request",
            SendMessageOpts::default(),
        )
        .await?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn repeated_membership_proposal_reuses_accepted_references() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await
        .inspect_err(|error| {
            if let crate::client::ClientError::Group(error) = error
                && let GroupError::Sync(summary) = error.as_ref()
            {
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
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
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
        wait_for_rotated_package(&group, caro.installation_id, &original_package).await
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
        caro.installation_id
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
#[case::membership_refresh(true)]
#[case::generic_commit(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn revoked_pending_add_does_not_block_publication(
    #[case] refresh_membership: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(eve, disable_workers);
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), eve.inbox_id()],
            None,
            None,
        )
        .await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    caro_group.receive().await?;
    tester!(bo2, from: bo, disable_workers);
    // Refresh leaf changes are inline. Seed the accepted standalone Add
    // explicitly so this test still exercises pending-proposal recovery.
    publish_installation_add(
        &caro_group,
        bo2.installation_id.as_slice(),
        &[&group, &caro_group],
    )
    .await?;
    // Caro is not an admin, so receivers reject the removal.
    assert!(caro_group.remove_members(&[eve.inbox_id()]).await.is_err());
    receive_completed_prefix(&caro_group, &group).await?;
    group.with_group_snapshot(|mls| {
        assert!(mls.pending_proposals().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(add)
                if add.key_package().leaf_node().signature_key().as_slice()
                    == bo2.installation_id.as_slice())
        }));
        Ok(())
    })?;
    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo2.installation_id.to_vec()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;

    // The installation diff no longer permits Bo2. Preparation must not reuse
    // the accepted Add from the rejected operation after this revocation.
    let kind = if refresh_membership {
        IntentKind::UpdateGroupMembership
    } else {
        IntentKind::CommitPendingProposals
    };
    let refresh = if refresh_membership {
        QueueIntent::update_group_membership()
            .data(group.get_membership_update_intent(&[], &[]).await?)
            .queue(&group)?
    } else {
        QueueIntent::commit_pending_proposals()
            .data(Vec::<u8>::from(CommitPendingProposalsIntentData::new()))
            .queue(&group)?
    };
    let before = group.with_group_snapshot(PreparedBase::capture)?;
    let (_, prepared) = prepare_kind(&group, kind).await?;
    assert_eq!(before, group.with_group_snapshot(PreparedBase::capture)?);
    let intent: StoredGroupIntent = group.context.db().fetch(&refresh.id)?.unwrap();
    let commit = decode_staged_commit(intent.staged_commit.as_ref().unwrap())?;
    assert!(
        !commit.add_proposals().any(|add| add
            .add_proposal()
            .key_package()
            .leaf_node()
            .signature_key()
            .as_slice()
            == bo2.installation_id.as_slice()),
        "prepared commit includes a revoked installation"
    );
    group.publish_intents().await?;
    group.sync_until_intent_resolved(refresh.id).await?;
    group
        .send_message(b"after pending add revocation", SendMessageOpts::default())
        .await?;
    assert!(!prepared.envelopes.is_empty());
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
) -> Result<(), GroupError> {
    use crate::groups::group_permissions::PreconfiguredPolicies;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dave, disable_workers);
    tester!(erin, disable_workers);
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
                        == caro.installation_id.as_slice() =>
                {
                    Some(add.key_package().clone())
                }
                _ => None,
            })
            .ok_or(GroupError::UninitializedResult)
    })?;
    let current_package = if rotate {
        caro.rotate_and_upload_key_package().await?;
        wait_for_rotated_package(&group, caro.installation_id, &original_package).await
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
        crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![caro.installation_id.to_vec()]),
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
        crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage(false, None);
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

#[xmtp_common::test(unwrap_try = true)]
async fn send_after_rejected_membership_refreshes_pending_adds_immediately() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(eve, disable_workers);
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), eve.inbox_id()],
            None,
            None,
        )
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    caro_group.receive().await?;
    tester!(bo2, from: bo, disable_workers);
    // Refresh leaf changes are inline. Seed the accepted standalone Add
    // explicitly so this test still exercises pending-proposal recovery.
    publish_installation_add(
        &caro_group,
        bo2.installation_id.as_slice(),
        &[&group, &caro_group],
    )
    .await?;
    // Caro is not an admin, so receivers reject the removal.
    assert!(caro_group.remove_members(&[eve.inbox_id()]).await.is_err());
    receive_completed_prefix(&caro_group, &group).await?;
    // A successful recent check must not hide a valid pending installation Add.
    group
        .context
        .db()
        .update_installations_time_checked(&group.group_id)?;
    group
        .send_message(
            b"immediate send after rejection",
            SendMessageOpts::default(),
        )
        .await?;
    bo2.sync_welcomes().await?;
    let bo2_group = bo2.group(&group.group_id)?;
    bo2_group.receive().await?;
    assert!(
        bo2_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| message.decrypted_message_bytes == b"immediate send after rejection")
    );
}

#[rstest::rstest]
#[case::live(false, false)]
#[case::reopened(true, false)]
#[case::concurrent_adds(false, true)]
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_removal_with_new_installation_does_not_block_publication(
    #[case] reopen: bool,
    #[case] concurrent_adds: bool,
) -> Result<(), GroupError> {
    tester!(alix, persistent_db, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dave, disable_workers);
    tester!(eve, disable_workers);
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
    tester!(bo2, from: bo, disable_workers);
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
        bo2.installation_id.as_slice(),
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
                bo2.installation_id.as_slice()
            );
            assert!(
                !mls_group
                    .members()
                    .any(|member| member.signature_key == bo2.installation_id.as_slice())
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
                        == bo2.installation_id.as_slice() =>
                {
                    Some(add.key_package().clone())
                }
                _ => None,
            })
            .ok_or(GroupError::UninitializedResult)
    })?;
    let retired_hash = crate::identity::serialize_key_package_hash_ref(
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
    crate::worker::key_package_maintenance::delete_key_package(
        &bo2.context,
        retired.key_package_hash_ref,
        retired.post_quantum_public_key,
    )?;
    let current_package =
        wait_for_rotated_package(&group, bo2.installation_id, &retained_package).await;
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
                    .filter(|key| **key == bo2.installation_id.as_slice())
                    .count(),
                2
            );
            assert!(adds.contains(&dave.installation_id.as_slice()));
            Ok(())
        })?;
    }
    let pending_before_reopen = group.with_group_snapshot(PreparedBase::capture)?;
    let group = if reopen {
        use crate::{
            Client, builder::DeviceSyncMode, identity::IdentityStrategy,
            utils::DefaultTestClientCreator,
        };
        use xmtp_db::{StorageOption, TestDb, XmtpTestDb};
        use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
        use xmtp_proto::{api_client::ApiBuilder, prelude::XmtpTestClient};
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
        drop(group);
        drop(alix);
        let store = TestDb::create_persistent_store(Some(database)).await;
        let api = std::sync::Arc::new(DefaultTestClientCreator::create().build().unwrap());
        let restarted = Client::builder(IdentityStrategy::CachedOnly)
            .store(store)
            .api_client(api)
            .default_mls_store()
            .unwrap()
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_disable_workers(true)
            .with_commit_log_worker(false)
            .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
            .build()
            .await
            .unwrap();
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
                == bo2.installation_id.as_slice()
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

// verifies: GMOD-038, JOIN-008
#[xmtp_common::test(unwrap_try = true)]
async fn accepted_add_that_expires_before_commit_does_not_block_publication()
-> Result<(), GroupError> {
    const PACKAGE_LIFETIME_SECS: u64 = 10;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("Bo must process the initial Welcome");
    let package = crate::state_tx::state_write(caro.context.mls_storage(), |tx| {
        let storage = tx.storage();
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let now = u64::try_from(xmtp_common::time::now_secs()).unwrap();
        let generated = xmtp_id::key_package::build_key_package(
            caro.inbox_id(),
            caro.context.identity().credential(),
            &caro.context.identity().installation_keys,
            &provider,
            xmtp_id::key_package::KeyPackageOptions {
                lifetime: Some(openmls::prelude::Lifetime::init(
                    now,
                    now + PACKAGE_LIFETIME_SECS,
                )),
                ..Default::default()
            },
        )
        .expect("build a package with a bounded lifetime");
        Ok::<_, GroupError>(Continue(generated.bundle.key_package().clone()))
    })?
    .into_continued();
    let bytes = package.tls_serialize_detached()?;
    caro.context.api().upload_key_package(bytes.clone()).await?;
    xmtp_common::wait_for_eq(
        || async {
            let fetched = group
                .context
                .api()
                .fetch_key_packages(&[caro.installation_id])
                .await
                .expect("fetch the short-lived package");
            fetched
                .get(&caro.installation_id)
                .and_then(Option::as_ref)
                .map(|entry| entry.key_package_tls_serialized.clone())
        },
        Some(bytes.clone()),
    )
    .await
    .expect("the short-lived package must be readable before proposing it");
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    bo_group.with_group_snapshot(|mls| {
        assert!(mls.pending_proposals().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(add) if add.key_package() == &package)
        }));
        Ok(())
    })?;
    let crypto = openmls_rust_crypto::RustCrypto::default();
    xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(&crypto, &bytes)
        .expect("the received Add must be accepted before its package expires");
    // Wait for the actual validity transition. No clock or receiver policy is changed.
    xmtp_common::wait_for_some(|| async {
        xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(&crypto, &bytes)
            .is_err()
            .then_some(())
    })
    .await
    .expect("the accepted package must expire before commit preparation");
    let before = bo_group.with_group_snapshot(PreparedBase::capture)?;
    let commit = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    prepare_kind(&bo_group, IntentKind::CommitPendingProposals).await?;
    assert_eq!(before, bo_group.with_group_snapshot(PreparedBase::capture)?);
    let prepared: StoredGroupIntent = bo_group.context.db().fetch(&commit.id)?.unwrap();
    let staged = decode_staged_commit(prepared.staged_commit.as_deref().unwrap())?;
    assert_eq!(staged.add_proposals().count(), 0);
    assert!(prepared.post_commit_data.is_none());
    bo_group.sync_until_intent_resolved(commit.id).await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert_eq!(group.members().await?.len(), 2);
    assert_eq!(
        group.epoch_authenticator().await?,
        bo_group.epoch_authenticator().await?
    );
    for peer in [&group, &bo_group] {
        peer.with_group_snapshot(|mls| {
            assert_eq!(mls.pending_proposals().count(), 0);
            Ok(())
        })?;
    }
    bo_group
        .send_message(b"after expired Add", SendMessageOpts::default())
        .await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert!(
        group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| { message.decrypted_message_bytes == b"after expired Add" })
    );
    Ok(())
}

// verifies: GMOD-038
#[rstest::rstest]
#[case::new_installation_update(false)]
#[case::request_satisfied_by_competing_commit(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn own_membership_delta_and_pending_removal_use_the_same_selected_state(
    #[case] already_satisfied: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("Bo must join the initial group");
    let caro_group = xmtp_common::wait_for_ok(|| async {
        caro.sync_welcomes()
            .await
            .and_then(|_| caro.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("Caro must join the initial group");
    tester!(bo2, from: bo, disable_workers);
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

// Create a group of three. Each joiner waits for its Welcome: a lagging
// replica can return nothing on the first sync.
async fn group_of_three(
    alix: &crate::utils::FullXmtpClient,
    bo: &crate::utils::FullXmtpClient,
    caro: &crate::utils::FullXmtpClient,
) -> Result<[crate::utils::TestMlsGroup; 3], GroupError> {
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    caro.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
    receive_completed_prefix(&group, &bo_group).await?;
    receive_completed_prefix(&group, &caro_group).await?;
    Ok([group, bo_group, caro_group])
}

pub(super) fn committed_membership(group: &crate::utils::TestMlsGroup) -> GroupMembership {
    group
        .with_group_snapshot(|mls| Ok(extract_group_membership(mls.extensions())?))
        .expect("read the committed membership")
}

// Read one member's committed sequence id for an inbox.
fn committed_sequence(group: &crate::utils::TestMlsGroup, inbox_id: &str) -> Option<u64> {
    committed_membership(group).get(inbox_id).copied()
}

pub(super) fn has_leaf(group: &crate::utils::TestMlsGroup, installation: &[u8]) -> bool {
    group
        .with_group_snapshot(|mls| {
            Ok(mls
                .members()
                .any(|member| member.signature_key == installation))
        })
        .expect("read the ratchet tree")
}

fn pending_membership_deltas(group: &crate::utils::TestMlsGroup) -> usize {
    use xmtp_mls_common::app_data::component_id::ComponentId;
    group
        .with_group_snapshot(|mls| {
            Ok(mls
                .pending_proposals()
                .filter(|proposal| {
                    matches!(proposal.proposal(), Proposal::AppDataUpdate(update)
                        if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16())
                })
                .count())
        })
        .expect("read pending proposals")
}

// Send on each member in turn, then check that every peer converged and
// received every message.
async fn send_on_every_member(
    members: &[&crate::utils::TestMlsGroup],
    label: &str,
) -> Result<(), GroupError> {
    let mut bodies = Vec::new();
    for (index, sender) in members.iter().enumerate() {
        let body = format!("{label} {index}").into_bytes();
        let result = sender.send_message(&body, SendMessageOpts::default()).await;
        if let Err(GroupError::SyncFailedToWait(summary)) = &result {
            panic!("member {index} must send: {:?}", summary.publish_errors);
        }
        result?;
        bodies.push(body);
        for peer in members {
            receive_completed_prefix(sender, peer).await?;
        }
    }
    let authenticator = members[0].epoch_authenticator().await?;
    for peer in members {
        assert_eq!(peer.epoch_authenticator().await?, authenticator);
        let messages = peer.find_messages(&MsgQueryArgs::default())?;
        for body in &bodies {
            assert!(
                messages
                    .iter()
                    .any(|message| message.decrypted_message_bytes == *body),
                "every member must receive every send"
            );
        }
    }
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn stale_refresh_does_not_lower_committed_sequence() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;

    // The saved refresh names Alix's sequence S2. A later refresh commits S3.
    tester!(alix2, from: alix, disable_workers);
    let stale_data = group.get_membership_update_intent(&[], &[]).await?;
    let stale_sequence = stale_data.membership_updates[alix.inbox_id()];
    tester!(alix3, from: alix, disable_workers);
    caro_group.add_missing_installations().await?;
    for peer in [&group, &bo_group] {
        receive_completed_prefix(&caro_group, peer).await?;
    }
    let committed = committed_sequence(&group, alix.inbox_id()).unwrap();
    assert!(stale_sequence < committed);
    assert!(has_leaf(&group, alix3.installation_id.as_slice()));
    let before = group.epoch_authenticator().await?;

    // The stale refresh must resolve as a no-op. It must not publish a
    // lowering membership delta or remove the newer installation.
    let stale = QueueIntent::update_group_membership()
        .data(stale_data)
        .queue(&group)?;
    group.sync_until_intent_resolved(stale.id).await?;
    for peer in [&group, &bo_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
        assert_eq!(peer.epoch_authenticator().await?, before);
        assert_eq!(committed_sequence(peer, alix.inbox_id()), Some(committed));
        assert!(has_leaf(peer, alix3.installation_id.as_slice()));
        assert_eq!(
            pending_membership_deltas(peer),
            0,
            "the stale request must not leave a membership proposal"
        );
    }

    send_on_every_member(&[&group, &bo_group, &caro_group], "after stale refresh").await?;
    for peer in [&group, &bo_group, &caro_group] {
        assert_eq!(committed_sequence(peer, alix.inbox_id()), Some(committed));
        assert!(has_leaf(peer, alix3.installation_id.as_slice()));
    }
    drop((alix2, alix3));
    Ok(())
}

// Seed the standalone Add whose recovery is under test. Membership refreshes
// carry this leaf change inline, so a rejected refresh no longer leaves it.
async fn publish_installation_add(
    author: &crate::utils::TestMlsGroup,
    installation: &[u8],
    peers: &[&crate::utils::TestMlsGroup],
) -> Result<(), GroupError> {
    let package = MlsStore::new(author.context.clone())
        .get_key_packages_for_installation_ids(vec![installation.to_vec()])
        .await?
        .remove(installation)
        .expect("installation has a key package")
        .expect("key package verifies")
        .inner;
    let payload = crate::state_tx::state_write(author.context.mls_storage(), |tx| {
        tx.with_group(author.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let (message, _) = mls
                .propose_add_member(
                    &provider,
                    &author.context.identity().installation_keys,
                    &package,
                )
                .map_err(GroupError::ProposeAddMember)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    publish_standalone_proposal(author, payload, peers).await
}

// Publish one signed standalone proposal and wait until each peer processed it.
pub(super) async fn publish_standalone_proposal(
    author: &crate::utils::TestMlsGroup,
    payload: Vec<u8>,
    peers: &[&crate::utils::TestMlsGroup],
) -> Result<(), GroupError> {
    let messages = author.prepare_group_messages(vec![(payload.as_slice(), false)])?;
    let receipts = author.context.api().send_group_messages(messages).await?;
    let topic = Topic::new_group_message(author.group_id);
    let (_, cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    for peer in peers {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), cursor)].into(),
            None,
        )
        .await?;
    }
    Ok(())
}

// Sign a standalone membership proposal without local checks. Each peer's
// admission decides whether to store it.
pub(super) fn propose_membership_payload(
    author: &crate::utils::TestMlsGroup,
    payload: Vec<u8>,
) -> Result<Vec<u8>, GroupError> {
    use openmls::messages::proposals::AppDataUpdateOperation;
    use xmtp_mls_common::app_data::component_id::ComponentId;
    Ok(
        crate::state_tx::state_write(author.context.mls_storage(), |tx| {
            tx.with_group(author.group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let (message, _) = mls
                    .propose_app_data_update(
                        &provider,
                        &author.context.identity().installation_keys,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(payload.into()),
                    )
                    .map_err(GroupError::Proposal)?;
                Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
            })
        })?
        .into_continued(),
    )
}

// verifies: GMOD-038
#[xmtp_common::test(unwrap_try = true)]
async fn stored_stale_membership_proposal_does_not_block_sends() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&group, &bo_group, &caro_group];

    // Bo's identity reaches S2, then S3. Alix commits S3 with both new leaves.
    tester!(bo2, from: bo, disable_workers);
    let stale_sequence = caro_group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    tester!(bo3, from: bo, disable_workers);
    group.add_missing_installations().await?;
    for peer in [&bo_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
    }
    let committed = committed_sequence(&group, bo.inbox_id()).unwrap();
    assert!(stale_sequence < committed);
    for peer in members {
        assert!(has_leaf(peer, bo3.installation_id.as_slice()));
    }
    let before = group.epoch_authenticator().await?;

    // Caro, an old client, proposes Bo's stale S2 entry. Admission does not
    // check sequence ids, so every member stores the whole delta.
    let stale_payload = caro_group.with_group_snapshot(|mls| {
        let old = extract_group_membership(mls.extensions())?;
        let mut stale = old.clone();
        stale.add(bo.inbox_id().to_string(), stale_sequence);
        crate::groups::mls_sync::update_group_membership::build_group_membership_app_data_payload(
            &caro_group.context.db(),
            mls,
            &old,
            &stale,
        )
    })?;
    let stale_delta = propose_membership_payload(&caro_group, stale_payload)?;
    publish_standalone_proposal(&caro_group, stale_delta, &members).await?;
    // The stale request also removed the installation added after S2. Alix
    // is permitted to remove members, so every member stores the Remove too.
    let removal = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let leaf = mls
                .members()
                .find(|member| member.signature_key == bo3.installation_id.as_slice())
                .map(|member| member.index)
                .ok_or(GroupError::InvalidGroupMembership)?;
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let (message, _) = mls
                .propose_remove_member(&provider, &group.context.identity().installation_keys, leaf)
                .map_err(GroupError::ProposeRemoveMember)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    publish_standalone_proposal(&group, removal, &members).await?;
    for peer in members {
        assert_eq!(pending_membership_deltas(peer), 1);
        assert!(peer.with_group_snapshot(|mls| {
            Ok(mls
                .pending_proposals()
                .any(|proposal| matches!(proposal.proposal(), Proposal::Remove(_))))
        })?);
        assert_eq!(peer.epoch_authenticator().await?, before);
    }

    // Bo is neither author. Its preflight commit must omit the stale delta
    // and the Remove that depended on it.
    send_on_every_member(
        &[&bo_group, &group, &caro_group],
        "after stored stale delta",
    )
    .await?;
    for peer in members {
        assert_ne!(peer.epoch_authenticator().await?, before);
        assert_eq!(committed_sequence(peer, bo.inbox_id()), Some(committed));
        assert!(has_leaf(peer, bo3.installation_id.as_slice()));
        assert!(has_leaf(peer, bo2.installation_id.as_slice()));
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0,
            "the next epoch expires the omitted proposals"
        );
    }
    Ok(())
}

fn membership_entry(sequence_id: u64, failed_installations: Vec<Vec<u8>>) -> tls_codec::VLBytes {
    use xmtp_proto::xmtp::mls::message_contents::{GroupMembershipEntry, group_membership_entry};
    tls_codec::VLBytes::new(
        GroupMembershipEntry {
            version: Some(group_membership_entry::Version::V1(
                group_membership_entry::V1 {
                    sequence_id,
                    failed_installations,
                },
            )),
        }
        .encode_to_vec(),
    )
}

// An admitted membership delta that no commit can keep.
#[derive(Debug, Clone, Copy)]
enum UncommittableDelta {
    // A valid update of one inbox, then a stale update of another.
    StaleAfterValidUpdate,
    // A delete, then a stale insert of the same inbox. Each proposal is
    // admitted against the committed map, so a separate insert of a
    // committed inbox is refused; one delta can carry both.
    StaleInsertAfterDelete,
    // An entry value that does not decode as a membership entry. Admission
    // checks the map codec, not entry contents.
    UndecodableEntry,
    // The same for an inbox that is not a member yet.
    UndecodableNewEntry,
    // A new inbox whose entry names sequence 0, which no added entry may.
    ZeroSequenceNewEntry,
    // A raise to a sequence id that no identity update can have.
    OutOfRangeSequence,
}

// verifies: GMOD-038
#[rstest::rstest]
#[case::stale_after_valid_update(UncommittableDelta::StaleAfterValidUpdate)]
#[case::stale_insert_after_delete(UncommittableDelta::StaleInsertAfterDelete)]
#[case::undecodable_entry(UncommittableDelta::UndecodableEntry)]
#[case::undecodable_new_entry(UncommittableDelta::UndecodableNewEntry)]
#[case::zero_sequence_new_entry(UncommittableDelta::ZeroSequenceNewEntry)]
#[case::out_of_range_sequence(UncommittableDelta::OutOfRangeSequence)]
#[xmtp_common::test(unwrap_try = true)]
async fn uncommittable_membership_delta_is_omitted_whole(
    #[case] uncommittable: UncommittableDelta,
) -> Result<(), GroupError> {
    use tls_codec::VLBytes;
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&group, &bo_group, &caro_group];

    // A commit raises Caro from S1 to S2, so S1 is a real stale sequence.
    let caro_stale = committed_sequence(&group, caro.inbox_id()).unwrap();
    tester!(caro2, from: caro, disable_workers);
    group.add_missing_installations().await?;
    for peer in [&bo_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
    }
    let caro_sequence = committed_sequence(&group, caro.inbox_id()).unwrap();
    assert!(caro_stale < caro_sequence);
    // Bo's identity then reaches S2. No commit has raised Bo's entry yet.
    tester!(bo2, from: bo, disable_workers);
    let raised = caro_group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    assert!(committed_sequence(&group, bo.inbox_id()).unwrap() < raised);
    let alix_sequence = committed_sequence(&group, alix.inbox_id()).unwrap();
    let key = |inbox: &str| InboxId::from_hex(inbox).expect("valid inbox id");
    let kept_marker = vec![0xA1; 32];
    let omitted_marker = vec![0xB2; 32];

    // Alix may add and remove members. A same-sequence entry and a raised
    // entry write no committed inbox lower, so the commit must keep them.
    let kept = TlsMapDelta::new()
        .update(
            key(alix.inbox_id()),
            membership_entry(alix_sequence, vec![kept_marker.clone()]),
        )
        .update(
            key(bo.inbox_id()),
            membership_entry(raised, vec![bo2.installation_id.to_vec()]),
        );
    let caro_key = key(caro.inbox_id());
    let uncommittable = match uncommittable {
        // Keeping this delta would replace Alix's kept entry.
        UncommittableDelta::StaleAfterValidUpdate => TlsMapDelta::new()
            .update(
                key(alix.inbox_id()),
                membership_entry(alix_sequence, vec![omitted_marker.clone()]),
            )
            .update(caro_key, membership_entry(caro_stale, vec![])),
        UncommittableDelta::StaleInsertAfterDelete => TlsMapDelta::new().delete(caro_key).insert(
            caro_key,
            membership_entry(caro_stale, vec![omitted_marker.clone()]),
        ),
        // Field 1 announces five bytes and ends.
        UncommittableDelta::UndecodableEntry => {
            TlsMapDelta::new().update(caro_key, VLBytes::new(vec![0x0a, 0x05]))
        }
        UncommittableDelta::UndecodableNewEntry => {
            TlsMapDelta::new().insert(key(&"ab".repeat(32)), VLBytes::new(vec![0x0a, 0x05]))
        }
        UncommittableDelta::ZeroSequenceNewEntry => {
            TlsMapDelta::new().insert(key(&"ab".repeat(32)), membership_entry(0, vec![]))
        }
        UncommittableDelta::OutOfRangeSequence => {
            TlsMapDelta::new().update(caro_key, membership_entry(u64::MAX, vec![]))
        }
    };
    // The uncommittable delta is last, so a commit that keeps it applies it whole.
    for delta in [kept, uncommittable] {
        let message = propose_membership_payload(&group, delta.tls_serialize_detached()?)?;
        publish_standalone_proposal(&group, message, &members).await?;
    }
    let before = group.epoch_authenticator().await?;
    for peer in members {
        assert_eq!(
            pending_membership_deltas(peer),
            2,
            "admission must store both deltas"
        );
    }

    // Bo is not the author. Its commit must omit the whole uncommittable
    // delta and keep the other one. A send would first refresh Bo's own
    // identity, and that request omits every delta that writes Bo, so commit
    // the pending proposals directly.
    let commit = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    let result = bo_group.sync_until_intent_resolved(commit.id).await;
    if let Err(GroupError::SyncFailedToWait(summary)) = &result {
        panic!("Bo must commit: {:?}", summary.publish_errors);
    }
    result?;
    for peer in members {
        receive_completed_prefix(&bo_group, peer).await?;
        assert_ne!(peer.epoch_authenticator().await?, before);
        let membership = committed_membership(peer);
        assert_eq!(membership.get(alix.inbox_id()), Some(&alix_sequence));
        assert_eq!(membership.get(bo.inbox_id()), Some(&raised));
        assert_eq!(membership.get(caro.inbox_id()), Some(&caro_sequence));
        assert_eq!(membership.members.len(), 3);
        assert!(membership.failed_installations.contains(&kept_marker));
        assert!(!membership.failed_installations.contains(&omitted_marker));
        assert!(has_leaf(peer, caro2.installation_id.as_slice()));
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    send_on_every_member(&members, "after uncommittable delta").await?;
    Ok(())
}

// A refresh of the sender's own installations reads every pending membership
// delta to find an equivalent accepted one. It must skip an omitted delta too.
// verifies: GMOD-038
#[xmtp_common::test(unwrap_try = true)]
async fn undecodable_membership_delta_does_not_block_own_refresh() -> Result<(), GroupError> {
    use tls_codec::VLBytes;
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&bo_group, &group, &caro_group];
    let caro_key = InboxId::from_hex(caro.inbox_id()).expect("valid inbox id");
    // Field 1 announces five bytes and ends.
    let delta = TlsMapDelta::new().update(caro_key, VLBytes::new(vec![0x0a, 0x05]));
    let message = propose_membership_payload(&group, delta.tls_serialize_detached()?)?;
    publish_standalone_proposal(&group, message, &members).await?;

    // Bo's new installation gives Bo's refresh its own membership delta.
    tester!(bo2, from: bo, disable_workers);
    bo_group.add_missing_installations().await?;
    for peer in members {
        receive_completed_prefix(&bo_group, peer).await?;
        assert!(has_leaf(peer, bo2.installation_id.as_slice()));
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    send_on_every_member(&members, "after undecodable delta").await?;
    Ok(())
}

// A pending delta can name an identity update that the backend never
// published. The committer resolves it, finds nothing, and omits the delta.
// verifies: GMOD-038
#[xmtp_common::test(unwrap_try = true)]
async fn membership_delta_with_absent_reference_is_omitted() -> Result<(), GroupError> {
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&group, &bo_group, &caro_group];
    let absent = InboxId::from_hex(&"ab".repeat(32)).expect("valid inbox id");
    let delta = TlsMapDelta::new().insert(absent, membership_entry(5, vec![]));
    let message = propose_membership_payload(&group, delta.tls_serialize_detached()?)?;
    publish_standalone_proposal(&group, message, &members).await?;
    let before = group.epoch_authenticator().await?;

    // The default absence wait is 30 seconds. Shorten it for the committer.
    let mut settings = bo.context.incoming_runtime().policy().clone();
    settings.identity_reference_wait = std::time::Duration::from_millis(5_500);
    let committer = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .with_disable_workers(true)
        .stream_policy(settings)
        .build()
        .await
        .expect("rebuild Bo's client with a short absence wait");
    let committer_group = committer.group(&group.group_id)?;
    let commit = QueueIntent::commit_pending_proposals().queue(&committer_group)?;
    let result = committer_group.sync_until_intent_resolved(commit.id).await;
    if let Err(GroupError::SyncFailedToWait(summary)) = &result {
        panic!("Bo must commit: {:?}", summary.publish_errors);
    }
    result?;
    for peer in members {
        receive_completed_prefix(&committer_group, peer).await?;
        assert_ne!(peer.epoch_authenticator().await?, before);
        assert_eq!(committed_membership(peer).members.len(), 3);
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    send_on_every_member(&members, "after absent reference").await?;
    Ok(())
}
