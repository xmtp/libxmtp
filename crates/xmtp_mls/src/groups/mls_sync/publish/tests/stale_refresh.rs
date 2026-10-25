//! A queued membership refresh that publishes after the group changed.

use super::membership_recovery::{
    committed_membership, has_leaf, propose_membership_payload, publish_standalone_proposal,
    receive_completed_prefix,
};
use super::*;
use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

/// A refresh records an installation whose key package it could not fetch.
/// By publication that installation can have a leaf: the publish-time fetch
/// succeeds, or another member's refresh added it. The commit must not list
/// it as failed, since a failure entry describes an absent installation.
// verifies: GMOD-044, GMOD-046
#[rstest::rstest]
#[case::publish_time_fetch(false)]
#[case::concurrent_refresh(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn stale_failure_with_a_leaf_is_not_written(
    #[case] concurrent_refresh: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    receive_completed_prefix(&group, &bo_group).await?;

    // Bo's identity gains bo2, whose key package Alix's refresh cannot fetch.
    tester!(bo2, from: bo, disable_workers);
    let bo2_id = bo2.installation_id.to_vec();
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo2_id.clone()]));
    // With a concurrent refresh, the stale request must still change the
    // group, so it also adds Caro.
    let add: &[&str] = if concurrent_refresh {
        &[caro.inbox_id()]
    } else {
        &[]
    };
    let stale = group.get_membership_update_intent(add, &[]).await?;
    set_test_mode_upload_malformed_keypackage(false, None);
    assert_eq!(stale.failed_installations, vec![bo2_id.clone()]);
    if concurrent_refresh {
        bo_group.add_missing_installations().await?;
        receive_completed_prefix(&bo_group, &group).await?;
        assert!(has_leaf(&group, &bo2_id));
    }

    let stale = QueueIntent::update_group_membership()
        .data(stale)
        .queue(&group)?;
    group.sync_until_intent_resolved(stale.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    for member in [&group, &bo_group] {
        let membership = committed_membership(member);
        assert!(has_leaf(member, &bo2_id));
        assert!(
            !membership.failed_installations.contains(&bo2_id),
            "an installation with a leaf must not be listed as failed"
        );
        assert_eq!(
            membership.get(caro.inbox_id()).is_some(),
            concurrent_refresh
        );
    }

    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo2_id.clone()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    group.add_missing_installations().await?;
    receive_completed_prefix(&group, &bo_group).await?;
    for member in [&group, &bo_group] {
        assert!(
            !has_leaf(member, &bo2_id),
            "a revoked installation must lose its leaf"
        );
    }
    drop(bo2);
    Ok(())
}

/// Selection builds the same failed list as publication. An accepted Add for
/// an installation that a stale refresh lists as failed is still wanted, so
/// the commit reuses it instead of signing another Add.
// verifies: GMOD-044, GMOD-038
#[xmtp_common::test(unwrap_try = true)]
async fn selection_reuses_an_add_for_a_stale_failure() -> Result<(), GroupError> {
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
    caro.wait_for_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    receive_completed_prefix(&group, &caro_group).await?;
    tester!(bo2, from: bo, disable_workers);
    let bo2_id = bo2.installation_id.to_vec();
    // Publish the Add explicitly: membership refresh commits now carry it
    // inline, so a rejected refresh no longer leaves a standalone Add.
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![bo.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&caro_group)?;
    caro_group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&caro_group, &group).await?;
    let is_bo2_add = |proposal: &openmls::group::QueuedProposal| {
        matches!(proposal.proposal(), Proposal::Add(add)
            if add.key_package().leaf_node().signature_key().as_slice() == bo2_id.as_slice())
    };
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().any(is_bo2_add)))?);

    // Alix's refresh cannot fetch bo2's key package when it is built.
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo2_id.clone()]));
    let stale = group.get_membership_update_intent(&[], &[]).await?;
    set_test_mode_upload_malformed_keypackage(false, None);
    assert_eq!(stale.failed_installations, vec![bo2_id.clone()]);
    let stale = QueueIntent::update_group_membership()
        .data(stale)
        .queue(&group)?;
    let (_, attempt) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    for proposal in &attempt.proposals {
        let signed = attempt
            .proposal_for_payload(&proposal.payload_hash)?
            .expect("each saved proposal decodes");
        assert!(!is_bo2_add(&signed), "reuse the accepted Add for bo2");
    }
    group.sync_until_intent_resolved(stale.id).await?;
    receive_completed_prefix(&group, &caro_group).await?;
    for member in [&group, &caro_group] {
        assert!(has_leaf(member, &bo2_id));
        assert!(
            !committed_membership(member)
                .failed_installations
                .contains(&bo2_id)
        );
    }
    assert_eq!(
        group.epoch_authenticator().await?,
        caro_group.epoch_authenticator().await?
    );
    Ok(())
}

