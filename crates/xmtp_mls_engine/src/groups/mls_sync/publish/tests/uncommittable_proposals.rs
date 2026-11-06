//! Pending proposals a commit cannot carry.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps};
use openmls::{
    framing::{MlsMessageIn, ProcessedMessageContent},
    messages::proposals::AppDataUpdateOperation,
};
use tls_codec::Deserialize as _;
use xmtp_db::incoming_envelope::StreamTopic;
use xmtp_mls_group_state::app_data::components::tls_map_components::GroupMembershipComponent;
use xmtp_mls_group_state::app_data::typed::Component as _;
use xmtp_mls_group_state::{app_data::component_id::ComponentId, inbox_id::InboxId};
use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMembershipEntry,
    group_membership_entry::{V1, Version},
};

/// Have `sender` propose `count` `AppDataUpdate`s, built by `update`, and
/// store each in the group of every peer in `receivers`, as receiving them
/// does. Batches go in one transaction each: a network round trip per
/// proposal outlasts the test timeout.
async fn flood(
    sender: &EngineTestPeer,
    group_id: crate::groups::GroupId,
    receivers: &[&EngineTestPeer],
    count: usize,
    update: impl Fn(usize) -> (ComponentId, AppDataUpdateOperation),
) -> Result<(), GroupError> {
    const BATCH: usize = 200;
    let signer = &sender.context.identity().installation_keys;
    for start in (0..count).step_by(BATCH) {
        let proposals = xmtp_mls_core::state_tx::state_write(sender.context.mls_storage(), |tx| {
            tx.with_group(group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let proposals = (start..(start + BATCH).min(count))
                    .map(|index| {
                        let (id, operation) = update(index);
                        let (message, _) = mls
                            .propose_app_data_update(&provider, signer, id.as_u16(), operation)
                            .map_err(GroupError::Proposal)?;
                        Ok(message.tls_serialize_detached()?)
                    })
                    .collect::<Result<Vec<_>, GroupError>>()?;
                Ok::<_, GroupError>(Continue(proposals))
            })
        })?
        .into_continued();
        for receiver in receivers {
            xmtp_mls_core::state_tx::state_write(receiver.context.mls_storage(), |tx| {
                tx.with_group(group_id, |mls, storage| {
                    let provider = XmtpOpenMlsProviderRef::new(storage);
                    for proposal in &proposals {
                        let message = MlsMessageIn::tls_deserialize_exact(proposal)?
                            .try_into_protocol_message()
                            .unwrap();
                        let ProcessedMessageContent::ProposalMessage(queued) = mls
                            .process_message(&provider, message)
                            .unwrap()
                            .into_content()
                        else {
                            unreachable!("a proposal");
                        };
                        mls.store_pending_proposal(storage, *queued)?;
                    }
                    Ok::<_, GroupError>(Continue(()))
                })
            })?;
        }
    }
    Ok(())
}

/// A membership delta that rewrites `inbox`'s entry at `sequence_id` with
/// `failed` installations, made byte-distinct by an unknown field holding
/// `index`.
fn distinct_entry(
    inbox: &str,
    sequence_id: u64,
    failed: &[Vec<u8>],
    index: usize,
) -> AppDataUpdateOperation {
    let mut entry = GroupMembershipEntry {
        version: Some(Version::V1(V1 {
            sequence_id,
            failed_installations: failed.to_vec(),
        })),
    }
    .encode_to_vec();
    // Field 15, varint: receivers keep unknown fields.
    entry.push(15 << 3);
    prost::encoding::encode_varint(index as u64, &mut entry);
    let delta = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .update(InboxId::from_hex(inbox).unwrap(), entry.into());
    AppDataUpdateOperation::Update(
        GroupMembershipComponent::encode_mutation(&delta)
            .unwrap()
            .into(),
    )
}

/// A commit covers each reference once, so a repeated reference must count
/// once in the pending state. Otherwise an Insert, a Delete, and the Insert
/// again would leave the key present for the sender and absent for every
/// receiver of the commit.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn a_repeated_reference_counts_once() -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let owner = InboxId::from_hex(alix.inbox_id()).unwrap();
    let insert = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .insert(owner, b"Alix".to_vec().into())
        .tls_serialize_detached()?;
    let delete = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .delete(owner)
        .tls_serialize_detached()?;
    let signer = &group.context.identity().installation_keys;
    let (omitted, expected) =
        xmtp_mls_core::state_tx::state_write(group.context.mls_storage(), |tx| {
            tx.with_group(group.group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let mut references = Vec::new();
                for payload in [&insert, &delete] {
                    let (_, reference) = mls
                        .propose_app_data_update(
                            &provider,
                            signer,
                            ComponentId::USER_DISPLAY_NAME.as_u16(),
                            AppDataUpdateOperation::Update(payload.clone().into()),
                        )
                        .map_err(GroupError::Proposal)?;
                    references.push(reference);
                }
                let repeat = mls.pending_proposals().next().cloned().unwrap();
                mls.store_pending_proposal(storage, repeat)?;
                let omitted = selection::omit_uncommittable_proposals(
                    mls,
                    storage,
                    crate::groups::app_data::MAX_APP_DATA_PROPOSALS,
                    xmtp_mls_validation::commit::MAX_DICTIONARY_BYTES,
                )?;
                Ok::<_, GroupError>(Continue((omitted, references)))
            })
        })?
        .into_continued();
    assert!(omitted);
    let stored: Vec<_> = group.with_group_snapshot(|mls| {
        Ok(mls
            .pending_proposals()
            .map(|queued| queued.proposal_reference_ref().clone())
            .collect())
    })?;
    assert_eq!(stored, expected);
    Ok(())
}

