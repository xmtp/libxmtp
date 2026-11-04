//! Archive tests: import tests.

use super::*;

// verifies: EVENT-001, EVENT-017
#[xmtp_common::test(unwrap_try = true)]
async fn partial_import_reports_incomplete_after_a_stored_change() {
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
        Some(4),
    );
    let record = StoredConsentRecord::new(
        xmtp_db::consent_record::ConsentType::InboxId,
        xmtp_db::consent_record::ConsentState::Allowed,
        "failed-import".into(),
    );
    let element = BackupElement {
        element: Some(Element::Consent(record.into())),
    };
    let mut elements =
        futures::stream::iter([Ok(element), Err(std::io::Error::other("archive cut"))]);
    assert!(
        insert_elements(&mut elements, &alix.context, Default::default())
            .await
            .is_err()
    );
    assert!(
        alix.db()
            .consent_row(&xmtp_db::consent_record::ConsentKey::inbox("failed-import"))?
            .is_some()
    );
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ArchiveRestored(restored)), ..
        }] if !restored.complete
    ));
}

// verifies: EVENT-004
#[xmtp_common::test(unwrap_try = true)]
async fn failed_import_suppresses_events_for_all_stored_elements() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let message_id = group.send_message_optimistic(b"archive message", Default::default())?;
    StoredConsentRecord::new(
        xmtp_db::consent_record::ConsentType::InboxId,
        xmtp_db::consent_record::ConsentState::Denied,
        bo.inbox_id().into(),
    )
    .merge_into(&alix.db())?;
    let key = vec![9; 32];
    let mut archive = vec![];
    exporter::export(
        ArchiveOptions {
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            start_ns: None,
            end_ns: None,
            exclude_disappearing_messages: false,
        },
        alix.db(),
        &key,
        &mut archive,
    )?;
    let importer =
        ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(archive))), &key).await?;
    let mut elements = importer
        .map(|item| item.map_err(DeviceSyncError::from))
        .chain(futures::stream::iter([Err(DeviceSyncError::from(
            std::io::Error::other("archive cut"),
        ))]));
    tester!(restored, from: alix, disable_workers);
    let events = restored.context.events().subscribe(
        xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ArchiveRestored,
            xmtp_events::EventKind::ConversationJoined,
            xmtp_events::EventKind::MessageReceived,
            xmtp_events::EventKind::ConsentChanged,
        ]),
        Some(64),
    );
    assert!(
        insert_elements(&mut elements, &restored.context, Default::default())
            .await
            .is_err()
    );
    assert!(restored.db().find_group(&group.group_id)?.is_some());
    assert!(restored.db().get_group_message(&message_id)?.is_some());
    assert!(
        restored
            .db()
            .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(bo.inbox_id()))?
            .is_some()
    );
    assert!(
        matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::ArchiveRestored(restored)), .. }] if !restored.complete)
    );
}
/// The greater record wins, and a time more than a day past the import
/// is stored at that limit.
// verifies: CONS-053, CONS-054
#[xmtp_common::test(unwrap_try = true)]
async fn archived_consent_keeps_the_latest_record() {
    tester!(alix, disable_workers);
    let soon = xmtp_common::time::now_ns() + xmtp_common::NS_IN_HOUR;
    let records = [
        (xmtp_db::consent_record::ConsentState::Denied, i64::MAX),
        (xmtp_db::consent_record::ConsentState::Allowed, 10),
        (xmtp_db::consent_record::ConsentState::Allowed, soon),
        (xmtp_db::consent_record::ConsentState::Denied, 20),
    ];
    let mut elements = futures::stream::iter(records.map(|(state, consented_at_ns)| {
        let record = StoredConsentRecord {
            entity_type: xmtp_db::consent_record::ConsentType::InboxId,
            state,
            entity: "archived-inbox".into(),
            consented_at_ns,
            owed_seq: 0,
        };
        Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::Consent(record.into())),
        })
    }));
    insert_elements(&mut elements, &alix.context, Default::default()).await?;
    let limit = xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY;
    let stored = alix
        .db()
        .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(
            "archived-inbox",
        ))?
        .unwrap();
    assert_eq!(stored.state, xmtp_db::consent_record::ConsentState::Denied);
    assert!(stored.consented_at_ns > soon && stored.consented_at_ns <= limit);
}

