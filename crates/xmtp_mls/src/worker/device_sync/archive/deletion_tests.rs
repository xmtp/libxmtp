//! Deletions survive archives, re-imports and local removal.
use super::*;
use crate::groups::GroupMessageProcessingError;
use crate::tester;
use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection};
use futures::io::{BufReader, Cursor};
use xmtp_content_types::{ContentCodec, delete_message::DeleteMessageCodec, text::TextCodec};
use xmtp_db::Store;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use xmtp_db::message_deletion::{QueryMessageDeletion, StoredMessageDeletion};
use xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave;
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

const SENDER: &str = "sender-inbox";
const ADMIN: &str = "admin-inbox";
const MEMBER: &str = "member-inbox";
const TARGET: [u8; 32] = [0x01; 32];
/// The first deletion: later in time, larger id.
const D1: [u8; 32] = [0x0f; 32];
/// The second deletion: earlier id.
const D2: [u8; 32] = [0x0a; 32];

fn row(
    group_id: GroupId,
    id: &[u8],
    sender: &str,
    bytes: Vec<u8>,
    status: DeliveryStatus,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: id.to_vec(),
        group_id,
        decrypted_message_bytes: bytes,
        sent_at_ns: 100,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![0x77; 32],
        sender_inbox_id: sender.into(),
        delivery_status: status,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        expire_at_ns: None,
        expiry_unknown: false,
        should_push: false,
        idempotency_key: hex::encode(id),
        deleted_by_inbox_id: None,
    }
}

fn text_with_authority(authority: &str, major: u32) -> Vec<u8> {
    let mut content = TextCodec::encode("archived target".into()).unwrap();
    let kind = content.r#type.as_mut().unwrap();
    kind.authority_id = authority.into();
    kind.version_major = major;
    xmtp_content_types::encoded_content_to_bytes(content)
}

fn target(group_id: GroupId, status: DeliveryStatus) -> StoredGroupMessage {
    row(
        group_id,
        &TARGET,
        SENDER,
        text_with_authority("xmtp.org", 1),
        status,
    )
}

fn deletion(
    group_id: GroupId,
    id: [u8; 32],
    sender: &str,
    status: DeliveryStatus,
) -> StoredGroupMessage {
    let bytes = xmtp_content_types::encoded_content_to_bytes(
        DeleteMessageCodec::encode(DeleteMessage {
            message_id: hex::encode(TARGET),
        })
        .unwrap(),
    );
    StoredGroupMessage {
        content_type: ContentType::DeleteMessage,
        reference_id: Some(TARGET.to_vec()),
        sent_at_ns: 200,
        ..row(group_id, &id, sender, bytes, status)
    }
}

fn record(group_id: GroupId, id: [u8; 32], by: &str, admin: bool, at: i64) -> BackupElement {
    BackupElement {
        element: Some(Element::MessageDeletion(
            StoredMessageDeletion {
                id: id.to_vec(),
                group_id,
                deleted_message_id: TARGET.to_vec(),
                deleted_by_inbox_id: by.into(),
                is_super_admin_deletion: admin,
                deleted_at_ns: at,
                authenticated: false,
            }
            .into(),
        )),
    }
}

/// A current archive's copy of `message` marked deleted by `by`.
fn marked(message: &StoredGroupMessage, by: &str) -> BackupElement {
    let mut save: GroupMessageSave = message.clone().into();
    save.deleted_by_inbox_id = Some(by.into());
    BackupElement {
        element: Some(Element::GroupMessage(save)),
    }
}

/// A pre-8.0 archive's copy of `message`: no expiry fields, no field 19.
fn legacy(message: &StoredGroupMessage) -> BackupElement {
    let mut save: GroupMessageSave = message.clone().into();
    save.expires_at_ns = None;
    save.expiry_known = None;
    save.deleted_by_inbox_id = None;
    BackupElement {
        element: Some(Element::GroupMessage(save)),
    }
}

