//! Minimum protocol version gating and pause behaviour.

use super::*;
use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer, EngineTestPeerOps};

#[xmtp_common::test(unwrap_try = true)]
fn test_increment_patch_version() {
    assert_eq!(increment_patch_version("1.2.3"), Some("1.2.4".to_string()));
    assert_eq!(increment_patch_version("0.0.9"), Some("0.0.10".to_string()));
    assert_eq!(increment_patch_version("1.0.0"), Some("1.0.1".to_string()));
    assert_eq!(
        increment_patch_version("1.0.0-alpha"),
        Some("1.0.1-alpha".to_string())
    );

    // Invalid inputs should return None
    assert_eq!(increment_patch_version("1.2"), None);
    assert_eq!(increment_patch_version("1.2.3.4"), None);
    assert_eq!(increment_patch_version("invalid"), None);
}

/// Send-side mirror of the receive-side downgrade check. A newly created
/// group already has the proposals protocol floor, so a lower request fails
/// before it queues an AppDataUpdate.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_downgrade() {
    use crate::groups::GroupError;

    let alix = EngineTestPeer::new().await;

    let alix_group = alix.create_group(None, None)?;
    let current = xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION;
    let err = alix_group
        .update_group_min_version("0.0.0")
        .await
        .expect_err("downgrade must be rejected by the send-side guard");
    assert!(
        matches!(
            err,
            GroupError::MinVersionDowngrade { ref requested, current: ref existing }
            if requested == "0.0.0" && existing == current
        ),
        "expected MinVersionDowngrade, got {err:?}",
    );
}

/// `update_group_min_version` surfaces an unparseable input as a clean
/// `InvalidMinVersion` error rather than leaking commit validation details.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_malformed_input() {
    use crate::groups::GroupError;

    let alix = EngineTestPeer::new().await;

    let alix_group = alix.create_group(None, None)?;
    let err = alix_group
        .update_group_min_version("not-a-version")
        .await
        .expect_err("malformed semver must be rejected");
    assert!(
        matches!(
            err,
            GroupError::InvalidMinVersion { ref value, .. } if value == "not-a-version"
        ),
        "expected InvalidMinVersion, got {err:?}",
    );
}

/// The steady-state bump path also refuses values above the client's own
/// version.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_above_own() {
    use crate::groups::GroupError;

    let alix = EngineTestPeer::new().await;

    let alix_group = alix.create_group(None, None)?;
    let err = alix_group
        .update_group_min_version("99.0.0")
        .await
        .expect_err("min_version above own pkg_version must be rejected");
    assert!(
        matches!(
            err,
            GroupError::MinVersionExceedsOwnVersion { ref requested, .. }
            if requested == "99.0.0"
        ),
        "expected MinVersionExceedsOwnVersion, got {err:?}",
    );
}

