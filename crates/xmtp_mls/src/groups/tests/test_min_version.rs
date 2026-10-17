//! Minimum protocol version gating and pause behaviour.

use super::*;

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

    tester!(alix);

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

    tester!(alix);

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

    tester!(alix);

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

#[xmtp_common::test(unwrap_try = true)]
async fn test_can_set_min_supported_protocol_version_for_commit() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    tester!(amal, version: amal_version.clone());
    tester!(bo);

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
    let bo = ClientBuilder::from_client(bo.client)
        .version(bo_version.clone())
        .build()
        .await
        .unwrap();

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
async fn test_client_on_old_version_blocks_welcome_until_upgrade() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    // Step 1: Create three clients, amal and bo are one version ahead of caro
    tester!(amal, version: amal_version);

    let mut bo_version = VersionInfo::default();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    tester!(bo, version: bo_version);
    tester!(caro, disable_workers);

    assert!(caro.version_info().pkg_version() != amal.version_info().pkg_version());
    assert!(bo.version_info().pkg_version() == amal.version_info().pkg_version());

    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal sends a message to the group
    amal_group
        .send_message("Hello, world!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);

    let message_text = String::from_utf8_lossy(&messages[1].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group to have a min version of current version + 1
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 6: Bo should still be able to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 4);

    // Step 7: Amal adds caro as a member
    amal_group
        .add_members(&[caro.context.identity.inbox_id()])
        .await
        .unwrap();

    // The unsupported Welcome must keep its keys and leave the group uninstalled.
    let welcome = caro
        .context
        .api()
        .query_welcome_messages(caro.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    let pending = crate::groups::welcome_sync::pending_welcome_for_test(&caro.context, &welcome)
        .await
        .unwrap();
    let result = crate::groups::XmtpWelcome::builder()
        .context(caro.context.clone())
        .welcome(&welcome)
        .pending(pending.clone())
        .validator(crate::groups::InitialMembershipValidator::new(
            caro.context.clone(),
        ))
        .process()
        .await;
    assert!(matches!(
        result,
        Err(GroupError::UnsupportedWelcomeVersion(version))
            if version == amal.version_info().pkg_version()
    ));
    assert!(
        caro.find_groups(GroupQueryArgs::default())
            .unwrap()
            .is_empty()
    );
    let db_topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: caro.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let Err(GroupError::StreamBarrier(error)) = caro.sync_welcomes().await else {
        panic!("the unsupported Welcome must remain blocked");
    };
    let status = assert_blocked_obligation(
        &error,
        &Topic::new_welcome_message(caro.context.installation_id()),
        Cursor(0),
        "welcome_blocked",
    );
    assert_eq!(status.unresolved_welcomes, vec![welcome.cursor]);
    let retained = caro
        .context
        .db()
        .pending_envelope(&db_topic, welcome.cursor)
        .unwrap()
        .unwrap();
    assert!(retained.blocked);
    assert_eq!(retained.envelope, pending.envelope);
    assert!(
        caro.context
            .db()
            .find_group(&amal_group.group_id)
            .unwrap()
            .is_none()
    );

    // Caro updates to the supported version and retries the same Welcome.
    let mut caro_version = caro.version_info().clone();
    caro_version.test_update_version(
        increment_patch_version(caro_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    let installation_id = caro.context.installation_id();
    let inbox_id = caro.inbox_id().to_string();
    let caro = ClientBuilder::from_client(caro.client)
        .version(caro_version)
        .with_disable_workers(true)
        .build()
        .await
        .unwrap();
    assert_eq!(caro.context.installation_id(), installation_id);
    assert_eq!(caro.inbox_id(), inbox_id);
    assert_eq!(
        caro.context
            .db()
            .pending_envelope(&db_topic, welcome.cursor)
            .unwrap()
            .unwrap()
            .envelope,
        pending.envelope,
    );
    caro.sync_welcomes().await.unwrap();
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);
    caro_group.sync().await.unwrap();
    assert!(
        caro.context
            .db()
            .pending_envelope(&db_topic, welcome.cursor)
            .unwrap()
            .is_none()
    );
    let fresh_id = amal_group
        .send_message(b"Hello after Caro upgrade", SendMessageOpts::default())
        .await
        .unwrap();
    caro_group.sync().await.unwrap();
    let fresh_messages = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let fresh = fresh_messages.last().unwrap();
    assert_eq!(fresh.id, fresh_id);
    assert_eq!(fresh.decrypted_message_bytes, b"Hello after Caro upgrade");

    // Caro should now be able to send a message
    caro_group
        .send_message("Hello from Caro".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages[messages.len() - 1].decrypted_message_bytes,
        "Hello from Caro".as_bytes()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_only_super_admins_can_set_min_supported_protocol_version() {
    tester!(amal);
    tester!(bo);

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
        .attributes
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
        .attributes
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
        .attributes
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
    let amal =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), amal_version).await;

    tester!(bo);

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
    let amal =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), amal_version.clone())
            .await;
    assert!(amal.context.version_info() != &VersionInfo::default());
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

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
    let bo = ClientBuilder::from_client(bo)
        .version(bo_version)
        .build()
        .await
        .unwrap();

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
