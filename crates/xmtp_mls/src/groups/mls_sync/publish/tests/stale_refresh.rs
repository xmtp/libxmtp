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
/// it as failed, or a later revocation removes nothing and the revoked
/// installation keeps reading the group.
// verifies: GMOD-044
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
    // Caro is not an admin. Members accept the Add for bo2 that Caro's
    // removal commit carries, and reject the commit.
    assert!(caro_group.remove_members(&[eve.inbox_id()]).await.is_err());
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