/// A refresh built before a commit removed an inbox must not insert that
/// inbox when it publishes. Only an add request inserts an inbox.
// verifies: GMOD-051
#[xmtp_common::test(unwrap_try = true)]
async fn stale_refresh_does_not_readd_a_removed_inbox() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    caro.wait_for_welcomes().await?;
    let caro_group = caro.group(&group.group_id)?;
    receive_completed_prefix(&group, &caro_group).await?;

    // The refresh raises Bo's entry for bo2. Then a commit removes Bo.
    tester!(bo2, from: bo, disable_workers);
    let stale = group.get_membership_update_intent(&[], &[]).await?;
    assert!(stale.membership_updates.contains_key(bo.inbox_id()));
    group.remove_members(&[bo.inbox_id()]).await?;
    let epoch = group.epoch().await?;

    let stale = QueueIntent::update_group_membership()
        .data(stale)
        .queue(&group)?;
    group.sync_until_intent_resolved(stale.id).await?;
    receive_completed_prefix(&group, &caro_group).await?;
    for member in [&group, &caro_group] {
        assert_eq!(committed_membership(member).get(bo.inbox_id()), None);
        assert!(!has_leaf(member, bo.installation_id.as_slice()));
        assert!(!has_leaf(member, bo2.installation_id.as_slice()));
        assert_eq!(member.epoch().await?, epoch, "the stale refresh is a no-op");
    }
    Ok(())
}

/// Older clients can list an installation that has a leaf as failed. A
/// revocation of that installation must still remove its leaf: receivers do
/// not exempt a key that has a leaf before the commit.
// verifies: GMOD-046
#[xmtp_common::test(unwrap_try = true)]
async fn revocation_removes_a_leaf_that_an_entry_lists_as_failed() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(bo2, from: bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    receive_completed_prefix(&group, &bo_group).await?;
    let bo2_id = bo2.installation_id.to_vec();
    assert!(has_leaf(&group, &bo2_id));

    // Write Bo's entry with bo2 listed as failed, as an older client could.
    // The payload resolves bo2's owner, so read through the transaction's
    // connection; another connection waits on this transaction.
    let payload = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let old = extract_group_membership(mls.extensions())?;
            let mut listed = old.clone();
            listed.failed_installations = vec![bo2_id.clone()];
            update_group_membership::build_group_membership_app_data_payload(
                &storage.db(),
                mls,
                &old,
                &listed,
            )
            .map(Continue)
        })
    })?
    .into_continued();
    let proposal = propose_membership_payload(&group, payload)?;
    publish_standalone_proposal(&group, proposal, &[&group, &bo_group]).await?;
    let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(commit.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    for member in [&group, &bo_group] {
        assert!(has_leaf(member, &bo2_id));
        assert!(
            committed_membership(member)
                .failed_installations
                .contains(&bo2_id)
        );
    }

    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo2_id.clone()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    group.add_missing_installations().await?;
    receive_completed_prefix(&group, &bo_group).await?;
    for member in [&group, &bo_group] {
        assert!(!has_leaf(member, &bo2_id), "the revocation must remove bo2");
        assert!(
            !committed_membership(member)
                .failed_installations
                .contains(&bo2_id)
        );
    }
    drop(bo2);
    Ok(())
}