/// An import that only moves a stored record's time still stored an
/// element, so it reports a restore.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn archived_consent_time_move_reports_a_restore() {
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
        Some(4),
    );
    let record = |consented_at_ns| StoredConsentRecord {
        entity_type: xmtp_db::consent_record::ConsentType::InboxId,
        state: xmtp_db::consent_record::ConsentState::Allowed,
        entity: "time-move".into(),
        consented_at_ns,
        owed_seq: 0,
    };
    assert!(record(10).merge_into(&alix.db())?.is_some());
    let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::Consent(record(30).into())),
    })]);
    insert_elements(&mut elements, &alix.context, Default::default()).await?;
    let stored = alix
        .db()
        .consent_row(&xmtp_db::consent_record::ConsentKey::inbox("time-move"))?
        .unwrap();
    assert_eq!(stored.consented_at_ns, 30);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ArchiveRestored(restored)), ..
        }] if restored.complete
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn archive_timestamp_keeps_a_message_received_during_import() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let save = xmtp_proto::xmtp::device_sync::group_backup::GroupSave {
        id: group.group_id.to_vec(),
        last_message_ns: Some(0),
        ..Default::default()
    };

    group.send_message_optimistic(b"message during import", Default::default())?;
    let current = alix.db().find_group(&group.group_id)??.last_message_ns;
    assert!(current > Some(0));
    MlsGroup::restore_from_archive(&alix.context, &save)?;
    assert_eq!(
        alix.db().find_group(&group.group_id)??.last_message_ns,
        current
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_archive_timestamps() {
    tester!(alix, disable_workers);
    tester!(alix2, from: alix);
    tester!(bo, disable_workers);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    alix_group.send_message(b"hi", Default::default()).await?;

    alix2.sync_welcomes().await?;
    bo.sync_welcomes().await?;

    let alix2_group = alix2.group(&alix_group.group_id)?;
    let bo_group = bo.group(&alix_group.group_id)?;

    alix2_group.sync().await?;
    bo_group.sync().await?;

    // We want to send this message so that alix's group timestamp gets ahead of alix2.
    alix_group
        .send_message(b"Hello again", Default::default())
        .await?;

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

    tester!(alix3, from: alix);

    // Now we will have alix2 and alix3 import the archives.
    // One installation has the group already, one does not.
    let reader = Box::pin(BufReader::new(Cursor::new(export.clone())));
    let mut importer = ArchiveImporter::load(reader, &key).await?;
    insert_importer(&mut importer, &alix2.context).await?;

    let reader = Box::pin(BufReader::new(Cursor::new(export)));
    let mut importer = ArchiveImporter::load(reader, &key).await?;
    insert_importer(&mut importer, &alix3.context).await?;

    let alix_timestamp = alix
        .db()
        .find_group(&alix_group.group_id)??
        .last_message_ns?;
    let alix2_timestamp = alix2
        .db()
        .find_group(&alix_group.group_id)??
        .last_message_ns?;
    let alix3_timestamp = alix3
        .db()
        .find_group(&alix_group.group_id)??
        .last_message_ns?;

    // Alix2's older timestamp on the existing group should be updated.
    assert_eq!(alix2_timestamp, alix_timestamp);
    // Alix3's timestamp should equal alix's timestamp.
    assert_eq!(alix3_timestamp, alix_timestamp);
}

// verifies: ARCH-013
#[xmtp_common::test(unwrap_try = true)]
async fn archive_import_leaves_a_known_message_unchanged() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let known_id = group.send_message(b"known", Default::default()).await?;
    let known = alix
        .db()
        .get_group_message(&known_id)?
        .expect("known message");
    let mut save: xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave =
        known.clone().into();
    save.sender_inbox_id = hex::encode([0x43; 32]);
    save.decrypted_message_bytes = b"ignored replacement".to_vec();
    let mut duplicate = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::GroupMessage(save)),
    })]);
    insert_elements(&mut duplicate, &alix.context, Default::default()).await?;
    assert_eq!(alix.db().get_group_message(&known_id)?, Some(known));
}

