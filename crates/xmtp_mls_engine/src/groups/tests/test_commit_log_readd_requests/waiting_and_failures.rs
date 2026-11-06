//! Commit log readd request tests: waiting and failures.

use super::*;

// verifies: FORK-062, FORK-080
#[rstest::rstest]
#[case::no_prior_failure(false)]
#[case::historical_failure(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_readd_without_a_usable_key_package_waits(
    #[case] prior_failure: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::{
        GroupError,
        intents::{QueueIntent, ReaddInstallationsIntentData},
        mls_sync::{note_retries, retry_due},
    };
    use openmls::prelude::{AppDataUpdateOperation, Proposal};
    use tls_codec::Serialize;
    use xmtp_db::{
        TransactionOutcome::Continue, XmtpOpenMlsProviderRef, prelude::QueryRefreshState,
        refresh_state::EntityKind,
    };
    use xmtp_db::{group_intent::IntentKind, prelude::QueryGroupIntent};
    use xmtp_mls_group_state::app_data::component_id::ComponentId;
    use xmtp_mls_identity::identity_lifecycle::test_mocks::{
        RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
    };
    use xmtp_mls_validation::commit::extract_group_membership;
    use xmtp_proto::types::Topic;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    let installation = bo.context.installation_id();

    if prior_failure {
        // Older commits may leave a live leaf also listed as failed.
        use prost::Message;
        use xmtp_mls_group_state::inbox_id::InboxId;
        use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
        use xmtp_proto::xmtp::mls::message_contents::{
            GroupMembershipEntry, group_membership_entry,
        };
        let sequence_id = group.with_group_snapshot(|mls| {
            Ok(extract_group_membership(mls.extensions())?.members[bo.inbox_id()])
        })?;
        let entry = GroupMembershipEntry {
            version: Some(group_membership_entry::Version::V1(
                group_membership_entry::V1 {
                    sequence_id,
                    failed_installations: vec![installation.to_vec()],
                },
            )),
        };
        let payload = TlsMapDelta::new()
            .update(
                InboxId::from_hex(bo.inbox_id())?,
                tls_codec::VLBytes::new(entry.encode_to_vec()),
            )
            .tls_serialize_detached()?;
        let proposal = xmtp_mls_core::state_tx::state_write(alix.context.mls_storage(), |tx| {
            tx.with_group(group.group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let (message, _) = mls
                    .propose_app_data_update(
                        &provider,
                        &alix.context.identity().installation_keys,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(payload.into()),
                    )
                    .map_err(GroupError::Proposal)?;
                Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
            })
        })?
        .into_continued();
        let messages = group.prepare_group_messages(vec![(proposal.as_slice(), false)])?;
        let receipts = alix.context.api().send_group_messages(messages).await?;
        let topic = Topic::new_group_message(group.group_id);
        let (_, cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
        crate::subscriptions::barrier::wait_through(&alix.context, [(topic, cursor)].into(), None)
            .await?;
        let intent = QueueIntent::commit_pending_proposals().queue(&group)?;
        group.sync_until_intent_resolved(intent.id).await?;
        assert!(group.with_group_snapshot(|mls| {
            Ok(extract_group_membership(mls.extensions())?
                .failed_installations
                .contains(&installation.to_vec()))
        })?);
    }
    bo.sync_all_welcomes_and_groups(None).await?;
    caro.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;

    // An admitted standalone Remove must not bypass the usable-package gate.
    let removal = xmtp_mls_core::state_tx::state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let leaf = mls
                .members()
                .find(|member| member.signature_key == installation.as_slice())
                .unwrap()
                .index;
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let (message, _) = mls
                .propose_remove_member(&provider, &alix.context.identity().installation_keys, leaf)
                .map_err(GroupError::ProposeRemoveMember)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = group.prepare_group_messages(vec![(removal.as_slice(), false)])?;
    let receipts = alix.context.api().send_group_messages(messages).await?;
    let topic = Topic::new_group_message(group.group_id);
    let (_, proposal_cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    crate::subscriptions::barrier::wait_through(
        &alix.context,
        [(topic, proposal_cursor)].into(),
        None,
    )
    .await?;
    assert!(group.with_group_snapshot(|mls| {
        Ok(mls
            .pending_proposals()
            .any(|proposal| matches!(proposal.proposal(), Proposal::Remove(_))))
    })?);
    let cursor = alix
        .context
        .db()
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    for requester in [&bo, &caro] {
        Oneshot::send_message(
            requester.context.clone(),
            vec![alix.inbox_id()],
            OneshotMessage {
                message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                    group_id: group.group_id.to_vec(),
                    latest_commit_sequence_id: cursor,
                })),
            },
        )
        .await?;
    }
    alix.sync_welcomes().await?;
    let before_epoch = group.epoch().await?;
    let before_membership = group.with_group_snapshot(|mls| {
        Ok(mls
            .extensions()
            .app_data_dictionary()
            .unwrap()
            .dictionary()
            .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
            .unwrap()
            .to_vec())
    })?;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![installation.to_vec()]));
    let restore = RestoreKeyPackages;
    let failed_at = xmtp_common::time::now_ns();
    worker._tick().await?;
    assert_eq!(
        group.epoch().await?,
        before_epoch + 1,
        "Caro's repair must progress"
    );
    group.with_group_snapshot(|mls| {
        assert_eq!(mls.members().count(), 3);
        assert!(
            mls.members()
                .any(|member| member.signature_key == installation.as_slice())
        );
        assert_eq!(
            mls.extensions()
                .app_data_dictionary()
                .unwrap()
                .dictionary()
                .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
                .unwrap(),
            before_membership.as_slice()
        );
        Ok(())
    })?;
    let db = alix.context.db();
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    assert_eq!(
        db.get_readd_status(&group.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id,
        None
    );
    assert!(!db.is_awaiting_readd(&group.group_id, caro.context.installation_id().as_slice())?);
    assert!(
        !retry_due(installation.as_slice(), failed_at),
        "the failed fetch is recorded"
    );
    caro_group.sync_with_conn().await?;
    caro.sync_welcomes().await?;
    assert_eq!(
        caro_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
    bo_group.sync().await?;
    assert!(bo_group.is_active()?);

    // A backed-off requester waits; another requester is still answered.
    let bo_key = [installation.to_vec()];
    note_retries(&bo_key.clone().into(), &bo_key, i64::MAX);
    let cursor = db
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    Oneshot::send_message(
        caro.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: cursor,
            })),
        },
    )
    .await?;
    alix.sync_welcomes().await?;
    // Each readd commit takes one tick to reach the remote commit log.
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 1);
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 2);
    let kinds = Some(vec![IntentKind::ReaddInstallations]);
    let intents = db.find_group_intents(group.group_id, None, kinds)?;
    let intent = ReaddInstallationsIntentData::try_from(intents.last().unwrap().data.as_slice())?;
    let caro_key = caro.context.installation_id().to_vec();
    assert_eq!(intent.readded_installations, vec![caro_key]);
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    note_retries(&bo_key.into(), &[], 0);
    drop(restore);
    bo.rotate_and_upload_key_package().await?;
    worker._tick().await?;
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 3);
    assert!(!db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    bo_group.sync_with_conn().await?;
    bo.sync_welcomes().await?;
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 3);
    Ok(())
}