/// One import step's events: how many `archive.restored` and
/// `message.deleted` events it emitted.
async fn import(context: &impl XmtpSharedContext, elements: Vec<BackupElement>) -> (usize, usize) {
    let events = context.events().subscribe(
        xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ArchiveRestored,
            xmtp_events::EventKind::MessageDeleted,
        ]),
        Some(32),
    );
    // Two passes, as the SDK imports: deletion messages first.
    let self_deletions = collect_self_deletions(&mut futures::stream::iter(
        elements.iter().cloned().map(Ok::<_, std::io::Error>),
    ))
    .await
    .unwrap();
    insert_elements(
        &mut futures::stream::iter(elements.into_iter().map(Ok::<_, std::io::Error>)),
        context,
        self_deletions,
    )
    .await
    .unwrap();
    let drained = events.drain();
    let count = |restored: bool| {
        drained
            .iter()
            .filter(|event| match &event.client {
                Some(ClientEvent::ArchiveRestored(_)) => restored,
                Some(ClientEvent::MessageDeleted(_)) => !restored,
                _ => false,
            })
            .count()
    };
    (count(true), count(false))
}

/// Process `message` as the receive path processes a network copy.
fn receive<C: XmtpSharedContext>(group: &MlsGroup<C>, message: &StoredGroupMessage, admin: bool) {
    crate::state_tx::state_write_with_events(
        group.context.mls_storage(),
        group.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            group.store_external_application_message(&storage, message, admin, buffer)?;
            Ok::<_, GroupMessageProcessingError>(Continue(()))
        },
    )
    .unwrap();
}

fn mark_of(db: &impl DbQuery, id: &[u8]) -> Option<String> {
    db.get_group_message(id)
        .unwrap()
        .and_then(|message| message.deleted_by_inbox_id)
}

#[derive(Clone, Copy, Debug)]
enum Case {
    ImportedSenderDeletionOfStoredTarget,
    OldFormatTargetFirst,
    OldFormatDeletionFirst,
    OldFormatAdminDeletion,
    OldFormatSelfDeletionOfHeldTarget,
    OldFormatAdminDeletionFirst,
    UnpublishedThenPublishedDeletion,
    ConvertedMarkOverKnownTarget,
    ConvertedMarkFresh,
    DeletionOnlyImport,
    TopicRecordsEarliestWins,
    KnownTargetWithStoredRecord,
    KnownTargetWithTopicRecord,
    CraftedArchiveKeepsHeldMessage,
    OldThenFieldNineteen,
    MarkOverOtherInstallationsUnpublished,
    MarkOverOtherInstallationsFailed,
    AdminRecordOutlivesItsMessage,
    RemovedTargetReimported,
    ReplayIsForeign,
    EarlierTopicRecordThenPresentTarget,
    ReplayOverImportedRecord,
    TopicDeletionOverForgedRecord,
    ForgedRowDoesNotBlockTopic,
    ForgedUnpublishedRowDoesNotBlockTopic,
    UnpublishedImportThenNetworkCopy,
    FailedDeletionImported,
    StaleCachedTextTarget,
}