/// Receivers accept each standalone proposal, and only a published commit
/// discards them. A commit that carried them all grew past the envelope
/// limit, so no member could send or change the group again. A commit keeps
/// the oldest `AppDataUpdate`s up to a cap; the rest expire with the epoch.
// verifies: GMOD-047
#[rstest::rstest]
#[case::dm_send(false)]
#[case::member_removal(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn a_proposal_flood_does_not_block_commits(
    #[case] removal: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    const ENVELOPE_LIMIT: usize = 64 * 1024;
    const FLOOD: usize = 2_500;
    // 65536 / 136.
    const KEPT: usize = 481;
    let alix =
        EngineTestPeer::new_with(|o| o.configure(|c| c.limits.max_envelope_bytes = ENVELOPE_LIMIT))
            .await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = if removal {
        alix.create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
            .await?
    } else {
        alix.find_or_create_dm(bo.inbox_id(), None).await?
    };
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let name = |index: usize| format!("name {index}");
    flood(&bo, bo_group.group_id, &[&alix], FLOOD, |index| {
        (
            ComponentId::GROUP_NAME,
            AppDataUpdateOperation::Update(name(index).into_bytes().into()),
        )
    })
    .await?;

    if removal {
        group.remove_members(&[caro.inbox_id()]).await?;
    } else {
        group
            .send_message(b"hello", SendMessageOpts::default())
            .await?;
    }

    let pending = group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?;
    assert_eq!(pending, 0);
    bo_group.sync().await?;
    assert_eq!(group.group_name()?, name(KEPT - 1));
    assert_eq!(bo_group.group_name()?, name(KEPT - 1));
    Ok(())
}

/// Membership deltas are capped too. Bo floods byte-distinct rewrites of its
/// own entry, which receivers accept; a commit that carried them all would
/// not fit the envelope. The intent's own change is not part of the cap.
// verifies: GMOD-047
#[rstest::rstest]
#[case::dm_send(false)]
#[case::member_removal(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn a_membership_proposal_flood_does_not_block_commits(
    #[case] removal: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    const ENVELOPE_LIMIT: usize = 64 * 1024;
    const FLOOD: usize = 2_500;
    let alix =
        EngineTestPeer::new_with(|o| o.configure(|c| c.limits.max_envelope_bytes = ENVELOPE_LIMIT))
            .await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = if removal {
        alix.create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
            .await?
    } else {
        alix.find_or_create_dm(bo.inbox_id(), None).await?
    };
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let (membership, failed) = group.with_group_snapshot(|mls| {
        Ok(xmtp_mls_validation::commit::extract_group_membership_with_failures(mls.extensions())?)
    })?;
    let bo_sequence = *membership.get(bo.inbox_id()).unwrap();
    assert!(failed.of(bo.inbox_id()).is_none_or(|keys| keys.is_empty()));
    flood(&bo, bo_group.group_id, &[&alix], FLOOD, |index| {
        (
            ComponentId::GROUP_MEMBERSHIP,
            distinct_entry(bo.inbox_id(), bo_sequence, &[], index),
        )
    })
    .await?;

    if removal {
        group.remove_members(&[caro.inbox_id()]).await?;
        let members = group.members().await?;
        assert!(members.iter().all(|m| m.inbox_id != caro.inbox_id()));
    } else {
        group
            .send_message(b"hello", SendMessageOpts::default())
            .await?;
    }

    let pending = group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?;
    assert_eq!(pending, 0);
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, group.epoch().await?);
    assert!(
        bo.context
            .db()
            .read_last_rejection(&StreamTopic::group(bo_group.group_id))?
            .is_none()
    );
    Ok(())
}

/// The cap scales with the deployment's envelope limit: at 16 KiB a commit
/// keeps 120 of the pending `AppDataUpdate`s, whose references fit.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn the_proposal_cap_scales_with_the_envelope_limit() -> Result<(), Box<dyn std::error::Error>>
{
    const ENVELOPE_LIMIT: usize = 16 * 1024;
    const FLOOD: usize = 500;
    // 16384 / 136.
    const KEPT: usize = 120;
    let alix =
        EngineTestPeer::new_with(|o| o.configure(|c| c.limits.max_envelope_bytes = ENVELOPE_LIMIT))
            .await;
    let bo = EngineTestPeer::new().await;
    let group = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let name = |index: usize| format!("name {index}");
    flood(&bo, bo_group.group_id, &[&alix], FLOOD, |index| {
        (
            ComponentId::GROUP_NAME,
            AppDataUpdateOperation::Update(name(index).into_bytes().into()),
        )
    })
    .await?;

    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;

    bo_group.sync().await?;
    assert_eq!(group.group_name()?, name(KEPT - 1));
    assert_eq!(bo_group.group_name()?, name(KEPT - 1));
    Ok(())
}

