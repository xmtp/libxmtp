//! Pending removal accounting must agree with receiver validation.

use super::membership_recovery::{
    committed_membership, has_leaf, propose_membership_payload, publish_standalone_proposal,
};
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{EngineTestGroup, EngineTestPeer, EngineTestPeerOps};
use crate::groups::tests::{revoke, store_locally};
use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Serialize, VLBytes};
use xmtp_cryptography::CredentialSign;
use xmtp_id::associations::{
    InstallationKeyContext, MemberIdentifier,
    builder::SignatureRequestBuilder,
    test_utils::{WalletTestExt, add_wallet_signature},
    unverified::UnverifiedSignature,
};
use xmtp_mls_group_state::{app_data::component_id::ComponentId, inbox_id::InboxId};
use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
use xmtp_proto::xmtp::mls::message_contents::{GroupMembershipEntry, group_membership_entry};

fn entry_update(inbox: &str, sequence: u64, failures: Vec<Vec<u8>>) -> Vec<u8> {
    let entry = GroupMembershipEntry {
        version: Some(group_membership_entry::Version::V1(
            group_membership_entry::V1 {
                sequence_id: sequence,
                failed_installations: failures,
            },
        )),
    };
    TlsMapDelta::new()
        .update(
            InboxId::from_hex(inbox).unwrap(),
            VLBytes::new(entry.encode_to_vec()),
        )
        .tls_serialize_detached()
        .unwrap()
}

/// Associate `installation`'s key with `member`'s inbox in the local histories
/// of `holders` only: the backend refuses a key that another inbox added.
async fn associate_locally(
    member: &EngineTestPeer,
    installation: &EngineTestPeer,
    holders: &[&EngineTestPeer],
) {
    let key = &installation.context.identity().installation_keys;
    let mut request = SignatureRequestBuilder::new(member.inbox_id())
        .add_association(
            MemberIdentifier::installation(key.public_slice().to_vec()),
            member.owner.identifier().into(),
        )
        .build();
    let signature = key
        .credential_sign::<InstallationKeyContext>(request.signature_text())
        .unwrap();
    request
        .add_signature(
            UnverifiedSignature::new_installation_key(signature, key.verifying_key()),
            &member.context.scw_verifier(),
        )
        .await
        .unwrap();
    add_wallet_signature(&mut request, &member.owner).await;
    store_locally(holders, request).await;
}

/// Revoke `key` from `member`'s inbox in the local histories of `holders` only.
async fn revoke_locally(member: &EngineTestPeer, key: &[u8], holders: &[&EngineTestPeer]) {
    let mut request = SignatureRequestBuilder::new(member.inbox_id())
        .revoke_association(
            member.owner.identifier().into(),
            MemberIdentifier::installation(key.to_vec()),
        )
        .build();
    add_wallet_signature(&mut request, &member.owner).await;
    store_locally(holders, request).await;
}

async fn accept_delta(group: &EngineTestGroup, payload: Vec<u8>) -> Result<(), GroupError> {
    let proposal = propose_membership_payload(group, payload)?;
    publish_standalone_proposal(group, proposal, &[group]).await
}

/// Restore a membership state an older receiver accepted, bypassing today's
/// stricter identity-removal check. This creates the pre-upgrade test state only.
fn legacy_entry(group: &EngineTestGroup, payload: Vec<u8>) -> Result<(), GroupError> {
    xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            crate::groups::app_data::stage_app_data_proposals_and_commit(
                mls,
                &provider,
                &group.context.identity().installation_keys,
                &[],
                &xmtp_configuration::LimitsConfiguration::default(),
                vec![(
                    ComponentId::GROUP_MEMBERSHIP,
                    AppDataUpdateOperation::Update(payload.into()),
                )],
            )?;
            mls.merge_pending_commit(&provider)?;
            Ok::<_, GroupError>(Continue(()))
        })
    })?;
    Ok(())
}

/// Alix's second installation's key is also associated with Bo, in Alix's
/// local history. After Bo revokes it, a pending delta that raises Bo's
/// entry without the Remove of that key's leaf, which names Alix, is omitted
/// by a commit of pending proposals and by a readd.
// verifies: GMOD-047, GMOD-052
#[rstest::rstest]
#[case::commit_pending(false)]
#[case::readd(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn shared_key_revocation_delta_without_remove_is_omitted(
    #[case] readd: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let alix2 = alix.new_installation().await;
    let bo = EngineTestPeer::new().await;
    associate_locally(&bo, &alix2, &[&alix]).await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    // The readd entry point must not consume the pending delta in a prior
    // minimum-version commit.
    group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    let key = alix2.context.installation_id().to_vec();
    assert!(has_leaf(&group, &key));
    assert!(group.with_group_snapshot(|mls| Ok(mls.members().any(
        |member| member.signature_key == key
            && xmtp_mls_validation::commit::inbox_id_from_credential(&member.credential).unwrap()
                == alix.inbox_id()
    )))?);
    let alix_sequence = committed_membership(&group)
        .get(alix.inbox_id())
        .copied()
        .unwrap();
    accept_delta(
        &group,
        entry_update(alix.inbox_id(), alix_sequence, vec![key.clone()]),
    )
    .await?;
    let claim = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(claim.id).await?;
    assert!(
        committed_membership(&group)
            .failed_installations
            .contains(&key)
    );

    revoke_locally(&bo, &key, &[&alix]).await;
    let old_sequence = committed_membership(&group)
        .get(bo.inbox_id())
        .copied()
        .unwrap();
    let raised = group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    accept_delta(&group, entry_update(bo.inbox_id(), raised, vec![])).await?;
    if readd {
        let epoch = group.epoch().await?;
        group.readd_installations(vec![key.clone()]).await?;
        assert_eq!(group.epoch().await?, epoch + 1, "the readd must commit");
    } else {
        let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
        group.sync_until_intent_resolved(commit.id).await?;
    }
    assert_eq!(
        committed_membership(&group).get(bo.inbox_id()),
        Some(&old_sequence)
    );
    assert!(has_leaf(&group, &key));
    // A later refresh removes the leaf, which leaves the key unaccounted
    // for Alix, so receivers reject it (GMOD Known limitations).
    Ok(())
}

