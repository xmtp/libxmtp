//! Archive tests: dm tests.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_archive() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    let alix_bo_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let archived_message_id = alix_bo_dm
        .send_message(b"old group", Default::default())
        .await?;

    let timestamp = alix
        .db()
        .find_group(&alix_bo_dm.group_id)??
        .last_message_ns?;

    let key = vec![7; 32];
    let opts = ArchiveOptions {
        start_ns: None,
        end_ns: None,
        elements: vec![
            BackupElementSelection::Messages,
            BackupElementSelection::Consent,
        ],
        exclude_disappearing_messages: false,
    };
    let export = {
        let mut file = vec![];

        exporter::export(opts, alix.db(), &key, &mut file)?;
        file
    };

    tester!(alix2, from: alix);
    let reader = Box::pin(BufReader::new(Cursor::new(export)));
    let mut importer = ArchiveImporter::load(reader, &key).await?;
    insert_importer(&mut importer, &alix2.context).await?;

    // Imported history has no joined MLS state for this installation.
    // Receipt may retain the old prefix, but processing must wait for a Welcome.
    let restored = alix2.group(&alix_bo_dm.group_id)?;
    let restored_topic = xmtp_db::incoming_envelope::StreamTopic::group(alix_bo_dm.group_id);
    let before_join = alix2.db().topic_progress(&restored_topic)?;
    xmtp_mls_core::mls_store::MlsStore::new(alix2.context.clone())
        .receive_topics_once(
            &[xmtp_proto::types::Topic::new_group_message(
                alix_bo_dm.group_id,
            )],
            alix2
                .context
                .incoming_runtime()
                .policy()
                .incoming_limits(xmtp_db::incoming_envelope::NetworkEntityKind::Group),
        )
        .await?;
    let received_before_join = alix2.db().topic_progress(&restored_topic)?;
    assert!(received_before_join.received > before_join.processed);
    assert!(matches!(
        restored.process_pending_group_head(None)?,
        xmtp_mls_engine::groups::test_support::GroupHeadOutcome::Inactive
    ));
    assert_eq!(
        alix2.db().topic_progress(&restored_topic)?.processed,
        before_join.processed
    );
    assert!(
        !alix2
            .db()
            .pending_states_through(&restored_topic, received_before_join.received)?
            .is_empty()
    );

    let alix2_bo_dm = alix2.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_ne!(alix_bo_dm.group_id, alix2_bo_dm.group_id);
    let mut msgs = alix2_bo_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(msgs.len(), 2);
    assert!(
        msgs.iter()
            .any(|m| m.decrypted_message_bytes == b"old group")
    );

    // assert_eq!(alix2_bo_dm.test_last_message_bytes().await??, b"old group");

    let timestamp2 = alix2
        .db()
        .find_group(&alix_bo_dm.group_id)??
        .last_message_ns?;
    assert_eq!(timestamp, timestamp2);

    let live_message_id = alix2_bo_dm
        .send_message(b"hi bo", Default::default())
        .await?;

    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_alix2_dm = bo.group(&alix2_bo_dm.group_id)?;
    assert_eq!(bo_alix2_dm.test_last_message_bytes().await??, b"hi bo");

    // Ordinary sync must add the new installation to the original DM too.
    alix2.sync_all_welcomes_and_groups(None).await?;
    let rejoined_original = alix2.group(&alix_bo_dm.group_id)?;
    assert!(rejoined_original.is_active()?);
    let stitched = alix2_bo_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(stitched.len(), 4);
    let application_ids: Vec<_> = stitched
        .iter()
        .filter(|message| message.kind == xmtp_db::group_message::GroupMessageKind::Application)
        .map(|message| message.id.clone())
        .collect();
    assert_eq!(application_ids, vec![archived_message_id, live_message_id]);
    assert_eq!(alix2.find_groups(Default::default())?.len(), 1);
    let bo_original = bo.group(&alix_bo_dm.group_id)?;
    rejoined_original.test_can_talk_with(&bo_original).await?;
    bo_original.test_can_talk_with(&rejoined_original).await?;
}