/// Each case ends with an import. `expected` is the mark on the target
/// afterwards; `restored` is how many `archive.restored` the last import
/// emits. No import emits `message.deleted`.
// verifies: PROC-037, ARCH-028, ARCH-008, ARCH-013, EVENT-004
#[rstest::rstest]
#[case::imported_sender_deletion_of_held_target(
    Case::ImportedSenderDeletionOfStoredTarget,
    None,
    1
)]
#[case::old_format_target_first(Case::OldFormatTargetFirst, Some(SENDER), 1)]
#[case::old_format_deletion_first(Case::OldFormatDeletionFirst, Some(SENDER), 1)]
#[case::old_format_self_deletion_of_held_target(Case::OldFormatSelfDeletionOfHeldTarget, None, 1)]
#[case::old_format_admin_deletion(Case::OldFormatAdminDeletion, None, 1)]
#[case::old_format_admin_deletion_first(Case::OldFormatAdminDeletionFirst, None, 1)]
#[case::unpublished_then_published_deletion(
    Case::UnpublishedThenPublishedDeletion,
    Some(SENDER),
    1
)]
#[case::converted_mark_over_known_target(Case::ConvertedMarkOverKnownTarget, None, 0)]
#[case::converted_mark_fresh(Case::ConvertedMarkFresh, Some(SENDER), 1)]
#[case::deletion_only_import_over_held_target(Case::DeletionOnlyImport, None, 1)]
#[case::topic_records_earliest_wins(Case::TopicRecordsEarliestWins, Some(SENDER), 1)]
#[case::known_target_with_imported_record(Case::KnownTargetWithStoredRecord, None, 0)]
#[case::known_target_with_topic_record(Case::KnownTargetWithTopicRecord, Some(SENDER), 1)]
#[case::old_then_field_nineteen(Case::OldThenFieldNineteen, None, 0)]
#[case::mark_over_other_installations_unpublished(
    Case::MarkOverOtherInstallationsUnpublished,
    None,
    0
)]
#[case::mark_over_other_installations_failed(Case::MarkOverOtherInstallationsFailed, None, 0)]
#[case::crafted_archive_keeps_held_message(Case::CraftedArchiveKeepsHeldMessage, None, 1)]
#[case::admin_record_outlives_its_message(Case::AdminRecordOutlivesItsMessage, Some(ADMIN), 1)]
#[case::removed_target_reimported(Case::RemovedTargetReimported, Some(SENDER), 1)]
#[case::replay_is_foreign(Case::ReplayIsForeign, Some(ADMIN), 1)]
#[case::earlier_topic_record_then_present_target(
    Case::EarlierTopicRecordThenPresentTarget,
    Some(ADMIN),
    0
)]
#[case::replay_over_imported_record(Case::ReplayOverImportedRecord, None, 1)]
#[case::topic_deletion_over_forged_record(Case::TopicDeletionOverForgedRecord, Some(SENDER), 1)]
#[case::forged_row_does_not_block_topic(Case::ForgedRowDoesNotBlockTopic, Some(MEMBER), 1)]
#[case::forged_unpublished_row_does_not_block_topic(
    Case::ForgedUnpublishedRowDoesNotBlockTopic,
    Some(MEMBER),
    1
)]
#[case::unpublished_import_then_network_copy(
    Case::UnpublishedImportThenNetworkCopy,
    Some(SENDER),
    0
)]
#[case::failed_deletion_imported(Case::FailedDeletionImported, None, 1)]
#[case::stale_cached_text_target(Case::StaleCachedTextTarget, None, 1)]
#[xmtp_common::test]
async fn deletion_survives_storage(
    #[case] case: Case,
    #[case] expected: Option<&str>,
    #[case] restored: usize,
) {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None).unwrap();
    let g = group.group_id;
    let db = alix.db();
    let published = DeliveryStatus::Published;
    let mut last = (0, 0);
    match case {
        Case::ImportedSenderDeletionOfStoredTarget => {
            target(g, published).store(&db).unwrap();
            last = import(
                &alix.context,
                vec![legacy(&deletion(g, D1, SENDER, published))],
            )
            .await;
        }
        Case::OldFormatTargetFirst => {
            last = import(
                &alix.context,
                vec![
                    legacy(&target(g, published)),
                    legacy(&deletion(g, D1, SENDER, published)),
                ],
            )
            .await;
        }
        Case::OldFormatDeletionFirst => {
            last = import(
                &alix.context,
                vec![
                    legacy(&deletion(g, D1, SENDER, published)),
                    legacy(&target(g, published)),
                ],
            )
            .await;
        }
        Case::OldFormatSelfDeletionOfHeldTarget => {
            // The same pre-8.0 archive over a target already held: the held
            // row is never marked by archive data.
            target(g, published).store(&db).unwrap();
            last = import(
                &alix.context,
                vec![
                    legacy(&target(g, published)),
                    legacy(&deletion(g, D1, SENDER, published)),
                ],
            )
            .await;
        }
        Case::OldFormatAdminDeletion | Case::OldFormatAdminDeletionFirst => {
            // An archive's deletion by anyone but the sender is never applied.
            let mut elements = vec![
                legacy(&target(g, published)),
                legacy(&deletion(g, D1, ADMIN, published)),
            ];
            if matches!(case, Case::OldFormatAdminDeletionFirst) {
                elements.reverse();
            }
            last = import(&alix.context, elements).await;
        }
        Case::UnpublishedThenPublishedDeletion => {
            // The Published copy restores the self-deletion of the target
            // this import creates, though the stored copy is Unpublished.
            last = import(
                &alix.context,
                vec![
                    legacy(&target(g, published)),
                    legacy(&deletion(g, D1, SENDER, DeliveryStatus::Unpublished)),
                    legacy(&deletion(g, D1, SENDER, published)),
                ],
            )
            .await;
            // The stored deletion row stays as it was.
            assert_eq!(
                db.get_group_message(D1).unwrap().unwrap().delivery_status,
                DeliveryStatus::Unpublished
            );
            assert!(!db.get_message_deletion(&D1).unwrap().unwrap().authenticated);
        }
        Case::ConvertedMarkOverKnownTarget | Case::ConvertedMarkFresh => {
            // The legacy classifier deleted a type the catalogue does not.
            let custom = row(
                g,
                &TARGET,
                SENDER,
                text_with_authority("custom.example", 99),
                published,
            );
            if matches!(case, Case::ConvertedMarkOverKnownTarget) {
                import(&alix.context, vec![legacy(&custom)]).await;
            }
            last = import(&alix.context, vec![marked(&custom, SENDER)]).await;
        }
        Case::DeletionOnlyImport => {
            target(g, published).store(&db).unwrap();
            last = import(&alix.context, vec![record(g, D1, SENDER, false, 10)]).await;
        }
        Case::TopicRecordsEarliestWins => {
            // Two deletions processed from the topic before the target was
            // stored: the earlier by envelope time names the deleter.
            let later = StoredGroupMessage {
                sent_at_ns: 300,
                ..deletion(g, D2, ADMIN, published)
            };
            receive(&group, &later, true);
            receive(&group, &deletion(g, D1, SENDER, published), false);
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
        }
        Case::KnownTargetWithStoredRecord => {
            import(&alix.context, vec![record(g, D1, SENDER, false, 10)]).await;
            // The target is stored again, unmarked.
            target(g, published).store(&db).unwrap();
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
        }
        Case::KnownTargetWithTopicRecord => {
            // A deletion processed from the topic before its target was
            // stored again; an import of the held target applies it.
            receive(&group, &deletion(g, D1, SENDER, published), false);
            target(g, published).store(&db).unwrap();
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
        }
        Case::CraftedArchiveKeepsHeldMessage => {
            // An archive marks a held message, forges a super-admin record
            // and carries a deletion message: none of it deletes the message.
            target(g, published).store(&db).unwrap();
            last = import(
                &alix.context,
                vec![
                    record(g, D2, ADMIN, true, 10),
                    marked(&target(g, published), ADMIN),
                    legacy(&deletion(g, D1, SENDER, published)),
                ],
            )
            .await;
        }
        Case::OldThenFieldNineteen => {
            import(&alix.context, vec![legacy(&target(g, published))]).await;
            last = import(&alix.context, vec![marked(&target(g, published), SENDER)]).await;
        }
        Case::MarkOverOtherInstallationsUnpublished | Case::MarkOverOtherInstallationsFailed => {
            let status = if matches!(case, Case::MarkOverOtherInstallationsFailed) {
                DeliveryStatus::Failed
            } else {
                DeliveryStatus::Unpublished
            };
            target(g, status).store(&db).unwrap();
            last = import(&alix.context, vec![marked(&target(g, status), SENDER)]).await;
            assert_eq!(
                db.get_group_message(TARGET)
                    .unwrap()
                    .unwrap()
                    .delivery_status,
                status
            );
        }
        Case::AdminRecordOutlivesItsMessage => {
            receive(&group, &deletion(g, D1, ADMIN, published), true);
            // The deletion message expires or is removed; its record stays.
            db.delete_message_by_id(D1).unwrap();
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
        }
        Case::RemovedTargetReimported => {
            target(g, published).store(&db).unwrap();
            receive(&group, &deletion(g, D1, SENDER, published), false);
            assert_eq!(mark_of(&db, &TARGET).as_deref(), Some(SENDER));
            db.delete_message_by_id(D1).unwrap();
            db.delete_message_by_id(TARGET).unwrap();
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
        }
        // An authenticated record is never overwritten by a replay.
        Case::ReplayIsForeign => {
            receive(&group, &deletion(g, D1, ADMIN, published), true);
            // Another member sends the same content under the same id.
            receive(&group, &deletion(g, D1, MEMBER, published), false);
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
            let kept = db.get_message_deletion(&D1).unwrap().unwrap();
            assert_eq!(kept.deleted_by_inbox_id, ADMIN);
            assert!(kept.is_super_admin_deletion && kept.authenticated);
        }
        Case::EarlierTopicRecordThenPresentTarget => {
            // A super admin's deletion is recorded from the topic while the
            // target is absent; the target is then stored, and the sender's
            // own later deletion arrives. The earlier authorized record names
            // the deleter, not the one being processed.
            let earlier = StoredGroupMessage {
                sent_at_ns: 100,
                ..deletion(g, D2, ADMIN, published)
            };
            receive(&group, &earlier, true);
            target(g, published).store(&db).unwrap();
            let later = StoredGroupMessage {
                sent_at_ns: 300,
                ..deletion(g, D1, SENDER, published)
            };
            receive(&group, &later, false);
        }
        Case::ReplayOverImportedRecord => {
            // An archive holds a record naming the admin; the topic's copy
            // comes from a member. The topic's record replaces the archived
            // one, and a member's deletion marks nothing.
            import(&alix.context, vec![record(g, D1, ADMIN, true, 10)]).await;
            receive(&group, &deletion(g, D1, MEMBER, published), false);
            last = import(&alix.context, vec![legacy(&target(g, published))]).await;
            let kept = db.get_message_deletion(&D1).unwrap().unwrap();
            assert_eq!(kept.deleted_by_inbox_id, MEMBER);
            assert!(!kept.is_super_admin_deletion && kept.authenticated);
        }
        Case::TopicDeletionOverForgedRecord => {
            // An archive forges a record of the deletion under another inbox;
            // the sender's real deletion from the topic still applies.
            target(g, published).store(&db).unwrap();
            last = import(&alix.context, vec![record(g, D1, MEMBER, false, 10)]).await;
            receive(&group, &deletion(g, D1, SENDER, published), false);
        }
        Case::ForgedRowDoesNotBlockTopic => {
            // An archive forges the deletion's row under another sender; the
            // real copy from a super admin still applies, and its record
            // replaces the archived one.
            target(g, published).store(&db).unwrap();
            last = import(
                &alix.context,
                vec![legacy(&deletion(g, D1, ADMIN, published))],
            )
            .await;
            receive(&group, &deletion(g, D1, MEMBER, published), true);
            let kept = db.get_message_deletion(&D1).unwrap().unwrap();
            assert_eq!(kept.deleted_by_inbox_id, MEMBER);
            assert!(kept.is_super_admin_deletion && kept.authenticated);
        }
        Case::ForgedUnpublishedRowDoesNotBlockTopic => {
            // The forged row writes no record; the topic's copy still applies.
            target(g, published).store(&db).unwrap();
            let forged = deletion(g, D1, ADMIN, DeliveryStatus::Unpublished);
            last = import(&alix.context, vec![legacy(&forged)]).await;
            receive(&group, &deletion(g, D1, MEMBER, published), true);
            assert!(db.get_message_deletion(&D1).unwrap().unwrap().authenticated);
        }
        Case::UnpublishedImportThenNetworkCopy => {
            target(g, published).store(&db).unwrap();
            import(
                &alix.context,
                vec![legacy(&deletion(
                    g,
                    D1,
                    SENDER,
                    DeliveryStatus::Unpublished,
                ))],
            )
            .await;
            assert_eq!(mark_of(&db, &TARGET), None);
            receive(&group, &deletion(g, D1, SENDER, published), false);
        }
        Case::FailedDeletionImported => {
            target(g, published).store(&db).unwrap();
            last = import(
                &alix.context,
                vec![legacy(&deletion(g, D1, SENDER, DeliveryStatus::Failed))],
            )
            .await;
            assert!(db.get_message_deletion(&D1).unwrap().is_none());
        }
        Case::StaleCachedTextTarget => {
            // The cached type says text; the wire type is not deletable.
            import(&alix.context, vec![record(g, D1, SENDER, false, 10)]).await;
            let stale = row(
                g,
                &TARGET,
                SENDER,
                text_with_authority("custom.example", 1),
                published,
            );
            last = import(&alix.context, vec![legacy(&stale)]).await;
        }
    }
    let stored = db.get_group_message(TARGET).unwrap().unwrap();
    assert_eq!(stored.deleted_by_inbox_id.as_deref(), expected, "{case:?}");
    assert_eq!(
        stored.decrypted_message_bytes.is_empty(),
        expected.is_some(),
        "{case:?}"
    );
    assert_eq!(last, (restored, 0), "{case:?} events");
}