/// A DM participant publishes a floor that differs from its peer's version
/// only in build metadata. Build metadata has no SemVer precedence, so the
/// peer keeps processing and sending.
// verifies: GMOD-025, GMOD-027
#[xmtp_common::test(unwrap_try = true)]
async fn build_metadata_floor_does_not_pause_same_precedence_dm_peer() {
    let peer_version = VersionInfo::default().pkg_version().to_string();
    let floor = format!("{peer_version}+octane.1");
    let mut participant_version = VersionInfo::default();
    participant_version.test_update_version(&floor);
    let victim = EngineTestPeer::new().await;
    let participant = EngineTestPeer::new_with(|o| o.version_info = participant_version).await;

    // The unsuffixed client creates the DM. The suffixed client is an
    // ordinary participant, not its creator or super admin.
    let victim_dm = victim
        .find_or_create_dm(participant.inbox_id(), None)
        .await?;
    participant.wait_for_welcomes().await?;
    let participant_dm = participant.group(&victim_dm.group_id)?;
    participant_dm.update_group_min_version(&floor).await?;
    participant_dm
        .send_message(b"message after build floor", SendMessageOpts::default())
        .await?;

    // One sync can miss a fresh write (the newest position lags), so poll.
    // A paused peer holds the message back until the timeout.
    let delivered = xmtp_common::wait_for_some(|| async {
        let _ = victim_dm.sync().await;
        let messages = victim_dm.find_messages(&MsgQueryArgs::default()).unwrap();
        messages
            .into_iter()
            .find(|message| message.decrypted_message_bytes == b"message after build floor")
    })
    .await;
    assert!(delivered.is_some(), "the peer must store the later message");
    // The committed floor keeps its build metadata.
    let committed = victim_dm.mutable_metadata()?;
    assert_eq!(
        committed
            .attributes()
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string()),
        Some(&floor)
    );
    assert_eq!(victim_dm.paused_for_version()?, None);
    victim_dm
        .send_message(b"reply", SendMessageOpts::default())
        .await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_can_set_min_supported_protocol_version_for_commit() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let amal = EngineTestPeer::new_with(|o| o.version_info = amal_version.clone()).await;
    let bo = EngineTestPeer::new().await;

    // ensure the version is as expected
    assert!(bo.context.version_info() != &amal_version);
    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal updates the group name and sends a message to the group
    amal_group
        .update_group_name("new name".to_string())
        .await
        .unwrap();
    amal_group
        .send_message("Hello, world!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message even though they are on different client versions
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);

    let message_text = String::from_utf8_lossy(&messages[2].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group version to match their client version
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 6: Bo should now be unable to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);

    // Step 7: Bo updates their client, and see if we can then download latest messages
    let mut bo_version = bo.version_info().clone();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let bo = bo
        .with_options(|o| o.version_info = bo_version.clone())
        .await;

    assert_eq!(bo.context.version_info(), amal.context.version_info());

    // Refresh Bo's group context
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();

    bo_group.sync().await.unwrap();
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 5);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_only_super_admins_can_set_min_supported_protocol_version() {
    let amal = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();
    amal_group
        .update_admin_list(
            UpdateAdminListType::Add,
            bo.context.identity.inbox_id().to_string(),
        )
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    let is_bo_admin = amal_group
        .is_admin(bo.context.identity.inbox_id().to_string())
        .unwrap();
    assert!(is_bo_admin);

    let is_bo_super_admin = amal_group
        .is_super_admin(bo.context.identity.inbox_id().to_string())
        .unwrap();
    assert!(!is_bo_super_admin);

    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes()
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(
        min_version.map(String::as_str),
        Some(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
    );

    let result = bo_group.update_group_min_version_to_match_self().await;
    assert!(result.is_err());
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes()
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(
        min_version.map(String::as_str),
        Some(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
    );

    amal_group.sync().await.unwrap();
    let result = amal_group.update_group_min_version_to_match_self().await;
    assert!(result.is_ok());
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes()
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version.unwrap(), amal.version_info().pkg_version());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message_while_paused_after_welcome_returns_expected_error() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    // Create two clients with different versions
    let amal = EngineTestPeer::new_with(|o| {
        o.version_info = amal_version;
        o.triggers = true;
    })
    .await;

    let bo = EngineTestPeer::new().await;

    // Amal creates a group and adds bo
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Amal sets minimum version requirement
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    let envelopes = amal
        .context
        .api()
        .query_group_messages(amal_group.group_id)
        .await
        .unwrap();
    assert!(envelopes.last().unwrap().is_commit());
    let predecessor = envelopes.iter().rev().nth(1).unwrap().cursor;

    // Bo joins group and attempts to send message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    let topic = Topic::new_group_message(bo_group.group_id);
    let db_topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let processed = bo.context.db().topic_progress(&db_topic).unwrap().processed;
    let before = bo_group.epoch_authenticator().await.unwrap();

    // The send cannot publish or process through the unsupported version bump.
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    let Err(GroupError::SyncFailedToWait(summary)) = result else {
        panic!("expected an unpublished blocked send, got {result:?}");
    };
    let Some(GroupError::StreamBarrier(error)) = summary.other.as_deref() else {
        panic!("expected the unsupported version barrier, got {summary:?}");
    };
    assert!(predecessor >= processed);
    assert_blocked_obligation(error, &topic, predecessor, "unsupported_protocol_version");
    assert_eq!(bo_group.epoch_authenticator().await.unwrap(), before);
    assert_eq!(
        bo.context.db().topic_progress(&db_topic).unwrap().processed,
        predecessor
    );

    assert_paused_sync(
        bo_group.sync().await.unwrap_err(),
        amal.version_info().pkg_version(),
    );

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message_after_min_version_update_gets_expected_error() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    // Create two clients with different versions
    let amal = EngineTestPeer::new_with(|o| {
        o.version_info = amal_version.clone();
        o.triggers = true;
    })
    .await;
    assert!(amal.context.version_info() != &VersionInfo::default());
    let bo = EngineTestPeer::new_with(|o| o.triggers = true).await;

    // Amal creates a group and adds bo
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Bo joins group and successfully sends initial message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    let topic = Topic::new_group_message(bo_group.group_id);
    let db_topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    let processed = bo.context.db().topic_progress(&db_topic).unwrap().processed;
    let before = bo_group.epoch_authenticator().await.unwrap();

    // Amal sets new minimum version requirement
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // The backend accepts Bo's send, but the unsupported prefix prevents confirmation.
    let envelopes = amal
        .context
        .api()
        .query_group_messages(amal_group.group_id)
        .await
        .unwrap();
    let commit = envelopes.last().unwrap();
    assert!(commit.is_commit());
    let blocked_cursor = commit.cursor;
    let predecessor = envelopes.iter().rev().nth(1).unwrap().cursor;
    assert!(predecessor >= processed);

    let result = bo_group
        .send_message(
            "Second message from Bo".as_bytes(),
            SendMessageOpts::default(),
        )
        .await;
    let Err(GroupError::PublishedButUnconfirmed {
        intent_id,
        cause: Some(error),
        ..
    }) = result
    else {
        panic!("expected an accepted send with a blocked processing obligation, got {result:?}");
    };
    assert_blocked_obligation(&error, &topic, predecessor, "unsupported_protocol_version");
    assert!(
        bo.context
            .db()
            .prepared_envelopes(intent_id)
            .unwrap()
            .is_some()
    );
    assert_eq!(bo_group.epoch_authenticator().await.unwrap(), before);
    assert_eq!(
        bo.context.db().topic_progress(&db_topic).unwrap().processed,
        predecessor
    );
    let pending = bo
        .context
        .db()
        .first_pending_envelope(&db_topic)
        .unwrap()
        .unwrap();
    assert_eq!(pending.sequence_id as u64, blocked_cursor.0);
    assert!(pending.blocked);
    assert_eq!(
        pending.error_code.as_deref(),
        Some("unsupported_protocol_version")
    );
    // The AppDataUpdate proposal is processed before its commit blocks.
    // It does not advance the epoch and does not need a rejection record.

    assert_paused_sync(
        bo_group.sync().await.unwrap_err(),
        amal.version_info().pkg_version(),
    );

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }

    // Verify Bo can send again after updating their version
    let mut bo_version = bo.version_info().clone();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let bo = bo.with_options(|o| o.version_info = bo_version).await;

    // Need to get fresh group reference after version update
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    // Should now succeed
    let result = bo_group
        .send_message(
            "Message after update".as_bytes(),
            SendMessageOpts::default(),
        )
        .await;
    assert!(result.is_ok());
}