// verifies: GMOD-047, GMOD-052
#[xmtp_common::test(unwrap_try = true)]
async fn failed_key_remove_does_not_cover_a_pending_refresh()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let bo2 = bo.new_installation().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let key = bo2.context.installation_id().to_vec();
    revoke(&bo, &key).await;
    let stale_sequence = group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    legacy_entry(
        &group,
        entry_update(bo.inbox_id(), stale_sequence, vec![key.clone()]),
    )?;
    assert!(has_leaf(&group, &key));
    assert!(
        committed_membership(&group)
            .failed_installations
            .contains(&key)
    );
    // A new identity update advances Bo without introducing a leaf to add.
    let temporary = bo.new_installation().await;
    revoke(&bo, temporary.context.installation_id().as_slice()).await;
    let raised = group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    accept_delta(
        &group,
        entry_update(bo.inbox_id(), raised, vec![key.clone()]),
    )
    .await?;
    let remove = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let leaf = mls
                .members()
                .find(|member| member.signature_key == key)
                .unwrap()
                .index;
            let (message, _) = mls.propose_remove_member(
                &provider,
                &group.context.identity().installation_keys,
                leaf,
            )?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    publish_standalone_proposal(&group, remove, &[&group]).await?;
    assert_eq!(
        group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        2
    );
    let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(commit.id).await?;
    assert_eq!(
        committed_membership(&group).get(bo.inbox_id()),
        Some(&stale_sequence)
    );
    assert!(has_leaf(&group, &key));
    group
        .send_message(b"after failed-key omission", SendMessageOpts::default())
        .await?;
    Ok(())
}

/// A commit accepted before receivers checked accounting can leave an
/// own-inbox key with no leaf and no failure listing, and later commits may
/// carry it forward. The
/// inbox's refresh retries the key: it adds the installation, its own echo
/// passes validation, and the installation joins.
// verifies: GMOD-055, GMOD-057
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_retries_a_carried_forward_unaccounted_own_key() {
    let bo = EngineTestPeer::new().await;
    let group = bo.create_group(None, None)?;
    let bo2 = bo.new_installation().await;
    let key = bo2.context.installation_id().to_vec();
    bo.inbox_state(true).await?;
    let sequence = bo
        .context
        .db()
        .get_latest_sequence_id_for_inbox(bo.inbox_id())? as u64;
    assert!(*committed_membership(&group).get(bo.inbox_id())? < sequence);
    legacy_entry(&group, entry_update(bo.inbox_id(), sequence, vec![]))?;
    let legacy = committed_membership(&group);
    assert_eq!(legacy.get(bo.inbox_id()), Some(&sequence));
    assert!(!legacy.failed_installations.contains(&key));
    assert!(!has_leaf(&group, &key));

    group.update_installations().await?;
    assert!(has_leaf(&group, &key), "the refresh retries the key");
    let joined = bo2.wait_for_welcomes().await?;
    assert_eq!(joined.first()?.group_id, group.group_id);
}

/// Alix's second installation's key is also associated with Bo, and Bo
/// revokes it, both in Alix's local history. Alix's refresh must remove that
/// key's leaf, which names Alix, so it leaves the key unaccounted for Alix
/// (GMOD Known limitations).
/// With a membership delta pending, commit building finds the failure: the
/// refresh intent must fail without blocking later intents.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn a_failed_own_change_does_not_block_later_intents() {
    use xmtp_db::group_intent::{IntentKind, IntentState, QueryGroupIntent};
    let alix = EngineTestPeer::new().await;
    let alix2 = alix.new_installation().await;
    let bo = EngineTestPeer::new().await;
    associate_locally(&bo, &alix2, &[&alix]).await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let key = alix2.context.installation_id().to_vec();
    revoke_locally(&bo, &key, &[&alix]).await;
    let alix_sequence = *committed_membership(&group).get(alix.inbox_id())?;
    accept_delta(&group, entry_update(alix.inbox_id(), alix_sequence, vec![])).await?;

    let refresh = group.update_installations().await;
    let queued = alix.context.db().find_group_intents(
        group.group_id,
        Some(vec![IntentState::ToPublish, IntentState::Published]),
        Some(vec![IntentKind::UpdateGroupMembership]),
    )?;
    assert!(
        queued.is_empty(),
        "the refresh is not left queued ({refresh:?}): {queued:?}"
    );

    let epoch = group.epoch().await?;
    group.update_group_name("after".into()).await?;
    assert!(group.epoch().await? > epoch);
    assert_eq!(group.group_name()?, "after");
}