/// Export writes a marked message with field 19 and no content, and every
/// record before the messages and outside the export window. Import of it
/// shows the placeholder and emits no `message.deleted`; a re-import changes
/// nothing; a record-only window marks a held, unmarked copy.
// verifies: ARCH-028, EVENT-004
#[xmtp_common::test(unwrap_try = true)]
async fn export_carries_marks_and_every_record() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let body = xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(
        "secret-target-body".into(),
    )?);
    let target_id = group.send_message(&body, Default::default()).await?;
    let key = vec![3; 32];
    let export = |start_ns: Option<i64>| {
        let mut archive = vec![];
        exporter::export(
            ArchiveOptions {
                elements: vec![BackupElementSelection::Messages],
                start_ns,
                end_ns: None,
                exclude_disappearing_messages: true,
            },
            alix.db(),
            &key,
            &mut archive,
        )
        .unwrap();
        archive
    };
    let elements = |archive: Vec<u8>| {
        let key = key.clone();
        async move {
            let importer =
                ArchiveImporter::load(Box::pin(BufReader::new(Cursor::new(archive))), &key)
                    .await
                    .unwrap();
            importer
                .map(|element| element.unwrap())
                .collect::<Vec<_>>()
                .await
        }
    };
    let before = export(None);
    let deletion_id = group.delete_message(target_id.clone())?;
    group.publish_messages().await?;
    let after = export(None);

    let parsed = elements(after.clone()).await;
    let position = |predicate: &dyn Fn(&Element) -> bool| {
        parsed
            .iter()
            .position(|element| element.element.as_ref().is_some_and(predicate))
            .unwrap()
    };
    let record_at = position(
        &|element| matches!(element, Element::MessageDeletion(save) if save.id == deletion_id),
    );
    let target_at =
        position(&|element| matches!(element, Element::GroupMessage(save) if save.id == target_id));
    assert!(record_at < target_at, "records come before messages");
    let Some(Element::GroupMessage(save)) = &parsed[target_at].element else {
        unreachable!()
    };
    assert!(save.decrypted_message_bytes.is_empty());
    assert_eq!(save.deleted_by_inbox_id.as_deref(), Some(alix.inbox_id()));

    tester!(fresh, from: alix, disable_workers);
    assert_eq!(import(&fresh.context, parsed.clone()).await, (1, 0));
    assert_eq!(
        mark_of(&fresh.db(), &target_id).as_deref(),
        Some(alix.inbox_id())
    );
    assert_eq!(
        import(&fresh.context, parsed).await,
        (0, 0),
        "a re-import changes nothing"
    );

    // An installation holds the target unmarked, then imports an archive
    // whose window holds no message but every record.
    tester!(older, from: alix, disable_workers);
    import(&older.context, elements(before.clone()).await).await;
    assert_eq!(mark_of(&older.db(), &target_id), None);
    let window = export(Some(xmtp_common::time::now_ns()));
    let window = elements(window).await;
    assert!(
        !window
            .iter()
            .any(|element| matches!(&element.element, Some(Element::GroupMessage(_))))
    );
    assert_eq!(import(&older.context, window).await, (1, 0));
    // An archived record never marks a message by itself.
    assert_eq!(mark_of(&older.db(), &target_id), None);
    assert!(older.db().get_message_deletion(&deletion_id)?.is_some());
    // The deleting installation's own record is from the topic: once the
    // target is removed locally, the old archive does not bring it back.
    alix.db().delete_message_by_id(&target_id)?;
    import(&alix.context, elements(before).await).await;
    let stored = alix.db().get_group_message(&target_id)?.unwrap();
    assert_eq!(stored.deleted_by_inbox_id.as_deref(), Some(alix.inbox_id()));
    assert!(stored.decrypted_message_bytes.is_empty());
}

