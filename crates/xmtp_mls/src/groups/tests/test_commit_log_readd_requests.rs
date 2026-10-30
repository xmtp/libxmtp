use crate::{
    context::XmtpSharedContext,
    groups::{UpdateAdminListType, commit_log::CommitLogWorker, oneshot::Oneshot},
    tester,
};
use xmtp_db::{consent_record::ConsentState, group::QueryGroup, prelude::QueryReaddStatus};

use xmtp_proto::{
    types::GroupId,
    xmtp::mls::message_contents::{OneshotMessage, ReaddRequest, oneshot_message::MessageType},
};

// verifies: FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn test_readd_installation_succeeds() {
    use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};

    tester!(alix);
    tester!(bo);
    tester!(caro);

    let a_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();

    // Use the creation floor before recording the pre-removal authenticator.
    a_group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;

    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    let b_group = bo.group(&a_group.group_id).unwrap();
    let c_group = caro.group(&a_group.group_id).unwrap();
    b_group.update_consent_state(ConsentState::Allowed)?;

    let bo_installation_id = bo.context.installation_id();
    let prev_authenticator = a_group.epoch_authenticator().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        prev_authenticator
    );
    bo.context
        .db()
        .set_group_commit_log_forked_status(&a_group.group_id, Some(true))?;
    a_group
        .readd_installations(vec![bo_installation_id.to_vec()])
        .await
        .unwrap();
    // Verify the commit was applied without erroring on A and C
    let new_authenticator = a_group.epoch_authenticator().await.unwrap();
    assert_ne!(prev_authenticator, new_authenticator);
    c_group.sync().await.unwrap();
    let c_group_authenticator = c_group.epoch_authenticator().await.unwrap();
    assert_eq!(c_group_authenticator, new_authenticator);

    // Process the removal at the same anchor carried by the replacement Welcome.
    b_group.sync_with_conn().await?;
    assert!(!b_group.is_active()?);
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&a_group.group_id)?,
        Some(true)
    );
    assert_eq!(b_group.epoch().await?, a_group.epoch().await?);
    let anchor = alix
        .context
        .db()
        .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage])?;
    assert_eq!(
        bo.context
            .db()
            .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage],)?,
        anchor
    );
    assert_eq!(b_group.epoch_authenticator().await?, prev_authenticator);

    // Model an older Welcome published after the replacement Welcome.
    // This fixture changes its stored ID; it does not delay network publication.
    let delayed_welcome_sequence = i64::MAX;
    let mut stored_group = bo.context.db().find_group(&a_group.group_id)?.unwrap();
    stored_group.sequence_id = Some(delayed_welcome_sequence);
    assert_eq!(
        bo.context
            .db()
            .insert_or_replace_group(stored_group)?
            .sequence_id,
        Some(delayed_welcome_sequence)
    );

    // Verify welcome was received and applied on B
    tracing::warn!("Syncing welcomes");
    bo.sync_welcomes().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        new_authenticator
    );
    assert!(b_group.is_active()?);
    assert_eq!(b_group.consent_state()?, ConsentState::Allowed);
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&a_group.group_id)?,
        None
    );
    assert_eq!(b_group.epoch().await?, a_group.epoch().await?);
    b_group.with_group_snapshot(|mls_group| {
        assert!(
            mls_group.pending_proposals().next().is_none(),
            "a replacement Welcome must not restore old-epoch proposals"
        );
        Ok(())
    })?;
    assert_eq!(
        bo.context
            .db()
            .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage])?,
        anchor
    );
    a_group.test_can_talk_with(&b_group).await?;
    b_group.test_can_talk_with(&a_group).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_readd_bookkeeping() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(devon, disable_workers);
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), devon.inbox_id()],
            None,
            None,
        )
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .unwrap();
    // An admin is not a permitted readder.
    group
        .update_admin_list(UpdateAdminListType::Add, devon.inbox_id().to_string())
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    devon.sync_all_welcomes_and_groups(None).await.unwrap();
    let b_group = bo.group(&group.group_id).unwrap();
    b_group.update_consent_state(ConsentState::Allowed).unwrap();
    let c_group = caro.group(&group.group_id).unwrap();
    c_group.update_consent_state(ConsentState::Allowed).unwrap();
    let d_group = devon.group(&group.group_id).unwrap();
    d_group.update_consent_state(ConsentState::Allowed).unwrap();

    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    // Publishes remote commit log (needed for readd request to be sent)
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    let c_conn = caro.context.db();
    let d_conn = devon.context.db();
    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&group.group_id, Some(true))
        .unwrap();

    let alix_installation = alix.context.installation_id();
    let bo_installation = bo.context.installation_id();
    let assert_awaiting = |expected: &[(&dyn QueryReaddStatus, &[u8], bool)]| {
        for (i, (conn, installation, awaiting)) in expected.iter().enumerate() {
            assert_eq!(
                conn.is_awaiting_readd(&group.group_id, installation)
                    .unwrap(),
                *awaiting,
                "row {i}"
            );
        }
    };
    // No readd requests yet
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], false),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
    ]);

    // Should trigger readd requests to be sent
    a_worker._tick().await.unwrap();
    // Should no-op
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    alix.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    devon.sync_welcomes().await.unwrap();

    // Only the requester and super admins record Alix's request; nobody records Bo
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], true),
        (&b_conn, &alix_installation[..], true),
        (&c_conn, &alix_installation[..], true),
        (&d_conn, &alix_installation[..], false),
        (&a_conn, &bo_installation[..], false),
        (&b_conn, &bo_installation[..], false),
        (&c_conn, &bo_installation[..], false),
    ]);

    // Let's say Bo's worker processes it first
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    b_worker._tick().await.unwrap();

    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    devon.sync_all_welcomes_and_groups(None).await.unwrap();
    // Apply the removal, but withhold the Welcome that completes our request.
    group.sync_with_conn().await?;
    assert!(!group.is_active()?);
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], true),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
        (&d_conn, &alix_installation[..], false),
    ]);
    a_worker._tick().await?; // Drain the removal's log upload.
    let published = alix.api_stats().publish.get_count();
    for _ in 0..2 {
        a_worker._tick().await?;
        assert_eq!(alix.api_stats().publish.get_count(), published);
    }
    alix.sync_welcomes().await?;
    assert!(group.is_active()?);
    assert_eq!(
        group.epoch_authenticator().await?,
        b_group.epoch_authenticator().await?
    );

    // Everyone should see that Alix is no longer awaiting readd
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], false),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
        (&d_conn, &alix_installation[..], false),
    ]);

    b_group.key_update().await?;
    group.sync_with_conn().await?;
    b_worker._tick().await?;
    a_worker._tick().await?;
    let published = alix.api_stats().publish.get_count();
    a_conn.set_group_commit_log_forked_status(&group.group_id, Some(true))?;
    a_worker._tick().await?;
    assert!(alix.api_stats().publish.get_count() > published);
    assert!(a_conn.is_awaiting_readd(&group.group_id, alix_installation.as_slice())?);
}