enum ReversibleFailure {
    Paused,
    Forked,
    ForkedThenMatchingCommit,
    ReaddFails,
}

// verifies: FORK-041, FORK-080
#[rstest::rstest]
#[case::paused(ReversibleFailure::Paused)]
#[case::forked(ReversibleFailure::Forked)]
#[case::forked_then_matching_commit(ReversibleFailure::ForkedThenMatchingCommit)]
#[case::readd_fails(ReversibleFailure::ReaddFails)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_reversible_failure_keeps_the_readd_request(
    #[case] failure: ReversibleFailure,
) -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_db::{ConnectionExt, diesel::RunQueryDsl, prelude::QueryGroupVersion};

    let alix = EngineTestPeer::new_with(|o| o.enable_fork_recovery_requests()).await;
    let bo = EngineTestPeer::new().await;
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    dm.key_update().await?;
    bo_dm.sync().await?;
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    let mut bo_worker = CommitLogWorker::new(bo.context.clone());
    alix_worker._tick().await?;
    bo_worker._tick().await?;
    let bo_db = bo.context.db();
    let installation = alix.context.installation_id();
    alix.context
        .db()
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);

    match failure {
        ReversibleFailure::Paused => bo_db.set_group_paused(&dm.group_id, "999.0.0")?,
        ReversibleFailure::Forked => {
            bo_db.set_group_commit_log_forked_status(&dm.group_id, Some(true))?
        }
        ReversibleFailure::ForkedThenMatchingCommit => {
            bo_db.set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
            dm.key_update().await?;
            alix_worker._tick().await?;
            bo_dm.sync().await?;
        }
        // Validation queues no intent, so only the readd hits this.
        ReversibleFailure::ReaddFails => bo_db
            .raw_query(|conn| {
                diesel::sql_query(
                    "CREATE TRIGGER fail_intent BEFORE INSERT ON group_intents \
                 BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END",
                )
                .execute(conn)
            })
            .map(drop)?,
    }
    let epoch_before = bo_dm.epoch().await?;
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before);
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);

    match failure {
        ReversibleFailure::Paused => bo_db.unpause_group(&dm.group_id)?,
        ReversibleFailure::ReaddFails => {
            bo_db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_intent").execute(conn))?;
        }
        // Stands in for the reset a Welcome applies.
        _ => bo_db.set_group_commit_log_forked_status(&dm.group_id, Some(false))?,
    }
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before + 1);
    assert!(!bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    Ok(())
}

// verifies: FORK-080
#[xmtp_common::test(unwrap_try = true)]
async fn a_moot_readd_request_drops_while_paused() {
    use xmtp_db::prelude::QueryGroupVersion;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    Oneshot::send_message(
        alix.context.clone(),
        vec![bo.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: dm.group_id.to_vec(),
                latest_commit_sequence_id: 0,
            })),
        },
    )
    .await?;
    bo.sync_welcomes().await?;
    let db = bo.context.db();
    let installation = alix.context.installation_id();
    assert!(db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    db.set_group_paused(&dm.group_id, "999.0.0")?;
    bo_dm.update_consent_state(ConsentState::Denied)?;
    CommitLogWorker::new(bo.context.clone())._tick().await?;
    assert!(!db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
}