/// An import larger than 10,000 messages: a target far into the archive is
/// marked by a deletion processed from the topic before it was stored, and
/// a marked row deep in the archive is stored marked.
// verifies: ARCH-028, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn import_larger_than_ten_thousand_messages() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let g = group.group_id;
    receive(
        &group,
        &deletion(g, D1, SENDER, DeliveryStatus::Published),
        false,
    );
    let filler = |n: u32| {
        let mut id = [0x33; 32];
        id[..4].copy_from_slice(&n.to_be_bytes());
        row(
            g,
            &id,
            MEMBER,
            text_with_authority("xmtp.org", 1),
            DeliveryStatus::Published,
        )
    };
    let mut elements: Vec<_> = (0..10_050u32).map(|n| legacy(&filler(n))).collect();
    elements.push(marked(&filler(20_000), MEMBER));
    elements.push(legacy(&target(g, DeliveryStatus::Published)));
    assert_eq!(import(&alix.context, elements).await, (1, 0));
    assert_eq!(mark_of(&alix.db(), &TARGET).as_deref(), Some(SENDER));
    assert_eq!(
        mark_of(&alix.db(), &filler(20_000).id).as_deref(),
        Some(MEMBER)
    );
    assert_eq!(mark_of(&alix.db(), &filler(0).id), None);
}

