//! Required membership components in an outgoing proposal subset.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_db::{group_message::MsgQueryArgs, incoming_envelope::StreamTopic};
use xmtp_mls_common::app_data::component_id::ComponentId;

// verifies: GMOD-019, GMOD-038
#[rstest::rstest]
#[case::commit_pending(IntentKind::CommitPendingProposals)]
#[case::metadata(IntentKind::MetadataUpdate)]
#[xmtp_common::test(unwrap_try = true)]
async fn accepted_membership_component_remove_does_not_block_publication(
    #[case] kind: IntentKind,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("the published Welcome must become visible");
    bo_group.receive().await?;
    let membership =
        group.with_group_snapshot(|mls| Ok(extract_group_membership(mls.extensions())?))?;
    assert_eq!(
        bo_group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        0
    );

    // The creator is authorized by the committed Delete policy. Publish its
    // signed standalone proposal and let the peer validate it through ingestion.
    let (payload, reference) = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let (message, reference) = mls
                .propose_app_data_update(
                    &provider,
                    &group.context.identity().installation_keys,
                    ComponentId::GROUP_MEMBERSHIP.as_u16(),
                    AppDataUpdateOperation::Remove,
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue((message.tls_serialize_detached()?, reference)))
        })
    })?
    .into_continued();
    let messages = group.prepare_group_messages(vec![(payload.as_slice(), false)])?;
    let receipts = group.context.api().send_group_messages(messages).await?;
    let topic = Topic::new_group_message(group.group_id);
    let (_, proposal_cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    crate::subscriptions::barrier::wait_through(
        &bo_group.context,
        [(topic.clone(), proposal_cursor)].into(),
        None,
    )
    .await?;
    bo_group.with_group_snapshot(|mls| {
        let accepted: Vec<_> = mls.pending_proposals().collect();
        assert_eq!(accepted.len(), 1, "the received Remove must be accepted");
        assert_eq!(accepted[0].proposal_reference_ref(), &reference);
        assert!(matches!(
            accepted[0].proposal(),
            Proposal::AppDataUpdate(update)
                if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16()
                    && matches!(update.operation(), AppDataUpdateOperation::Remove)
        ));
        Ok(())
    })?;
    let before = bo_group.with_group_snapshot(PreparedBase::capture)?;
    let request = match kind {
        IntentKind::CommitPendingProposals => {
            QueueIntent::commit_pending_proposals().queue(&bo_group)?
        }
        IntentKind::MetadataUpdate => QueueIntent::metadata_update()
            .data(Vec::<u8>::from(
                UpdateMetadataIntentData::new_update_group_name("after component Remove".into()),
            ))
            .queue(&bo_group)?,
        _ => unreachable!("each case names a commit intent"),
    };
    let _ = prepare_kind(&bo_group, kind).await?;
    let prepared: StoredGroupIntent = bo_group.context.db().fetch(&request.id)?.unwrap();
    let staged = decode_staged_commit(prepared.staged_commit.as_deref().unwrap())?;
    assert!(
        staged
            .queued_proposals()
            .all(|proposal| proposal.proposal_reference_ref() != &reference),
        "omit the whole component Remove from the outgoing commit"
    );
    assert_eq!(
        bo_group.with_group_snapshot(PreparedBase::capture)?,
        before,
        "preparation must preserve the accepted reference and committed state"
    );
    bo_group.sync_until_intent_resolved(request.id).await?;
    assert!(bo_group.epoch().await? > before.epoch);
    let target = bo_group
        .context
        .db()
        .topic_progress(&StreamTopic::group(group.group_id))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &group.context,
        [(topic.clone(), target)].into(),
        None,
    )
    .await?;
    for peer in [&group, &bo_group] {
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(extract_group_membership(mls.extensions())?))?,
            membership
        );
        assert_eq!(
            peer.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0,
            "normal epoch advancement expires the omitted proposal"
        );
    }
    assert_eq!(group.epoch().await?, bo_group.epoch().await?);
    assert_eq!(
        group.epoch_authenticator().await?,
        bo_group.epoch_authenticator().await?
    );
    if kind == IntentKind::MetadataUpdate {
        assert_eq!(group.group_name()?, "after component Remove");
        assert_eq!(bo_group.group_name()?, group.group_name()?);
    }

    let body = b"send after accepted membership component Remove";
    bo_group
        .send_message(body, SendMessageOpts::default())
        .await?;
    let target = bo_group
        .context
        .db()
        .topic_progress(&StreamTopic::group(group.group_id))?
        .processed;
    crate::subscriptions::barrier::wait_through(&group.context, [(topic, target)].into(), None)
        .await?;
    let messages = group.find_messages(&MsgQueryArgs::default())?;
    assert!(
        messages
            .iter()
            .any(|message| message.decrypted_message_bytes == body)
    );
    Ok(())
}