/// A new group records the floor `8.0.0-0`. A client that reports
/// `1.12.0-dev.1`, as `8.0.0-rc4` and earlier builds of `main` did, holds the
/// group's Welcome. A client that reports `8.0.0-rc5` joins and reads a
/// message.
// verifies: GMOD-025, JOIN-057
#[xmtp_common::test(unwrap_try = true)]
async fn rc4_client_holds_welcome_and_rc5_client_joins() {
    let reporting = |version: &str| {
        let mut info = VersionInfo::default();
        info.test_update_version(version);
        info
    };
    let alix = EngineTestPeer::new().await;
    let rc4 = EngineTestPeer::new_with(|o| o.version_info = reporting("1.12.0-dev.1")).await;
    let rc5 = EngineTestPeer::new_with(|o| o.version_info = reporting("8.0.0-rc5")).await;
    let group = alix
        .create_group_with_members(&[rc4.inbox_id(), rc5.inbox_id()], None, None)
        .await?;
    group
        .send_message(b"for 8.0.0 builds", SendMessageOpts::default())
        .await?;

    let rc5_group = rc5.wait_for_welcomes().await?.first()?.clone();
    // One sync can miss a fresh write, so poll.
    let delivered = xmtp_common::wait_for_some(|| async {
        let _ = rc5_group.sync().await;
        rc5_group
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .into_iter()
            .find(|message| message.decrypted_message_bytes == b"for 8.0.0 builds")
    })
    .await;
    assert!(delivered.is_some(), "the rc5 client must read the message");

    xmtp_common::wait_for_eq(
        || async {
            let _ = rc4.sync_welcomes().await;
            rc4.context.db().has_pending_welcomes().unwrap()
        },
        true,
    )
    .await?;
    assert!(
        !rc4.group(&group.group_id)
            .is_ok_and(|group| group.is_active().unwrap_or(false)),
        "the rc4 client must not join"
    );
}

