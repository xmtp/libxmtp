//! Proposal tests: missing proposals.

use super::*;

// =============================================================================
// Proposal Permission Validation Tests
// =============================================================================

/// A commit whose proposal the local store lacks is rejected only when the
/// complete prefix never delivered that proposal. The same reference, delivered
/// and then lost from local state, is held without advancing `P`, and an
/// independent topic still completes. A local-state loss must not become a
/// permanent skip of a commit every other member applies.
// verifies: PROC-011, PROC-012
#[xmtp_common::test(unwrap_try = true)]
async fn missing_proposal_absent_vs_lost() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use xmtp_db::TransactionOutcome::Continue;
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dan = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = super::super::receive_group_invite(&bo).await;
    let caro_group = super::super::receive_group_invite(&caro).await;
    bo_group.receive().await?;
    caro_group.receive().await?;
    let topic = StreamTopic::group(bo_group.group_id);
    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let bo_before = bo.context.db().topic_progress(&topic)?.received;
    let caro_before = caro.context.db().topic_progress(&topic)?.received;
    assert_eq!(bo_before, caro_before);

    // A membership update is proposals by reference, then their commit.
    let epoch = alix_group.epoch().await? as i64;
    alix_group.add_members(&[dan.inbox_id()]).await?;
    assert!(
        alix.context
            .db()
            .received_proposals(alix_group.group_id, epoch)?
            .is_empty(),
        "a merged commit clears the evidence of the proposals it consumed"
    );
    let mut envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), bo_before)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let commit = envelopes.pop().expect("the commit is on the network");
    assert!(!envelopes.is_empty(), "the commit has proposal envelopes");
    let commit_cursor = xmtp_proto::types::Cursor(
        commit
            .meta
            .as_ref()
            .and_then(|meta| meta.cursor.as_ref())
            .expect("the backend supplies a cursor")
            .sequence_id,
    );
    let limits = bo
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(topic.kind);

    // Bo receives the proposals, then local state loses them.
    let mut prefix = envelopes.clone();
    prefix.push(commit.clone());
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic.clone(),
            after: bo_before,
            envelopes: prefix.into_iter().map(Into::into).collect(),
        },
        limits,
    )?;
    for _ in &envelopes {
        assert!(matches!(
            bo_group.process_pending_group_head(None)?,
            GroupHeadOutcome::Progress { result: Ok(_), .. }
        ));
    }
    let proposals_handled = bo.context.db().topic_progress(&topic)?.processed;
    xmtp_mls_core::state_tx::state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let references: Vec<_> = mls
                .pending_proposals()
                .map(|proposal| proposal.proposal_reference_ref().clone())
                .collect();
            assert!(!references.is_empty());
            for reference in references {
                mls.remove_pending_proposal(storage, &reference).unwrap();
            }
            Ok::<_, crate::groups::GroupError>(Continue(()))
        })
    })?;
    let GroupHeadOutcome::Waiting {
        cursor,
        code,
        blocked,
    } = bo_group.process_pending_group_head(None)?
    else {
        panic!("a commit whose delivered proposal was lost must be held");
    };
    assert_eq!(cursor, commit_cursor);
    assert_eq!(code, "lost_proposal_state");
    assert!(blocked);
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        proposals_handled
    );
    assert_eq!(
        bo.context
            .db()
            .first_pending_envelope(&topic)?
            .map(|head| head.sequence_id as u64),
        Some(commit_cursor.0)
    );
    assert!(bo.context.db().read_last_rejection(&topic)?.is_none());

    // Caro's complete prefix never carried the proposals: the same reference is terminal.
    caro.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: caro_before,
            envelopes: vec![commit.into()],
        },
        limits,
    )?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = caro_group.process_pending_group_head(None)?
    else {
        panic!("a commit naming an absent proposal must be rejected");
    };
    assert_eq!(cursor, commit_cursor);
    assert!(error.is_missing_proposal(), "{error:?}");
    assert_eq!(
        caro.context.db().topic_progress(&topic)?.processed,
        commit_cursor
    );
    assert_eq!(
        caro.context
            .db()
            .read_last_rejection(&topic)?
            .map(|rejection| rejection.sequence_id),
        Some(commit_cursor)
    );

    // The held head does not stop Bo's other topics.
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    dm.send_message(b"independent", SendMessageOpts::default())
        .await?;
    bo.sync_welcomes().await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.sync().await?;
    assert!(
        bo_dm
            .find_messages(&Default::default())?
            .iter()
            .any(|message| message.decrypted_message_bytes == b"independent")
    );
    assert_eq!(
        bo.context.db().topic_progress(&topic)?.processed,
        proposals_handled
    );
}

