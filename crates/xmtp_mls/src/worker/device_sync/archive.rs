use std::collections::HashMap;

use super::DeviceSyncError;
use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, group_permissions::PolicySet},
    worker::device_sync::MissingField,
};
use futures::{Stream, StreamExt};
pub use xmtp_archive::*;
use xmtp_db::{
    ConnectionExt, XmtpMlsStorageProvider,
    consent_record::StoredConsentRecord,
    group::{ConversationType, GroupMembershipState},
    group_message::StoredGroupMessage,
    prelude::*,
};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

use xmtp_events::{ArchiveRestored, ClientEvent, EventWriter};
use xmtp_proto::types::GroupId;
#[derive(Default)]
struct ImportContext {
    group_timestamps: HashMap<Vec<u8>, Option<i64>>,
    changed: bool,
}

impl ImportContext {
    fn post_import(&mut self, context: &impl XmtpSharedContext) -> Result<(), DeviceSyncError> {
        use xmtp_db::diesel::prelude::*;
        use xmtp_db::schema::groups::dsl;

        // Keep a newer timestamp written by message receipt during the import.
        // Each group update acquires the writer and uses the current row value.
        for (group_id, timestamp) in &self.group_timestamps {
            let Some(timestamp) = *timestamp else {
                continue;
            };
            let changed = crate::state_tx::state_write(context.mls_storage(), |tx| {
                let storage = tx.storage();
                let changed = storage.db().raw_query(|conn| {
                    xmtp_db::diesel::update(dsl::groups.find(group_id))
                        .filter(
                            dsl::last_message_ns
                                .is_null()
                                .or(dsl::last_message_ns.lt(timestamp)),
                        )
                        .set(dsl::last_message_ns.eq(Some(timestamp)))
                        .execute(conn)
                })?;
                Ok::<_, xmtp_db::StorageError>(xmtp_db::TransactionOutcome::Continue(changed > 0))
            })?
            .into_continued();
            self.changed |= changed;
        }

        Ok(())
    }
}