// verifies: ARCH-019, ARCH-009
#[xmtp_common::test(unwrap_try = true)]
async fn imported_message_keeps_expiry_and_exclusion() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let source = alix.create_group(
        None,
        Some(GroupMetadataOptions {
            message_disappearing_settings: Some(MessageDisappearingSettings::new(
                1,
                xmtp_common::NS_IN_HOUR,
            )),
            ..Default::default()
        }),
    )?;
    let message_id = source
        .send_message(b"disappearing archive content", Default::default())
        .await?;
    let source_message = alix
        .db()
        .get_group_message(&message_id)?
        .expect("sent message");
    assert!(
        source_message
            .expire_at_ns
            .is_some_and(|deadline| deadline > xmtp_common::time::now_ns()),
        "the source message must still be eligible for export"
    );

    source
        .remove_conversation_message_disappearing_settings()
        .await?;
    let permanent_id = source
        .send_message(b"permanent archive content", Default::default())
        .await?;
    let permanent = alix
        .db()
        .get_group_message(&permanent_id)?
        .expect("permanent source");
    assert_eq!(
        (permanent.expire_at_ns, permanent.expiry_unknown),
        (None, false)
    );

    let key = [0x4a; 32];
    let options = ArchiveOptions {
        elements: vec![BackupElementSelection::Messages],
        start_ns: None,
        end_ns: None,
        exclude_disappearing_messages: false,
    };
    let mut first_archive = Vec::new();
    exporter::export(options.clone(), alix.db(), &key, &mut first_archive)?;
    let reader = Box::pin(BufReader::new(Cursor::new(first_archive)));
    let mut importer = ArchiveImporter::load(reader, &key).await?;
    insert_importer(&mut importer, &bo.context).await?;

    let imported = bo
        .db()
        .get_group_message(&message_id)?
        .expect("imported message");
    assert_eq!(
        imported.decrypted_message_bytes,
        source_message.decrypted_message_bytes
    );
    assert_eq!(imported.expire_at_ns, source_message.expire_at_ns);
    assert!(!imported.expiry_unknown);
    let permanent_copy = bo
        .db()
        .get_group_message(&permanent_id)?
        .expect("permanent copy");
    assert_eq!(
        (permanent_copy.expire_at_ns, permanent_copy.expiry_unknown),
        (None, false)
    );
    assert_eq!(
        bo.db()
            .find_group(&source.group_id)?
            .expect("imported group")
            .membership_state,
        GroupMembershipState::Restored
    );

    source.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    assert!(bo.group(&source.group_id)?.is_active()?);

    // An older archive lacks expiry_known, even if it carries a stray deadline.
    let unknown_id = vec![0x5b; 32];
    let mut old = xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave::from(permanent);
    old.id = unknown_id.clone();
    old.expiry_known = None;
    old.expires_at_ns = source_message.expire_at_ns;
    let mut old_elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::GroupMessage(old)),
    })]);
    insert_elements(&mut old_elements, &bo.context, Default::default()).await?;
    let unknown = bo
        .db()
        .get_group_message(&unknown_id)?
        .expect("old archive row");
    assert_eq!((unknown.expire_at_ns, unknown.expiry_unknown), (None, true));

    let mut excluded = options.clone();
    excluded.exclude_disappearing_messages = true;
    let mut second_archive = Vec::new();
    exporter::export(excluded, bo.db(), &key, &mut second_archive)?;
    let reader = Box::pin(BufReader::new(Cursor::new(second_archive)));
    let messages: Vec<_> = ArchiveImporter::load(reader, &key)
        .await?
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()?;
    let saved: Vec<_> = messages
        .into_iter()
        .filter_map(|element| match element.element {
            Some(Element::GroupMessage(saved)) => Some(saved),
            _ => None,
        })
        .collect();
    assert_eq!(
        saved.len(),
        1,
        "only known non-disappearing history is exported"
    );
    assert_eq!(saved[0].id, permanent_id);
    assert_eq!(
        (saved[0].expiry_known, saved[0].expires_at_ns),
        (Some(true), None)
    );

    let mut default_export = Vec::new();
    exporter::export(options, bo.db(), &key, &mut default_export)?;
    let reader = Box::pin(BufReader::new(Cursor::new(default_export)));
    let exported = ArchiveImporter::load(reader, &key)
        .await?
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let saved = exported
        .into_iter()
        .find_map(|element| match element.element {
            Some(Element::GroupMessage(saved)) if saved.id == unknown_id => Some(saved),
            _ => None,
        })
        .expect("default export retains unknown history");
    assert_eq!(
        (saved.expiry_known, saved.expires_at_ns),
        (Some(false), None)
    );
}

/// A DM restored before a failing element is a completed element. Its
/// archived activity must survive the failed import, even when no
/// retained message carries that timestamp.
// verifies: ARCH-027
#[xmtp_common::test(unwrap_try = true)]
async fn failed_import_keeps_archived_activity_of_restored_prefix() {
    use xmtp_mls_group_state::group_metadata::DmMembers;
    use xmtp_proto::xmtp::device_sync::{
        group_backup::GroupSave,
        message_backup::{GroupMessageKindSave, GroupMessageSave},
    };

    tester!(caro, disable_workers);
    let pair = DmMembers {
        member_one_inbox_id: hex::encode([0x41; 32]),
        member_two_inbox_id: hex::encode([0x42; 32]),
    };
    let group_id = vec![0x65; 16];
    let archived_activity = 1_234_567;
    let group = GroupSave {
        id: group_id.clone(),
        conversation_type: 2,
        dm_id: Some(pair.to_string()),
        last_message_ns: Some(archived_activity),
        ..Default::default()
    };
    // A recognized message element with an unspecified kind is malformed.
    let malformed = GroupMessageSave {
        id: vec![0x66; 32],
        group_id: group_id.clone(),
        kind: GroupMessageKindSave::Unspecified as i32,
        sender_inbox_id: pair.member_one_inbox_id.clone(),
        ..Default::default()
    };

    let events = caro.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
        Some(4),
    );
    let mut elements = futures::stream::iter([
        Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::Group(group)),
        }),
        Ok(BackupElement {
            element: Some(Element::GroupMessage(malformed.clone())),
        }),
    ]);
    let error = insert_elements(&mut elements, &caro.context, Default::default())
        .await
        .expect_err("a malformed element fails the import");
    assert!(
        matches!(
            error,
            DeviceSyncError::ProtoConversion(xmtp_proto::ConversionError::Unspecified(
                "message_kind"
            ))
        ),
        "unexpected import error: {error:?}"
    );

    let restored = caro
        .db()
        .find_group(&GroupId::try_from(group_id.as_slice())?)??;
    assert_eq!(restored.membership_state, GroupMembershipState::Restored);
    assert_eq!(restored.last_message_ns, Some(archived_activity));
    assert!(caro.db().get_group_message(&malformed.id)?.is_none());
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ArchiveRestored(restored)), ..
        }] if !restored.complete
    ));
}