/// A message received from the network while an import runs is not marked
/// by the archive's records, even with a super-admin bit.
// verifies: ARCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn network_insert_during_import_is_unaffected() {
    use futures::StreamExt;
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let g = group.group_id;
    let db = alix.db();
    let elements = vec![
        legacy(&row(
            g,
            &[0x44; 32],
            MEMBER,
            text_with_authority("xmtp.org", 1),
            DeliveryStatus::Published,
        )),
        record(g, D2, ADMIN, true, 10),
        marked(&target(g, DeliveryStatus::Published), ADMIN),
    ];
    // The target arrives from the topic after the import started, just
    // before the archive's record for it is read.
    let mut stream = futures::stream::iter(elements.into_iter().enumerate()).map(|(n, element)| {
        if n == 1 {
            receive(&group, &target(g, DeliveryStatus::Published), false);
        }
        Ok::<_, std::io::Error>(element)
    });
    insert_elements(&mut stream, &alix.context, Default::default()).await?;
    let stored = db.get_group_message(TARGET)?.unwrap();
    assert_eq!(stored.deleted_by_inbox_id, None);
    assert!(!stored.decrypted_message_bytes.is_empty());
}

/// A read failure during the first pass fails the scan, so an import never
/// runs with some of the archive's deletion messages missing.
// verifies: ARCH-028
#[xmtp_common::test(unwrap_try = true)]
async fn incomplete_deletion_scan_fails() {
    let g = GroupId::from([0x55; 32]);
    let mut elements = futures::stream::iter([
        Ok(legacy(&deletion(g, D1, SENDER, DeliveryStatus::Published))),
        Err(std::io::Error::other("archive cut")),
    ]);
    assert!(collect_self_deletions(&mut elements).await.is_err());
}