pub async fn insert_importer(
    importer: &mut ArchiveImporter,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError> {
    insert_elements(importer, context).await
}

async fn insert_elements<S, E>(
    elements: &mut S,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
    DeviceSyncError: From<E>,
{
    let mut import_ctx = ImportContext::default();
    let result = async {
        while let Some(element) = elements.next().await {
            let element = element.map_err(DeviceSyncError::from)?;
            // Propagate insert failures to the supervisor rather than skipping the record.
            insert(element, context, &mut import_ctx)?;
        }
        import_ctx.post_import(context)?;
        Ok(())
    }
    .await;
    if import_ctx.changed {
        context.events().emit(
            Some(ClientEvent::ArchiveRestored(ArchiveRestored {
                complete: result.is_ok(),
            })),
            None,
        );
    }
    result
}

fn insert(
    element: BackupElement,
    context: &impl XmtpSharedContext,
    import_context: &mut ImportContext,
) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            import_context.changed |= context.db().insert_newer_consent_record(consent)?;
        }
        Element::Group(save) => {
            // Propagate a lookup error (incl. a dropped pool); only a genuine
            // "not found" falls through to restore the group.
            if let Some(existing_group) = context
                .db()
                .find_group(&GroupId::try_from(save.id.as_slice())?)?
            {
                let timestamp = match (existing_group.last_message_ns, save.last_message_ns) {
                    (Some(e), Some(s)) => Some(e.max(s)),
                    (None, Some(s)) => Some(s),
                    (Some(e), None) => Some(e),
                    (None, None) => None,
                };

                import_context
                    .group_timestamps
                    .insert(existing_group.id.to_vec(), timestamp);
                // Do not restore groups that already exist.
                return Ok(());
            }

            let conversation_type = save.conversation_type().try_into()?;
            let attributes = save
                .mutable_metadata
                .map(|m| m.attributes)
                .unwrap_or_default();

            // Imported messages update this field from their sent time.
            // Keep the archive timestamp too, including messages omitted by export filters.
            import_context
                .group_timestamps
                .insert(save.id.clone(), save.last_message_ns);
            let message_disappearing_settings =
                match (save.message_disappear_from_ns, save.message_disappear_in_ns) {
                    (Some(from_ns), Some(in_ns)) => {
                        Some(MessageDisappearingSettings::new(from_ns, in_ns))
                    }
                    _ => None,
                };

            let metadata_options = GroupMetadataOptions {
                name: attributes.get("group_name").cloned(),
                image_url_square: attributes.get("group_image_url_square").cloned(),
                description: attributes.get("description").cloned(),
                app_data: attributes.get("app_data").cloned(),
                message_disappearing_settings,
            };
            match conversation_type {
                ConversationType::Dm => {
                    let Some(dm_id) = save.dm_id else {
                        return Err(DeviceSyncError::MissingField(
                            MissingField::Conversation(super::ConversationField::DmId),
                            format!("DM with id of {:?} was missing the dm_id field.", save.id),
                        ));
                    };

                    let pair = crate::groups::parse_canonical_dm_id(Some(&dm_id))?;

                    MlsGroup::create_restored_dm_and_insert(
                        context,
                        pair,
                        metadata_options,
                        &save.id,
                    )?;
                }
                _ => {
                    MlsGroup::insert(
                        context,
                        Some(&save.id),
                        GroupMembershipState::Restored,
                        conversation_type,
                        PolicySet::default(),
                        metadata_options,
                        None,
                        false,
                    )?;
                }
            }
            import_context.changed = true;
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            // Keep validation and insertion under one writer. A rejected
            // message cannot enter stitched history between the two steps.
            let changed = crate::state_tx::state_write(
                context.mls_storage(),
                |tx| -> Result<_, GroupError> {
                    let storage = tx.storage();
                    let db = storage.db();
                    let existing: Option<StoredGroupMessage> = db.fetch(&message.id)?;
                    if existing.is_some() {
                        return Ok(xmtp_db::TransactionOutcome::Continue(false));
                    }
                    let group = db.find_group(&message.group_id)?.ok_or_else(|| {
                        xmtp_db::StorageError::NotFound(xmtp_db::NotFound::GroupById(
                            message.group_id,
                        ))
                    })?;
                    if group.conversation_type == ConversationType::Dm {
                        let pair = crate::groups::parse_canonical_dm_id(group.dm_id.as_deref())?;
                        if ![
                            pair.member_one_inbox_id.as_str(),
                            pair.member_two_inbox_id.as_str(),
                        ]
                        .contains(&message.sender_inbox_id.as_str())
                            || db.has_sender_outside_pair(
                                &message.group_id,
                                [&pair.member_one_inbox_id, &pair.member_two_inbox_id],
                            )?
                        {
                            return Err(crate::groups::MetadataPermissionsError::from(
                                crate::groups::DmValidationError::StoredMessageSenderOutsidePair,
                            )
                            .into());
                        }
                    }
                    Ok(xmtp_db::TransactionOutcome::Continue(
                        message.store_or_ignore_changed(&db)?,
                    ))
                },
            )?
            .into_continued();
            import_context.changed |= changed;
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(unused)]
    use super::*;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::tester;
    use crate::utils::{LocalTester, Tester};
    use crate::worker::device_sync::{ArchiveOptions, BackupElementSelection};
    use crate::{builder::ClientBuilder, utils::test::wait_for_min_intents};
    use diesel::prelude::*;
    use futures::AsyncReadExt;
    use futures::io::{BufReader, Cursor};
    use std::{path::Path, sync::Arc};
    use xmtp_archive::exporter::ArchiveExporter;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::group_message::MsgQueryArgs;
    use xmtp_db::{
        consent_record::StoredConsentRecord,
        group::StoredGroup,
        group_message::StoredGroupMessage,
        schema::{consent_records, group_messages, groups},
    };
    use xmtp_mls_common::{
        group::{DMMetadataOptions, GroupMetadataOptions},
        group_mutable_metadata::MessageDisappearingSettings,
    };

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
        assert!(insert_elements(&mut elements, &alix.context).await.is_err());
        assert!(
            alix.db()
                .get_consent_record(
                    "failed-import".into(),
                    xmtp_db::consent_record::ConsentType::InboxId,
                )?
                .is_some()
        );
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ArchiveRestored(restored)), ..
            }] if !restored.complete
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_timestamp_keeps_a_message_received_during_import() {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let mut pending_import = ImportContext {
            group_timestamps: [(group.group_id.to_vec(), Some(0))].into(),
            ..Default::default()
        };

        group.send_message_optimistic(b"message during import", Default::default())?;
        let current = alix.db().find_group(&group.group_id)??.last_message_ns;
        assert!(current > Some(0));
        pending_import.post_import(&alix.context)?;
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
            let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
            exporter.read_to_end(&mut file).await?;
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

            let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
            exporter.read_to_end(&mut file).await?;
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
        crate::mls_store::MlsStore::new(alix2.context.clone())
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
            crate::groups::mls_sync::GroupHeadOutcome::Inactive
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

    // verifies: DMS-015, ARCH-013, ARCH-021, DMS-014
    #[xmtp_common::test(unwrap_try = true)]
    async fn authentic_archive_rejects_outside_dm_sender_before_stitched_history() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let source = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let good_id = source
            .send_message(b"pair history", Default::default())
            .await?;
        let mut outside = alix
            .db()
            .get_group_message(&good_id)?
            .expect("source message");
        // Export orders messages by id. Keep the valid element first so
        // Completed-prefix behavior is deterministic.
        outside.id = vec![0xff; 32];
        outside.sender_inbox_id = hex::encode([0x43; 32]);
        outside.sender_installation_id = vec![0x43; 32];
        outside.sent_at_ns += 1;
        outside.sequence_id += 1000;
        outside.idempotency_key = "older-poisoned-dm".into();
        outside.store(&alix.db())?;

        // The normal exporter authenticates both messages under the user's key.
        let key = vec![0x29; 32];
        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![BackupElementSelection::Messages],
            exclude_disappearing_messages: false,
        };
        let mut export = vec![];
        ArchiveExporter::new(opts, alix.db(), &key)
            .read_to_end(&mut export)
            .await?;

        tester!(alix2, from: alix);
        let active = alix2.find_or_create_dm(bo.inbox_id(), None).await?;
        assert_ne!(active.group_id, source.group_id);
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        assert!(
            insert_importer(&mut importer, &alix2.context)
                .await
                .is_err()
        );

        let restored = alix2
            .db()
            .find_group(&source.group_id)?
            .expect("restored DM");
        assert_eq!(restored.membership_state, GroupMembershipState::Restored);
        assert!(alix2.db().get_group_message(&good_id)?.is_some());
        assert!(alix2.db().get_group_message(&outside.id)?.is_none());
        assert!(
            !alix2
                .db()
                .has_sender_outside_pair(&source.group_id, [alix2.inbox_id(), bo.inbox_id()],)?
        );
        let stitched = active.find_messages(&MsgQueryArgs::default())?;
        assert!(stitched.iter().any(|message| message.id == good_id));
        assert!(!stitched.iter().any(|message| message.id == outside.id));
    }

    // verifies: ARCH-015, ARCH-022, DMS-015
    #[xmtp_common::test(unwrap_try = true)]
    async fn foreign_archive_preserves_dm_pair_and_pair_history() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(charlie, disable_workers);
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
        ArchiveExporter::new(opts.clone(), alix.db(), &key)
            .read_to_end(&mut export)
            .await?;
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &charlie.context).await?;

        let stored = charlie
            .db()
            .find_group(&source.group_id)?
            .expect("Restored DM");
        assert_eq!(stored.membership_state, GroupMembershipState::Restored);
        assert_eq!(stored.dm_id.as_deref(), Some(archived_dm_id.as_str()));
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
        for (id, sender, bytes) in [
            (&alix_id, alix.inbox_id(), b"from alix".as_slice()),
            (&bo_id, bo.inbox_id(), b"from bo".as_slice()),
        ] {
            let message = charlie.db().get_group_message(id)?.expect("pair history");
            assert_eq!(message.sender_inbox_id, sender);
            assert_eq!(message.decrypted_message_bytes, bytes);
        }
        let mut outside: xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave = charlie
            .db()
            .get_group_message(&alix_id)?
            .expect("pair message")
            .into();
        outside.id = vec![0xe3; 32];
        outside.sender_inbox_id = charlie.inbox_id().to_string();
        let mut rejected = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::GroupMessage(outside.clone())),
        })]);
        assert!(
            insert_elements(&mut rejected, &charlie.context)
                .await
                .is_err()
        );
        assert!(charlie.db().get_group_message(&outside.id)?.is_none());
        crate::builder::ClientBuilder::from_client(charlie.client.clone())
            .with_disable_workers(true)
            .build()
            .await?;
    }

    // verifies: DMS-015, ARCH-013
    #[xmtp_common::test(unwrap_try = true)]
    async fn joined_dm_archive_import_keeps_duplicate_and_rejects_outside_sender() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let good_id = dm.send_message(b"known", Default::default()).await?;
        let good = alix
            .db()
            .get_group_message(&good_id)?
            .expect("known message");
        let mut save: xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave =
            good.clone().into();
        save.sender_inbox_id = hex::encode([0x43; 32]);
        save.decrypted_message_bytes = b"ignored replacement".to_vec();
        let mut duplicate = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::GroupMessage(save.clone())),
        })]);
        insert_elements(&mut duplicate, &alix.context).await?;
        assert_eq!(alix.db().get_group_message(&good_id)?, Some(good.clone()));
        assert!(
            !alix
                .db()
                .has_sender_outside_pair(&dm.group_id, [alix.inbox_id(), bo.inbox_id()],)?
        );

        save.id = vec![0x44; 32];
        let mut new_message = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
            element: Some(Element::GroupMessage(save.clone())),
        })]);
        assert!(
            insert_elements(&mut new_message, &alix.context)
                .await
                .is_err()
        );
        assert!(alix.db().get_group_message(&save.id)?.is_none());
    }

    // verifies: DMS-015, ARCH-021
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
        assert!(insert_elements(&mut elements, &alix.context).await.is_err());
        assert!(
            alix.db()
                .find_group(&GroupId::try_from(group_id.as_slice())?)?
                .is_none()
        );
    }

    // verifies: EVENT-001, EVENT-017
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_buffer_export_import() {
        use futures::io::BufReader;
        use futures_util::AsyncReadExt;

        tester!(alix);
        tester!(bo);

        let alix_group = alix.create_group(None, None).unwrap();
        alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
        alix_group
            .send_message(b"hello there", SendMessageOpts::default())
            .await
            .unwrap();

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };

        let key = vec![7; 32];

        let file = {
            let mut file = Vec::new();
            let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
            exporter.read_to_end(&mut file).await.unwrap();
            file
        };

        let alix2_wallet = generate_local_wallet();
        let alix2 = ClientBuilder::new_test_client(&alix2_wallet).await;
        let events = alix2.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ArchiveRestored]),
            Some(4),
        );

        // No messages
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query(|conn| {
                group_messages::table
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })
            .unwrap();
        assert_eq!(messages.len(), 0);

        let reader = BufReader::new(Cursor::new(file.clone()));
        let reader = Box::pin(reader);
        let mut importer = ArchiveImporter::load(reader, &key).await.unwrap();
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(xmtp_events::ClientEvent::ArchiveRestored(restored)), ..
            }] if restored.complete
        ));

        let reader = Box::pin(BufReader::new(Cursor::new(file)));
        let mut second = ArchiveImporter::load(reader, &key).await.unwrap();
        insert_importer(&mut second, &alix2.context).await.unwrap();
        assert!(events.drain().is_empty());

        // One message.
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query(|conn| {
                group_messages::table
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    // verifies: JOIN-080
    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_file_backup() {
        use crate::{groups::send_message_opts::SendMessageOpts, tester};
        use diesel::QueryDsl;
        use xmtp_db::group::{ConversationType, GroupQueryArgs};

        tester!(alix, sync_worker, triggers);
        tester!(bo);

        let alix_group = alix.create_group(None, None)?;

        // wait for user preference update
        wait_for_min_intents(&alix.context.db(), 2).await?;

        alix_group.add_members(&[bo.inbox_id()]).await?;
        alix_group.update_group_name("My group".to_string()).await?;

        bo.sync_welcomes().await?;
        let bo_group = bo.group(&alix_group.group_id)?;

        // wait for add member intent/commit
        wait_for_min_intents(&alix.context.db(), 1).await?;

        alix_group
            .send_message(b"hello there", SendMessageOpts::default())
            .await?;

        // wait for send message intent/commit publish
        // Wait for Consent state update
        wait_for_min_intents(&alix.context.db(), 4).await?;

        let mut consent_records: Vec<StoredConsentRecord> = alix
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        let old_consent_record = consent_records.pop()?;

        let mut groups: Vec<StoredGroup> = alix
            .context
            .db()
            .raw_query(|conn| groups::table.load(conn))?;
        assert_eq!(groups.len(), 2);
        let old_group = groups.pop()?;

        let old_messages: Vec<StoredGroupMessage> = alix.context.db().raw_query(|conn| {
            group_messages::table
                .select(StoredGroupMessage::as_select())
                .load(conn)
        })?;
        assert_eq!(old_messages.len(), 4);

        let opts = ArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages,
                BackupElementSelection::Consent,
            ],
            exclude_disappearing_messages: false,
        };

        let key = xmtp_common::rand_vec::<32>();
        let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
        let path = Path::new("archive.xmtp");
        let _ = tokio::fs::remove_file(path).await;
        exporter.write_to_file(path).await?;

        tester!(alix2, sync_worker);
        alix2.device_sync_client().wait_for_sync_worker_init().await;

        // No consent before
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 0);

        let mut importer = ArchiveImporter::from_file(path, &key).await?;
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();

        // Consent is there after the import
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        // It's the same consent record.
        assert_eq!(consent_records[0], old_consent_record);

        let groups: Vec<StoredGroup> = alix2.context.db().raw_query(|conn| {
            groups::table
                .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
                .load(conn)
        })?;
        assert_eq!(groups.len(), 1);
        // It's the same group
        assert_eq!(groups[0].id, old_group.id);

        let messages: Vec<StoredGroupMessage> = alix2.context.db().raw_query(|conn| {
            group_messages::table
                .select(StoredGroupMessage::as_select())
                .filter(group_messages::group_id.eq(&groups[0].id))
                .load(conn)
        })?;
        // Only the application messages should sync
        assert_eq!(messages.len(), 1);
        for msg in messages {
            let old_msg = old_messages.iter().find(|m| msg.id == m.id)?;
            assert_eq!(old_msg.authority_id, msg.authority_id);
            assert_eq!(old_msg.decrypted_message_bytes, msg.decrypted_message_bytes);
            assert_eq!(old_msg.sent_at_ns, msg.sent_at_ns);
            assert_eq!(old_msg.sender_installation_id, msg.sender_installation_id);
            assert_eq!(old_msg.sender_inbox_id, msg.sender_inbox_id);
            assert_eq!(old_msg.group_id, msg.group_id);
        }

        let alix2_group = alix2.group(&old_group.id)?;
        // Loading all the groups works fine
        let _groups = alix2.find_groups(GroupQueryArgs::default())?;
        // Can fetch the group name no problem
        alix2_group.group_name()?;
        assert!(!alix2_group.is_active()?);

        // Add the new inbox to the groups
        alix.group(&old_group.id)?
            .add_members(&[alix2.inbox_id()])
            .await?;
        alix2.sync_welcomes().await?;

        // The group restores to being fully functional
        let alix2_group = alix2.group(&old_group.id)?;
        assert!(alix2_group.is_active()?);

        // The old messages should be stitched in
        let msgs = alix2_group.find_messages(&MsgQueryArgs::default())?;
        let old_msg_exists = msgs
            .iter()
            .any(|msg| msg.decrypted_message_bytes == b"hello there");
        assert!(old_msg_exists);

        // Bo should see the new message from alix2
        alix2_group
            .send_message(b"this should send", SendMessageOpts::default())
            .await?;
        bo_group.sync().await?;
        let msgs = bo_group.find_messages(&MsgQueryArgs::default())?;
        let new_msg_exists = msgs
            .iter()
            .any(|msg| msg.decrypted_message_bytes == b"this should send");
        assert!(new_msg_exists);

        // cleanup
        let _ = tokio::fs::remove_file(path).await;
    }

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_legacy_archive_import() {
        use std::path::PathBuf;

        use crate::tester;

        let key = vec![0; 32];
        let path = PathBuf::from("tests/assets/archive-legacy.xmtp");
        let mut importer = ArchiveImporter::from_file(path, &key).await?;

        tester!(alix);

        let result = insert_importer(&mut importer, &alix.context).await;
        assert!(result.is_ok());
    }

    /// This archive was generated by the legacy creation build before groups
    /// were born with an AppData dictionary. Import must preserve the saved
    /// metadata while it creates dictionary-native backup stubs.
    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_import_current_legacy_archive_metadata() {
        use std::path::PathBuf;

        use crate::groups::group_permissions::PolicySet;
        use xmtp_db::group::ConversationType;

        let key = vec![9; 32];
        let path = PathBuf::from("tests/assets/archive-current-legacy-metadata.xmtp");
        let mut preview = ArchiveImporter::from_file(path.clone(), &key).await?;
        let mut archived_dm_id = None;
        while let Some(element) = preview.next().await {
            if let Some(Element::Group(group)) = element?.element {
                archived_dm_id = archived_dm_id.or(group.dm_id);
            }
        }
        let archived_dm_id = archived_dm_id.expect("legacy DM pair");
        let mut importer = ArchiveImporter::from_file(path, &key).await?;
        tester!(alix, disable_workers);
        let pair = crate::groups::parse_canonical_dm_id(Some(&archived_dm_id))?;
        assert_ne!(pair.member_one_inbox_id, alix.inbox_id());
        assert_ne!(pair.member_two_inbox_id, alix.inbox_id());
        insert_importer(&mut importer, &alix.context).await?;

        let groups: Vec<StoredGroup> = alix
            .context
            .db()
            .raw_query(|conn| groups::table.load(conn))?;
        assert_eq!(groups.len(), 2);
        for record in &groups {
            let group = alix.group(&record.id)?;
            group.with_group_snapshot(|mls| {
                assert!(mls.extensions().app_data_dictionary().is_some());
                Ok(())
            })?;
        }

        let restored_group_record = groups
            .iter()
            .find(|group| group.conversation_type == ConversationType::Group)
            .expect("legacy group archive entry");
        assert_eq!(restored_group_record.message_disappear_from_ns, Some(123));
        assert_eq!(restored_group_record.message_disappear_in_ns, Some(456));
        let restored_group = alix.group(&restored_group_record.id)?;
        assert_eq!(restored_group.group_name()?, "legacy archive group name");
        assert_eq!(
            restored_group.group_description()?,
            "legacy archive group description"
        );
        assert_eq!(
            restored_group.group_image_url_square()?,
            "https://example.com/legacy-archive.png"
        );
        assert_eq!(restored_group.app_data()?, "legacy archive app data");
        assert_eq!(restored_group.permissions()?.policies, PolicySet::default());

        let restored_dm_record = groups
            .iter()
            .find(|group| group.conversation_type == ConversationType::Dm)
            .expect("legacy DM archive entry");
        assert_eq!(
            restored_dm_record.dm_id.as_deref(),
            Some(archived_dm_id.as_str())
        );
        assert_eq!(restored_dm_record.message_disappear_from_ns, Some(123));
        assert_eq!(restored_dm_record.message_disappear_in_ns, Some(456));
        let restored_dm = alix.group(&restored_dm_record.id)?;
        assert_eq!(restored_dm.group_name()?, "legacy archive dm name");
        assert_eq!(
            restored_dm.group_description()?,
            "legacy archive dm description"
        );
        assert_eq!(
            restored_dm.group_image_url_square()?,
            "https://example.com/legacy-dm-archive.png"
        );
        assert_eq!(restored_dm.app_data()?, "legacy archive dm app data");
    }

    /// Dictionary metadata must survive archive export and import.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_archive_includes_dictionary_groups() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);

        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        alix_group.send_message(b"hi", Default::default()).await?;

        // Update metadata on the dictionary-native group so the exported
        // values come from the AppData dictionary.

        alix_group
            .update_group_name("post-migration name".to_string())
            .await?;
        alix_group
            .update_group_description("post-migration description".to_string())
            .await?;
        alix_group
            .update_group_image_url_square("https://example.com/post-migration.png".to_string())
            .await?;

        // A second group must restore alongside the first group.
        let second_group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        second_group
            .update_group_name("second group name".to_string())
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
            let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
            exporter.read_to_end(&mut file).await?;
            file
        };

        // Fresh installation of the same inbox restores from the
        // archive only (workers disabled, no welcome sync).
        tester!(alix2, from: alix);
        let reader = Box::pin(BufReader::new(Cursor::new(export)));
        let mut importer = ArchiveImporter::load(reader, &key).await?;
        insert_importer(&mut importer, &alix2.context).await?;

        let restored = alix2.db().find_group(&alix_group.group_id)?;
        assert!(
            restored.is_some(),
            "dictionary group missing from restored archive"
        );

        // Presence is not enough: the metadata written before export
        // must round-trip through the archive, or per-field loss in
        // the exporter's dict read would go unnoticed.
        let restored_group = alix2.group(&alix_group.group_id)?;
        assert_eq!(restored_group.group_name()?, "post-migration name");
        assert_eq!(
            restored_group.group_description()?,
            "post-migration description"
        );
        assert_eq!(
            restored_group.group_image_url_square()?,
            "https://example.com/post-migration.png"
        );

        let restored_second = alix2.group(&second_group.group_id)?;
        assert_eq!(restored_second.group_name()?, "second group name");
    }
}