/// A non-admin's commit that carries a pending display-name proposal and an
/// admin-list insert it may not make is rejected, and every member evicts the
/// proposal with it. An honest author whose commit, built before it saw that
/// rejection, references the proposal must reject its own commit as the other
/// members do, not apply its saved state and advance alone.
// verifies: GMOD-058
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_commit_eviction_does_not_split_a_prepared_author() {
    use crate::groups::{
        GroupError, app_data::accumulate_app_data_updates, intents::QueueIntent,
        mls_ext::commit_without_group_info, mls_sync::generate_prepared_commit,
        test_support::EngineTestGroup,
    };
    use openmls::{
        group::QueuedProposal,
        messages::proposals::{AppDataUpdateOperation, AppDataUpdateProposal, Proposal},
    };
    use openmls_traits::OpenMlsProvider;
    use tls_codec::{Serialize, VLBytes};
    use xmtp_db::{TransactionOutcome::Continue, XmtpOpenMlsProviderRef};
    use xmtp_db::{
        group_intent::{IntentState, StoredGroupIntent},
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    };
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_mls_group_state::{app_data::component_id::ComponentId, inbox_id::InboxId};
    use xmtp_mls_tls_collections::{tls_map::TlsMapDelta, tls_set::TlsSetDelta};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = super::super::receive_group_invite(&bo).await;
    let caro_group = super::super::receive_group_invite(&caro).await;

    // Bo proposes its display name; every member holds the proposal.
    let name = TlsMapDelta::<InboxId, VLBytes>::new()
        .insert(
            InboxId::from_hex(bo.inbox_id())?,
            VLBytes::new(b"Bo".to_vec()),
        )
        .tls_serialize_detached()?;
    let proposal = state_write(bo.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &bo.context.identity().installation_keys,
                    ComponentId::USER_DISPLAY_NAME.as_u16(),
                    AppDataUpdateOperation::Update(name.clone().into()),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
    bo.context.api().send_group_messages(messages).await?;
    let pending = |group: &EngineTestGroup| {
        group.with_group_snapshot(|mls| {
            Ok(mls
                .pending_proposals()
                .map(|proposal| proposal.proposal_reference_ref().clone())
                .collect::<Vec<_>>())
        })
    };
    // Reads can lag writes, so each member syncs until it holds the proposal.
    for group in [&alix_group, &bo_group, &caro_group] {
        xmtp_common::wait_for_eq(
            || async {
                group.sync().await.unwrap();
                pending(group).unwrap().len()
            },
            1,
        )
        .await?;
    }
    let references = pending(&alix_group)?;
    let reference = references[0].clone();
    for group in [&bo_group, &caro_group] {
        assert_eq!(pending(group)?, references);
    }
    let epoch = alix_group.epoch().await?;

    // Caro, not an admin, commits Bo's proposal by reference with an inline
    // admin-list insert of itself.
    let admin = TlsSetDelta::new()
        .insert(InboxId::from_hex(caro.inbox_id())?)
        .tls_serialize_detached()?;
    let signer = &caro.context.identity().installation_keys;
    let commit = state_write(caro.context.mls_storage(), |tx| {
        tx.with_group(caro_group.group_id, |mls, storage| {
            let (bundle, _, _) = generate_prepared_commit(storage, mls, |group, provider| {
                let inline = Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::update(
                    ComponentId::ADMIN_LIST.as_u16(),
                    admin.clone(),
                )));
                let operations: Vec<_> = group
                    .pending_proposals()
                    .map(QueuedProposal::proposal)
                    .chain([&inline])
                    .filter_map(|proposal| match proposal {
                        Proposal::AppDataUpdate(p) => {
                            Some((p.component_id(), p.operation().clone()))
                        }
                        _ => None,
                    })
                    .collect();
                let updates = accumulate_app_data_updates(
                    group,
                    operations.iter().map(|(id, operation)| (*id, operation)),
                )
                .unwrap();
                let mut stage = commit_without_group_info(group, provider.storage(), |builder| {
                    builder.consume_proposal_store(true).add_proposal(inline)
                })
                .unwrap();
                stage.with_app_data_dictionary_updates(updates);
                let bundle = stage
                    .build(provider.rand(), provider.crypto(), signer, |_| true)
                    .unwrap()
                    .stage_commit(provider)
                    .unwrap();
                Ok::<_, GroupError>(bundle)
            })?;
            let (commit, _, _) = bundle.into_messages();
            Ok::<_, GroupError>(Continue(commit.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = caro_group.prepare_group_messages(vec![(commit.as_slice(), false)])?;
    caro.context.api().send_group_messages(messages).await?;

    // Alix builds and publishes a commit before it sees Caro's, so its
    // saved commit references Bo's proposal too.
    let intent = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(
            crate::groups::intents::UpdateMetadataIntentData::new_update_group_name("after".into()),
        ))
        .queue(&alix_group)?;
    alix_group.publish_intents().await?;

    // Alix syncs until its intent resolves, and Bo until it has processed
    // as far as Alix.
    let state = || async {
        Fetch::<StoredGroupIntent>::fetch(&alix.context.db(), &intent.id)
            .unwrap()
            .unwrap()
            .state
    };
    xmtp_common::wait_for_eq(
        || async {
            let _ = alix_group.sync().await;
            matches!(
                state().await,
                IntentState::Published | IntentState::ToPublish
            )
        },
        false,
    )
    .await?;
    let topic = StreamTopic::group(alix_group.group_id);
    let processed =
        |group: &EngineTestGroup| group.context.db().topic_progress(&topic).unwrap().processed;
    let target = processed(&alix_group);
    xmtp_common::wait_for_eq(
        || async {
            let _ = bo_group.sync().await;
            processed(&bo_group) >= target
        },
        true,
    )
    .await?;
    for group in [&alix_group, &bo_group] {
        assert_eq!(
            group.epoch().await?,
            epoch,
            "a member applied a rejected commit"
        );
        assert!(
            !pending(group)?.contains(&reference),
            "the evicted proposal is still pending"
        );
    }
    let authenticator = |group: &EngineTestGroup| {
        group.with_group_snapshot(|mls| Ok(mls.epoch_authenticator().as_slice().to_vec()))
    };
    assert_eq!(authenticator(&alix_group)?, authenticator(&bo_group)?);
    assert_eq!(state().await, IntentState::Error);
    alix_group
        .send_message(b"after the race", SendMessageOpts::default())
        .await?;
    xmtp_common::wait_for_eq(
        || async {
            bo_group.sync().await.unwrap();
            bo_group
                .find_messages(&Default::default())
                .unwrap()
                .iter()
                .any(|message| message.decrypted_message_bytes == b"after the race")
        },
        true,
    )
    .await?;
}