async fn check_false_failure_claim(selection: bool) -> Result<(), GroupError> {
    use crate::groups::send_message_opts::SendMessageOpts;
    use tls_codec::{Serialize, VLBytes};
    use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
    use xmtp_proto::xmtp::mls::message_contents::{GroupMembershipEntry, group_membership_entry};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(bo2, from: bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    bo2.wait_for_welcomes().await?;
    caro.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let bo2_group = bo2.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
    for peer in [&bo_group, &bo2_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
    }
    let revoked_id = bo2.installation_id.to_vec();
    assert!(has_leaf(&group, &revoked_id));

    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![revoked_id.clone()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    let revoked_sequence = group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    assert!(revoked_sequence > *committed_membership(&group).get(bo.inbox_id()).unwrap());

    // Sign a raw AppDataUpdate as an ordinary group member. The normal
    // membership sender would add a Remove and strip this false failure.
    let entry = GroupMembershipEntry {
        version: Some(group_membership_entry::Version::V1(
            group_membership_entry::V1 {
                sequence_id: revoked_sequence,
                failed_installations: vec![revoked_id.clone()],
            },
        )),
    };
    let payload = TlsMapDelta::new()
        .update(
            InboxId::from_hex(bo.inbox_id()).expect("valid test inbox"),
            VLBytes::new(entry.encode_to_vec()),
        )
        .tls_serialize_detached()?;
    let proposal = propose_membership_payload(&caro_group, payload)?;
    publish_standalone_proposal(
        &caro_group,
        proposal,
        &[&group, &bo_group, &bo2_group, &caro_group],
    )
    .await?;
    let old_sequence = *committed_membership(&group).get(bo.inbox_id()).unwrap();
    if selection {
        let commit = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
        bo_group.sync_until_intent_resolved(commit.id).await?;
        receive_completed_prefix(&bo_group, &group).await?;
        assert_eq!(
            committed_membership(&bo_group).get(bo.inbox_id()),
            Some(&old_sequence)
        );
        assert_eq!(
            committed_membership(&group).get(bo.inbox_id()),
            Some(&old_sequence)
        );
        return Ok(());
    }

    // Bypass proposal selection so this reaches the receiver's rule.
    let receiver_group =
        OpenMlsGroup::load(group.context.mls_storage(), &group.group_id.to_openmls())?
            .expect("receiver group is stored");
    let signer = caro.context.identity().installation_keys.clone();
    let (payloads, staged, sender) =
        crate::state_tx::state_write(caro.context.mls_storage(), |tx| {
            tx.with_group(caro_group.group_id, |mls, storage| {
                let sender = mls.own_leaf_index();
                let ((proposals, bundle), staged, _) =
                    generate_prepared_commit(storage, mls, |mls, provider| {
                        crate::groups::app_data::stage_app_data_proposals_and_commit(
                            mls,
                            provider,
                            &signer,
                            &[],
                            vec![],
                        )
                        .map_err(GroupError::from)
                    })?;
                let (commit, _, _) = bundle.into_messages();
                let bytes = proposals
                    .iter()
                    .chain(std::iter::once(&commit))
                    .map(Serialize::tls_serialize_detached)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, GroupError>(Continue((bytes, staged, sender)))
            })
        })?
        .into_continued();
    let staged = xmtp_db::db_deserialize(&staged.expect("raw commit is staged"))?;
    let error = crate::groups::validated_commit::ValidatedCommit::from_staged_commit(
        &group.context,
        &staged,
        sender,
        &receiver_group,
    )
    .await
    .expect_err("the raw commit must be rejected");
    assert!(matches!(
        error,
        CommitValidationError::Rule(CommitRuleError::UnexpectedInstallationsRemoved(keys))
            if keys.contains(&revoked_id)
    ));
    let messages = caro_group.prepare_group_messages(
        payloads
            .iter()
            .map(|bytes| (bytes.as_slice(), false))
            .collect(),
    )?;
    let receipts = caro.context.api().send_group_messages(messages).await?;
    let topic = Topic::new_group_message(group.group_id);
    let (_, commit_cursor, _) = xmtp_api_backend::envelope::metadata(
        receipts.last().expect("raw commit receipt"),
        topic.kind(),
    )?;
    for member in [&group, &bo_group, &bo2_group] {
        // A full sync also refreshes membership. Stop at the received commit
        // so the rejection and unchanged state are observed before healing.
        crate::subscriptions::barrier::wait_through(
            &member.context,
            [(topic.clone(), commit_cursor)].into(),
            None,
        )
        .await?;
        let rejected = member
            .context
            .db()
            .read_last_rejection(&xmtp_db::incoming_envelope::StreamTopic::group(
                member.group_id,
            ))?
            .expect("terminal rejection recorded");
        assert_eq!(rejected.sequence_id, commit_cursor);
        assert_eq!(rejected.code, "commit_validation_failure");
        assert!(has_leaf(member, &revoked_id));
        assert_eq!(
            committed_membership(member).get(bo.inbox_id()),
            Some(&old_sequence)
        );
    }
    group.add_missing_installations().await?;
    for member in [&bo_group, &bo2_group] {
        receive_completed_prefix(&group, member).await?;
    }
    for member in [&group, &bo_group, &bo2_group] {
        assert!(!has_leaf(member, &revoked_id));
    }
    group
        .send_message(b"after revocation", SendMessageOpts::default())
        .await?;
    let _ = bo2_group.receive().await;
    assert_ne!(
        bo2_group
            .test_last_message_bytes()
            .await
            .expect("read revoked device messages"),
        Some(b"after revocation".to_vec())
    );
    Ok(())
}

// verifies: GMOD-046
#[xmtp_common::test(unwrap_try = true)]
async fn newly_asserted_failure_cannot_keep_revoked_leaf() {
    check_false_failure_claim(false).await?;
}

// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn false_failure_delta_is_omitted_from_pending_commits() {
    check_false_failure_claim(true).await?;
}