/// While the committed floor is above this client's version, a standalone
/// `GroupContextExtensions` proposal and a handshake beyond the sender's
/// forward window are held, not rejected: a client at the floor may accept
/// them.
// verifies: GMOD-059, PROC-061
#[rstest::rstest]
#[case::unsupported_proposal(false, "unsupported_protocol_version")]
#[case::far_forward_handshake(true, "mls_processing_failure")]
#[xmtp_common::test(unwrap_try = true)]
async fn floor_above_own_version_holds_unsupported_and_far_forward(
    #[case] far_forward: bool,
    #[case] held_code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use openmls::prelude::{LeafNodeParameters, tls_codec::Serialize as _};
    use xmtp_db::{
        TransactionOutcome::Continue,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    };
    use xmtp_mls_core::state_tx::state_write;
    use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

    let mut newer = VersionInfo::default();
    newer.test_update_version(
        increment_patch_version(newer.pkg_version())
            .unwrap()
            .as_str(),
    );
    let alix = EngineTestPeer::new_with(|o| o.version_info = newer.clone()).await;
    let bo = EngineTestPeer::new_with(|o| o.version_info = newer.clone()).await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    alix_group.update_group_min_version_to_match_self().await?;
    bo_group.receive().await?;
    // Bo restarts on an older build, below the floor it already applied.
    let bo = bo
        .with_options(|o| o.version_info = VersionInfo::default())
        .await;
    let bo_group = bo.find_groups(GroupQueryArgs::default())?[0].clone();

    let signer = alix.context.identity().installation_keys.clone();
    let message = state_write(alix.context.mls_storage(), |tx| {
        tx.with_group(alix_group.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let message = if far_forward {
                let mut last = None;
                for _ in 0..1_002 {
                    let (message, _) = mls
                        .propose_self_update(&provider, &signer, LeafNodeParameters::default())
                        .unwrap();
                    last = Some(message);
                }
                last.unwrap()
            } else {
                let extensions = mls.extensions().clone();
                mls.propose_group_context_extensions(&provider, extensions, &signer)
                    .unwrap()
                    .0
            };
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let topic = StreamTopic::group(bo_group.group_id);
    let db = bo.context.db();
    let before = db.topic_progress(&topic)?;
    let rejection = db.read_last_rejection(&topic)?;
    alix.context
        .api()
        .send_group_messages(alix_group.prepare_group_messages(vec![(&message, false)])?)
        .await?;
    let wire_topic = Topic::new_group_message(bo_group.group_id);
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(wire_topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert_eq!(envelopes.len(), 1);
    bo.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire_topic,
            after: before.received,
            envelopes: envelopes.into_iter().map(Into::into).collect(),
        },
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;

    let outcome = match bo_group.process_pending_group_head(None)? {
        GroupHeadOutcome::Waiting { code, .. } => code,
        GroupHeadOutcome::Progress { result, .. } => panic!("not held: {result:?}"),
        _ => panic!("the message must be held"),
    };
    assert_eq!(outcome, held_code);
    assert_eq!(db.read_last_rejection(&topic)?, rejection);
    assert_eq!(db.topic_progress(&topic)?.processed, before.processed);
    assert!(db.first_pending_envelope(&topic)?.is_some());
    Ok(())
}