#[xmtp_common::test]
async fn test_request_readd_with_allowlisted_groups() {
    // Step 1: Bo creates a group
    tester!(bo);
    tester!(caro);

    let group = bo
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await
        .unwrap();

    let group_id = group.group_id;
    let group_id_typed: GroupId = group_id;
    let group_id_hex = hex::encode(group_id);
    let unnormalized_group_id = "0x".to_owned() + &group_id_hex.to_uppercase();

    // Step 2: Create Alix with that group ID in the allowlist
    tester!(alix, enable_fork_recovery_requests_for: vec![unnormalized_group_id]);

    // Step 3: Bo adds Alix to the group
    group
        .add_members(&[alix.inbox_id().to_string()])
        .await
        .unwrap();

    alix.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();

    // Fork detection and recovery does not operate on non-consented groups
    let a_group = alix.group(&group_id).unwrap();
    a_group.update_consent_state(ConsentState::Allowed).unwrap();
    let c_group = caro.group(&group_id).unwrap();
    c_group.update_consent_state(ConsentState::Allowed).unwrap();

    // Upload remote commit log on Bo's end
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    b_worker._tick().await.unwrap();
    // Download remote commit log on Alix's end - we need the latest remote commit sequence ID for readd request to be sent
    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    let c_conn = caro.context.db();

    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&group_id, Some(true))
        .unwrap();

    // No readd requests yet
    assert!(
        !a_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );

    // Should trigger readd requests to be sent for this allowlisted group
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    // Alix should have recorded a readd request since the group is allowlisted
    assert!(
        a_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Bo is a superadmin so should have recorded the request
    assert!(
        b_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Caro is not a superadmin so should not have received the request
    assert!(
        !c_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
}

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
    use crate::utils::test_mocks_helpers::{
        RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
    };
    use openmls::prelude::{AppDataUpdateOperation, Proposal};
    use tls_codec::Serialize;
    use xmtp_db::{
        TransactionOutcome::Continue, XmtpOpenMlsProviderRef, prelude::QueryRefreshState,
        refresh_state::EntityKind,
    };
    use xmtp_db::{group_intent::IntentKind, prelude::QueryGroupIntent};
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_mls_validation::commit::extract_group_membership;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
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
        use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
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
        let proposal = crate::state_tx::state_write(alix.context.mls_storage(), |tx| {
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
    let removal = crate::state_tx::state_write(alix.context.mls_storage(), |tx| {
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

// verifies: FORK-050, FORK-053, FORK-080, GMOD-052, FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_peer_answers_a_readd_request() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(bo, disable_workers);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    assert!(bo_dm.super_admin_list()?.is_empty());
    // Give both members a post-Welcome commit to compare before recovery.
    dm.key_update().await?;
    bo_dm.sync().await?;

    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    alix_worker._tick().await?;
    let alix_db = alix.context.db();
    let bo_db = bo.context.db();
    let installation = alix.context.installation_id();
    let epoch_before = bo_dm.epoch().await?;
    assert!(!alix_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(!bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    // Simulate fork detection without creating divergent MLS epoch secrets.
    alix_db.set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(alix_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    let requested_at = alix_db
        .get_readd_status(&dm.group_id, installation.as_slice())?
        .unwrap()
        .requested_at_sequence_id;
    assert!(requested_at.is_some());
    // A second requester tick keeps the original request while the peer has not answered.
    alix_worker._tick().await?;
    alix.sync_welcomes().await?;
    bo.sync_welcomes().await?;
    for db in [&alix_db, &bo_db] {
        assert_eq!(
            db.get_readd_status(&dm.group_id, installation.as_slice())?
                .unwrap()
                .requested_at_sequence_id,
            requested_at
        );
        assert!(!db.is_awaiting_readd(&dm.group_id, bo.context.installation_id().as_slice())?);
    }

    let mut bo_worker = CommitLogWorker::new(bo.context.clone());
    for _ in 0..3 {
        bo_worker._tick().await?;
        if !bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())? {
            break;
        }
    }
    let status = bo_db
        .get_readd_status(&dm.group_id, installation.as_slice())?
        .expect("the response retains its bookkeeping row");
    assert!(status.responded_at_sequence_id.is_some());
    assert!(!bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(bo_dm.epoch().await? > epoch_before);
    dm.sync_with_conn().await?;
    assert!(!dm.is_active()?);
    alix.sync_welcomes().await?;
    assert!(dm.is_active()?);
    assert_eq!(
        dm.epoch_authenticator().await?,
        bo_dm.epoch_authenticator().await?
    );
    assert_eq!(
        alix_db.get_group_commit_log_forked_status(&dm.group_id)?,
        None
    );
    let repaired_epoch = dm.epoch().await?;
    alix_worker._tick().await?;
    bo_worker._tick().await?;
    assert_eq!(dm.epoch().await?, repaired_epoch);
    assert_eq!(bo_dm.epoch().await?, repaired_epoch);
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

    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(bo, disable_workers);
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

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
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

// verifies: FORK-080
#[xmtp_common::test(unwrap_try = true)]
async fn a_readder_promoted_in_an_unsynced_commit_answers() {
    use xmtp_db::{
        prelude::{QueryGroupVersion, QueryRefreshState},
        refresh_state::EntityKind,
    };

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;
    // Bo has not processed its promotion when the request arrives.
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let cursor = alix
        .context
        .db()
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    Oneshot::send_message(
        caro.context.clone(),
        vec![bo.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: cursor,
            })),
        },
    )
    .await?;
    bo.sync_welcomes().await?;
    let db = bo.context.db();
    let installation = caro.context.installation_id();
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    let mut worker = CommitLogWorker::new(bo.context.clone());
    // A failed sync must not judge permission from the unsynced state.
    db.set_group_paused(&group.group_id, "999.0.0")?;
    worker._tick().await?;
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    db.unpause_group(&group.group_id)?;
    let epoch_before = group.epoch().await?;
    for _ in 0..3 {
        worker._tick().await?;
        if !db.is_awaiting_readd(&group.group_id, installation.as_slice())? {
            break;
        }
    }
    assert_eq!(bo_group.epoch().await?, epoch_before + 1);
    assert!(!db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
}

// verifies: FORK-050, FORK-080, FORK-064, GMOD-052
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_readd_is_answered_once_by_peer_or_sibling() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(alix2, from: alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    dm.update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    alix2.sync_all_welcomes_and_groups(None).await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let sibling_dm = alix2.group(&dm.group_id)?;
    let bo_dm = bo.group(&dm.group_id)?;
    sibling_dm.update_consent_state(ConsentState::Allowed)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    dm.key_update().await?;
    sibling_dm.sync().await?;
    bo_dm.sync().await?;
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    alix_worker._tick().await?;
    alix.context
        .db()
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    alix2.sync_welcomes().await?;
    bo.sync_welcomes().await?;
    let installation = alix.context.installation_id();
    assert!(
        alix2
            .context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    assert!(
        bo.context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    let epoch_before = dm.epoch().await?;
    let mut sibling_worker = CommitLogWorker::new(alix2.context.clone());
    sibling_worker._tick().await?;
    assert_eq!(sibling_dm.epoch().await?, epoch_before + 1);
    assert!(
        !alix2
            .context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    let mut bo_worker = CommitLogWorker::new(bo.context.clone());
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before + 1);
    assert!(
        !bo.context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    assert!(
        bo.context
            .db()
            .get_readd_status(&dm.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id
            .is_some()
    );
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before + 1);
}

// verifies: FORK-080
#[rstest::rstest]
#[case::signed_max(i64::MAX as u64)]
#[case::unsigned_max(u64::MAX)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_future_readd_claim_publishes_no_commit(
    #[case] claimed: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    assert!(group.super_admin_list()?.is_empty());
    group.key_update().await?;
    bo_group.sync().await?;
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    Oneshot::send_message(
        bo.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: claimed,
            })),
        },
    )
    .await?;
    alix.sync_welcomes().await?;
    let installation = bo.context.installation_id();
    let epoch_before = group.epoch().await?;
    for _ in 0..3 {
        worker._tick().await?;
        assert_eq!(group.epoch().await?, epoch_before);
        let status = alix
            .context
            .db()
            .get_readd_status(&group.group_id, installation.as_slice())?
            .expect("the responder must retain a future request");
        assert_eq!(status.requested_at_sequence_id, Some(claimed as i64));
        assert_eq!(status.responded_at_sequence_id, None);
        assert!(
            alix.context
                .db()
                .get_readds_awaiting_response(
                    &group.group_id,
                    alix.context.installation_id().as_slice(),
                )?
                .iter()
                .any(|pending| pending.installation_id == installation.as_slice())
        );
    }
    Ok(())
}

// verifies: FORK-080, FORK-064
#[xmtp_common::test(unwrap_try = true)]
async fn a_claim_at_the_processed_cursor_is_answered_once() {
    use xmtp_db::{
        prelude::{QueryLocalCommitLog, QueryRefreshState},
        refresh_state::EntityKind,
    };

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    group
        .send_message(b"advance the processed cursor", Default::default())
        .await?;
    let db = alix.context.db();
    let cursor = db
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    assert!(
        cursor
            > db.get_latest_log_for_group(&group.group_id)?
                .unwrap()
                .commit_sequence_id as u64
    );
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    Oneshot::send_message(
        bo.context.clone(),
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
    let epoch_before = group.epoch().await?;
    worker._tick().await?;
    assert_eq!(group.epoch().await?, epoch_before + 1);
    let installation = bo.context.installation_id();
    assert!(!db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    assert!(
        db.get_readd_status(&group.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id
            .unwrap()
            > cursor as i64
    );
    for _ in 0..2 {
        worker._tick().await?;
        assert_eq!(group.epoch().await?, epoch_before + 1);
    }
}
