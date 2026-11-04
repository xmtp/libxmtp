//! Membership recovery publication tests: membership deltas.

use super::*;

// Create a group of three. Each joiner waits for its Welcome: a lagging
// replica can return nothing on the first sync.
async fn group_of_three(
    alix: &EngineTestPeer,
    bo: &EngineTestPeer,
    caro: &EngineTestPeer,
) -> Result<[EngineTestGroup; 3], Box<dyn std::error::Error>> {
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

// Read one member's committed sequence id for an inbox.
fn committed_sequence(group: &EngineTestGroup, inbox_id: &str) -> Option<u64> {
    committed_membership(group).get(inbox_id).copied()
}

fn pending_membership_deltas(group: &EngineTestGroup) -> usize {
    use xmtp_mls_group_state::app_data::component_id::ComponentId;
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
async fn send_on_every_member(members: &[&EngineTestGroup], label: &str) -> Result<(), GroupError> {
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
async fn stale_refresh_does_not_lower_committed_sequence() -> Result<(), Box<dyn std::error::Error>>
{
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;

    // The saved refresh names Alix's sequence S2. A later refresh commits S3.
    let alix2 = alix.new_installation().await;
    let stale_data = group.get_membership_update_intent(&[], &[]).await?;
    let stale_sequence = stale_data.membership_updates[alix.inbox_id()];
    let alix3 = alix.new_installation().await;
    caro_group.add_missing_installations().await?;
    for peer in [&group, &bo_group] {
        receive_completed_prefix(&caro_group, peer).await?;
    }
    let committed = committed_sequence(&group, alix.inbox_id()).unwrap();
    assert!(stale_sequence < committed);
    assert!(has_leaf(&group, alix3.context.installation_id().as_slice()));
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
        assert!(has_leaf(peer, alix3.context.installation_id().as_slice()));
        assert_eq!(
            pending_membership_deltas(peer),
            0,
            "the stale request must not leave a membership proposal"
        );
    }

    send_on_every_member(&[&group, &bo_group, &caro_group], "after stale refresh").await?;
    for peer in [&group, &bo_group, &caro_group] {
        assert_eq!(committed_sequence(peer, alix.inbox_id()), Some(committed));
        assert!(has_leaf(peer, alix3.context.installation_id().as_slice()));
    }
    drop((alix2, alix3));
    Ok(())
}

// verifies: GMOD-038
#[xmtp_common::test(unwrap_try = true)]
async fn stored_stale_membership_proposal_does_not_block_sends()
-> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&group, &bo_group, &caro_group];

    // Bo's identity reaches S2, then S3. Alix commits S3 with both new leaves.
    let bo2 = bo.new_installation().await;
    let stale_sequence = caro_group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    let bo3 = bo.new_installation().await;
    group.add_missing_installations().await?;
    for peer in [&bo_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
    }
    let committed = committed_sequence(&group, bo.inbox_id()).unwrap();
    assert!(stale_sequence < committed);
    for peer in members {
        assert!(has_leaf(peer, bo3.context.installation_id().as_slice()));
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
    let removal = xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let leaf = mls
                .members()
                .find(|member| member.signature_key == bo3.context.installation_id().as_slice())
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
        assert!(has_leaf(peer, bo3.context.installation_id().as_slice()));
        assert!(has_leaf(peer, bo2.context.installation_id().as_slice()));
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
) -> Result<(), Box<dyn std::error::Error>> {
    use tls_codec::VLBytes;
    use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
    use xmtp_id::associations::{
        InstallationKeyContext, MemberIdentifier,
        builder::SignatureRequestBuilder,
        test_utils::{WalletTestExt, add_wallet_signature},
        unverified::UnverifiedSignature,
    };
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    // Markers are real absent installations. Each inbox whose entry the
    // uncommittable cases can replace has its own: one key never belongs to
    // two member inboxes.
    let kept_key = XmtpInstallationCredential::new();
    let alix_marker_key = XmtpInstallationCredential::new();
    let caro_marker_key = XmtpInstallationCredential::new();
    for (client, installation) in [
        (&alix, &kept_key),
        (&alix, &alix_marker_key),
        (&caro, &caro_marker_key),
    ] {
        let mut request = SignatureRequestBuilder::new(client.inbox_id())
            .add_association(
                MemberIdentifier::installation(installation.public_slice().to_vec()),
                client.owner.identifier().into(),
            )
            .build();
        let signature = installation
            .credential_sign::<InstallationKeyContext>(request.signature_text())
            .unwrap();
        request
            .add_signature(
                UnverifiedSignature::new_installation_key(signature, installation.verifying_key()),
                &client.context.scw_verifier(),
            )
            .await
            .unwrap();
        add_wallet_signature(&mut request, &client.owner).await;
        client
            .identity_updates()
            .apply_signature_request(request)
            .await?;
    }
    let kept_marker = kept_key.public_slice().to_vec();
    let alix_marker = alix_marker_key.public_slice().to_vec();
    let caro_marker = caro_marker_key.public_slice().to_vec();
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    assert!(!has_leaf(&group, &kept_marker));
    assert!(!has_leaf(&group, &alix_marker));
    assert!(!has_leaf(&group, &caro_marker));
    let members = [&group, &bo_group, &caro_group];

    // A commit raises Caro from S1 to S2, so S1 is a real stale sequence.
    let caro_stale = committed_sequence(&group, caro.inbox_id()).unwrap();
    let caro2 = caro.new_installation().await;
    group.add_missing_installations().await?;
    for peer in [&bo_group, &caro_group] {
        receive_completed_prefix(&group, peer).await?;
    }
    let caro_sequence = committed_sequence(&group, caro.inbox_id()).unwrap();
    assert!(caro_stale < caro_sequence);
    // Bo's identity then reaches S2. No commit has raised Bo's entry yet.
    let bo2 = bo.new_installation().await;
    let raised = caro_group
        .get_membership_update_intent(&[], &[])
        .await?
        .membership_updates[bo.inbox_id()];
    assert!(committed_sequence(&group, bo.inbox_id()).unwrap() < raised);
    let alix_sequence = committed_sequence(&group, alix.inbox_id()).unwrap();
    let key = |inbox: &str| InboxId::from_hex(inbox).expect("valid inbox id");

    // Alix may add and remove members. A same-sequence entry and a raised
    // entry write no committed inbox lower, so the commit must keep them.
    // Every marker stays listed: each is leafless and still associated, so
    // dropping one would leave it unaccounted and receivers would reject the
    // commit.
    let kept = TlsMapDelta::new()
        .update(
            key(alix.inbox_id()),
            membership_entry(
                alix_sequence,
                vec![kept_marker.clone(), alix_marker.clone()],
            ),
        )
        .update(
            key(bo.inbox_id()),
            membership_entry(raised, vec![bo2.context.installation_id().to_vec()]),
        )
        .update(
            key(caro.inbox_id()),
            membership_entry(caro_sequence, vec![caro_marker.clone()]),
        );
    let caro_key = key(caro.inbox_id());
    let uncommittable = match uncommittable {
        // Keeping this delta would replace Alix's kept entry.
        // It also drops the kept marker from Alix's entry.
        UncommittableDelta::StaleAfterValidUpdate => TlsMapDelta::new()
            .update(
                key(alix.inbox_id()),
                membership_entry(alix_sequence, vec![alix_marker.clone()]),
            )
            .update(caro_key, membership_entry(caro_stale, vec![])),
        UncommittableDelta::StaleInsertAfterDelete => TlsMapDelta::new().delete(caro_key).insert(
            caro_key,
            membership_entry(caro_stale, vec![caro_marker.clone()]),
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
        assert!(membership.failed_installations.contains(&alix_marker));
        assert!(membership.failed_installations.contains(&caro_marker));
        assert!(has_leaf(peer, caro2.context.installation_id().as_slice()));
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
async fn undecodable_membership_delta_does_not_block_own_refresh()
-> Result<(), Box<dyn std::error::Error>> {
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&bo_group, &group, &caro_group];
    let caro_key = InboxId::from_hex(caro.inbox_id()).expect("valid inbox id");
    // Field 1 announces five bytes and ends.
    let delta = TlsMapDelta::new().update(caro_key, VLBytes::new(vec![0x0a, 0x05]));
    let message = propose_membership_payload(&group, delta.tls_serialize_detached()?)?;
    publish_standalone_proposal(&group, message, &members).await?;

    // Bo's new installation gives Bo's refresh its own membership delta.
    let bo2 = bo.new_installation().await;
    bo_group.add_missing_installations().await?;
    for peer in members {
        receive_completed_prefix(&bo_group, peer).await?;
        assert!(has_leaf(peer, bo2.context.installation_id().as_slice()));
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
async fn membership_delta_with_absent_reference_is_omitted()
-> Result<(), Box<dyn std::error::Error>> {
    use xmtp_mls_group_state::inbox_id::InboxId;
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let [group, bo_group, caro_group] = group_of_three(&alix, &bo, &caro).await?;
    let members = [&group, &bo_group, &caro_group];
    let absent = InboxId::from_hex(&"ab".repeat(32)).expect("valid inbox id");
    let delta = TlsMapDelta::new().insert(absent, membership_entry(5, vec![]));
    let message = propose_membership_payload(&group, delta.tls_serialize_detached()?)?;
    publish_standalone_proposal(&group, message, &members).await?;
    let before = group.epoch_authenticator().await?;

    // The default absence wait is 30 seconds. Shorten it for the committer.
    let committer = bo
        .with_options(|options| {
            options.stream_policy.identity_reference_wait = std::time::Duration::from_millis(5_500)
        })
        .await;
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
