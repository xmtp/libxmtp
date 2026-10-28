//! Pending proposals a commit cannot carry.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use openmls::{
    framing::{MlsMessageIn, ProcessedMessageContent},
    messages::proposals::AppDataUpdateOperation,
};
use tls_codec::Deserialize as _;
use xmtp_mls_common::{
    app_data::component_id::ComponentId, inbox_id::InboxId, tls_map::TlsMapDelta,
};

/// A commit covers each reference once, so a repeated reference must count
/// once in the pending state. Otherwise an Insert, a Delete, and the Insert
/// again would leave the key present for the sender and absent for every
/// receiver of the commit.
// verifies: GMOD-047
#[xmtp_common::test(unwrap_try = true)]
async fn a_repeated_reference_counts_once() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let owner = InboxId::from_hex(alix.inbox_id()).unwrap();
    let insert = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .insert(owner, b"Alix".to_vec().into())
        .tls_serialize_detached()?;
    let delete = TlsMapDelta::<InboxId, tls_codec::VLBytes>::new()
        .delete(owner)
        .tls_serialize_detached()?;
    let signer = &group.context.identity().installation_keys;
    let (omitted, expected) = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
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
            let omitted = selection::omit_uncommittable_proposals(mls, storage)?;
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
async fn a_proposal_flood_does_not_block_commits(#[case] removal: bool) -> Result<(), GroupError> {
    const ENVELOPE_LIMIT: usize = 64 * 1024;
    const FLOOD: usize = 2_500;
    const BATCH: usize = 200;
    const KEPT: usize = 1_000;
    tester!(alix, configured: |c| c.limits.max_envelope_bytes = ENVELOPE_LIMIT, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = if removal {
        alix.create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
            .await?
    } else {
        alix.find_or_create_dm(bo.inbox_id(), None).await?
    };
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let name = |index: usize| format!("name {index}");
    let signer = &bo.context.identity().installation_keys;
    for start in (0..FLOOD).step_by(BATCH) {
        let proposals = crate::state_tx::state_write(bo.context.mls_storage(), |tx| {
            tx.with_group(bo_group.group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let proposals = (start..(start + BATCH).min(FLOOD))
                    .map(|index| {
                        let (message, _) = mls
                            .propose_app_data_update(
                                &provider,
                                signer,
                                ComponentId::GROUP_NAME.as_u16(),
                                AppDataUpdateOperation::Update(name(index).into_bytes().into()),
                            )
                            .map_err(GroupError::Proposal)?;
                        Ok(message.tls_serialize_detached()?)
                    })
                    .collect::<Result<Vec<_>, GroupError>>()?;
                Ok::<_, GroupError>(Continue(proposals))
            })
        })?
        .into_continued();
        // Store the batch as receiving it does, in one transaction: a
        // network round trip per proposal outlasts the test timeout.
        crate::state_tx::state_write(alix.context.mls_storage(), |tx| {
            tx.with_group(group.group_id, |mls, storage| {
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