// verifies: ARCH-015, ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn foreign_archive_preserves_dm_pair_and_history() {
    use xmtp_db::{
        consent_record::{ConsentState, ConsentType},
        group::{DmIdExt, QueryGroup},
        readd_status::ReaddStatus,
    };
    use xmtp_mls_engine::groups::GroupError;
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(charlie, disable_workers);
    tester!(dana, disable_workers);
    let source = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let alix_id = source
        .send_message(b"from alix", Default::default())
        .await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&source.group_id)?;
    let bo_id = bo_group
        .send_message(b"from bo", Default::default())
        .await?;
    source.sync().await?;
    let archived_dm_id = alix
        .db()
        .find_group(&source.group_id)?
        .expect("source DM")
        .dm_id
        .expect("DM pair");
    assert!(!archived_dm_id.contains(charlie.inbox_id()));

    let key = vec![0x38; 32];
    let opts = ArchiveOptions {
        start_ns: None,
        end_ns: None,
        elements: vec![BackupElementSelection::Messages],
        exclude_disappearing_messages: false,
    };
    let mut export = vec![];
    exporter::export(opts.clone(), alix.db(), &key, &mut export)?;
    let reader = Box::pin(BufReader::new(Cursor::new(export)));
    let mut importer = ArchiveImporter::load(reader, &key).await?;
    insert_importer(&mut importer, &charlie.context).await?;

    let stored = charlie
        .db()
        .find_group(&source.group_id)?
        .expect("Restored DM");
    assert_eq!(stored.membership_state, GroupMembershipState::Restored);
    assert_eq!(stored.dm_id.as_deref(), Some(archived_dm_id.as_str()));
    assert_eq!(archived_dm_id.other_inbox_id(charlie.inbox_id()), None);
    let restored = charlie.group(&source.group_id)?;
    assert_eq!(
        restored
            .metadata()
            .await?
            .dm_members
            .expect("stub pair")
            .to_string(),
        archived_dm_id
    );
    assert!(!restored.is_active()?);
    let stats = charlie.context.api().api_client.mls_stats();
    stats.clear();
    assert!(matches!(
        restored.send_message(b"inactive", Default::default()).await,
        Err(GroupError::GroupInactive)
    ));
    assert!(matches!(
        restored.sync().await,
        Err(GroupError::GroupInactive)
    ));
    let Err(sync_summary) = restored.sync_with_conn().await else {
        panic!("Restored sync_with_conn must reject the request");
    };
    assert!(matches!(
        sync_summary.other_error(),
        Some(GroupError::GroupInactive)
    ));
    assert!(matches!(
        restored.receive().await,
        Err(GroupError::GroupInactive)
    ));
    assert!(matches!(
        charlie.client.sync_all_groups(vec![restored.clone()]).await,
        Err(GroupError::GroupInactive)
    ));
    assert_eq!(stats.publish.get_count(), 0);
    let joined = charlie.create_group(None, None)?;
    joined.add_members(&[dana.inbox_id()]).await?;
    dana.sync_welcomes().await?;
    let dana_group = dana.group(&joined.group_id)?;
    let incoming_id = dana_group
        .send_message(b"joined group", Default::default())
        .await?;
    let Err(GroupError::Sync(summary)) = charlie
        .client
        .sync_all_groups(vec![restored.clone(), joined])
        .await
    else {
        panic!("mixed explicit sync must report the inactive group");
    };
    assert!(matches!(
        summary.other_error(),
        Some(GroupError::GroupInactive)
    ));
    assert!(charlie.db().get_group_message(&incoming_id)?.is_some());
    let sweep = charlie.client.sync_all_welcomes_and_groups(None).await?;
    assert_eq!(sweep.num_eligible, 1);
    assert_eq!(sweep.num_synced, 1);
    assert!(!restored.is_active()?);
    let db = charlie.db();
    // Even an allowed archived DM has no authority for remote recovery.
    StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(source.group_id),
    )
    .merge_into(&db)?;
    let excludes_publish = db
        .get_conversation_ids_for_remote_log_publish()?
        .iter()
        .all(|group| group.id != source.group_id);
    let excludes_download = db
        .get_conversation_ids_for_remote_log_download()?
        .iter()
        .all(|group| group.id != source.group_id);
    let excludes_fork_check = db
        .get_conversation_ids_for_fork_check()?
        .iter()
        .all(|id| id.as_slice() != source.group_id.as_ref());
    db.set_group_commit_log_forked_status(&source.group_id, Some(true))?;
    let excludes_requesting_readds = db
        .get_conversation_ids_for_requesting_readds()?
        .iter()
        .all(|group| group.group_id != source.group_id);
    ReaddStatus {
        group_id: source.group_id,
        installation_id: vec![0x42; 32],
        requested_at_sequence_id: Some(1),
        responded_at_sequence_id: None,
    }
    .store(&db)?;
    let excludes_responding_readds = db
        .get_conversation_ids_for_responding_readds()?
        .iter()
        .all(|group| group.group_id != source.group_id);
    assert_eq!(
        [
            excludes_publish,
            excludes_download,
            excludes_fork_check,
            excludes_requesting_readds,
            excludes_responding_readds,
        ],
        [true; 5],
        "Restored DM must be absent from every recovery selector"
    );
    for (id, sender, bytes) in [
        (&alix_id, alix.inbox_id(), b"from alix".as_slice()),
        (&bo_id, bo.inbox_id(), b"from bo".as_slice()),
    ] {
        let message = charlie.db().get_group_message(id)?.expect("pair history");
        assert_eq!(message.sender_inbox_id, sender);
        assert_eq!(message.decrypted_message_bytes, bytes);
    }

    let mut reexport = vec![];
    exporter::export(opts, charlie.db(), &key, &mut reexport)?;
    let reader = Box::pin(BufReader::new(Cursor::new(reexport)));
    let mut exported = ArchiveImporter::load(reader, &key).await?;
    while let Some(element) = exported.next().await {
        match element?.element {
            Some(Element::Group(group)) => assert_ne!(group.id, source.group_id.as_ref()),
            Some(Element::GroupMessage(message)) => {
                assert_ne!(message.group_id, source.group_id.as_ref());
            }
            _ => {}
        }
    }

    let reopened = xmtp_mls::builder::ClientBuilder::from_client(charlie.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let reopened_group = reopened.group(&source.group_id)?;
    assert_eq!(
        reopened_group.dm_id.as_deref(),
        Some(archived_dm_id.as_str())
    );
    assert!(!reopened_group.is_active()?);
    assert!(reopened.db().get_group_message(&alix_id)?.is_some());
    assert!(reopened.db().get_group_message(&bo_id)?.is_some());
}

// verifies: DMS-001, ARCH-027
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_archived_dm_id_fails_before_group_insert() {
    tester!(alix, disable_workers);
    let group_id = vec![0x64; 16];
    let save = xmtp_proto::xmtp::device_sync::group_backup::GroupSave {
        id: group_id.clone(),
        conversation_type: 2,
        dm_id: Some("dm:malformed".into()),
        ..Default::default()
    };
    let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::Group(save)),
    })]);
    assert!(
        insert_elements(&mut elements, &alix.context, Default::default())
            .await
            .is_err()
    );
    assert!(
        alix.db()
            .find_group(&GroupId::try_from(group_id.as_slice())?)?
            .is_none()
    );
}
