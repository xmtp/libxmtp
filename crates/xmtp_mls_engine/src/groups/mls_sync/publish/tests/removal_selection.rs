//! Pending removal accounting must agree with receiver validation.

use super::membership_recovery::{
    committed_membership, has_leaf, propose_membership_payload, publish_standalone_proposal,
};
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{
    EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use crate::groups::tests::revoke;
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

async fn associate(member: &EngineTestPeer, installation: &EngineTestPeer) {
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
    member
        .identity_updates()
        .apply_signature_request(request)
        .await
        .unwrap();
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
    associate(&bo, &alix2).await;
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

    revoke(&bo, &key).await;
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
    group
        .send_message(b"after shared-key omission", SendMessageOpts::default())
        .await?;
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