/// A membership delta past the cap is omitted with the Add that depended on
/// it, so receivers still accept the commit.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn a_capped_membership_delta_drops_its_add() -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::{intents::QueueIntent, test_support::propose_member_update_data};
    const JUNK: usize = 1_000;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    let bo_sequence = *group
        .with_group_snapshot(|mls| {
            Ok(xmtp_mls_validation::commit::extract_group_membership(
                mls.extensions(),
            )?)
        })?
        .get(bo.inbox_id())
        .unwrap();
    flood(&bo, bo_group.group_id, &[&alix, &caro], JUNK, |index| {
        (
            ComponentId::GROUP_MEMBERSHIP,
            distinct_entry(bo.inbox_id(), bo_sequence, &[], index),
        )
    })
    .await?;
    let intent = QueueIntent::propose_member_update()
        .data(propose_member_update_data(
            vec![dave.inbox_id().to_string()],
            vec![],
        )?)
        .queue(&caro_group)?;
    caro_group.sync_until_intent_resolved(intent.id).await?;
    group.receive().await?;
    let pending = || group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()));
    assert!(pending()? > JUNK, "alix holds caro's Add and delta");

    let epoch = group.epoch().await?;
    let commit = QueueIntent::commit_pending_proposals().queue(&group)?;
    group.sync_until_intent_resolved(commit.id).await?;
    assert!(group.epoch().await? > epoch);
    assert_eq!(pending()?, 0);
    for (peer, peer_group) in [(&bo, &bo_group), (&caro, &caro_group)] {
        peer_group.sync().await?;
        assert_eq!(peer_group.epoch().await?, group.epoch().await?);
        assert!(
            peer.context
                .db()
                .read_last_rejection(&StreamTopic::group(peer_group.group_id))?
                .is_none()
        );
    }
    let members = group.members().await?;
    assert!(members.iter().all(|m| m.inbox_id != dave.inbox_id()));
    Ok(())
}

/// Any member can flood byte-distinct copies of a delta that gives a
/// refresh's own target, for both rungs of the retry ladder: with Bo's new
/// installation added, and with it listed as failed. Only the oldest copy
/// stands for the refresh; the rest are other members' deltas, so the
/// commit still fits and adds the installation. With the cap already full
/// of another member's deltas, the refresh's own change is still kept.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn copies_of_the_refresh_target_do_not_escape_the_cap()
-> Result<(), Box<dyn std::error::Error>> {
    use super::membership_recovery::{committed_membership, has_leaf};
    const ENVELOPE_LIMIT: usize = 64 * 1024;
    // More than the cap of 481 at this limit.
    const FILL: usize = 500;
    const COPIES: usize = 2_500;
    let alix =
        EngineTestPeer::new_with(|o| o.configure(|c| c.limits.max_envelope_bytes = ENVELOPE_LIMIT))
            .await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    let committed = committed_membership(&group);
    let caro_sequence = *committed.get(caro.inbox_id()).unwrap();
    let bo2 = bo.new_installation().await;
    bo.inbox_state(true).await?;
    let raised = bo
        .context
        .db()
        .get_latest_sequence_id_for_inbox(bo.inbox_id())? as u64;
    assert!(raised > *committed.get(bo.inbox_id()).unwrap());
    // Caro fills the cap with rewrites of her own entry, then Bo sends
    // copies of the entry Alix's refresh will write for him.
    flood(&caro, caro_group.group_id, &[&alix, &bo], FILL, |index| {
        (
            ComponentId::GROUP_MEMBERSHIP,
            distinct_entry(caro.inbox_id(), caro_sequence, &[], index),
        )
    })
    .await?;
    let bo2_key = bo2.context.installation_id().to_vec();
    for failed in [vec![], vec![bo2_key]] {
        flood(&bo, bo_group.group_id, &[&alix, &caro], COPIES, |index| {
            (
                ComponentId::GROUP_MEMBERSHIP,
                distinct_entry(bo.inbox_id(), raised, &failed, index),
            )
        })
        .await?;
    }

    group.update_installations().await?;

    assert_eq!(
        committed_membership(&group).get(bo.inbox_id()),
        Some(&raised)
    );
    assert!(has_leaf(&group, bo2.context.installation_id().as_slice()));
    let pending = group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?;
    assert_eq!(pending, 0);
    for (peer, peer_group) in [(&bo, &bo_group), (&caro, &caro_group)] {
        peer_group.sync().await?;
        assert_eq!(peer_group.epoch().await?, group.epoch().await?);
        assert!(
            peer.context
                .db()
                .read_last_rejection(&StreamTopic::group(peer_group.group_id))?
                .is_none()
        );
    }
    Ok(())
}
