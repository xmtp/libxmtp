use std::collections::HashMap;

use super::DeviceSyncError;
use crate::{
    context::XmtpSharedContext,
    groups::{MlsGroup, device_sync::MissingField, group_permissions::PolicySet},
};
use futures::StreamExt;
pub use xmtp_archive::*;
use xmtp_db::{
    ConnectionExt, StoreOrIgnore,
    consent_record::StoredConsentRecord,
    group::{ConversationType, DmIdExt, GroupMembershipState},
    group_message::StoredGroupMessage,
    prelude::*,
};
use xmtp_mls_common::group::{DMMetadataOptions, GroupMetadataOptions};
use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

#[derive(Default)]
struct ImportContext {
    group_timestamps: HashMap<Vec<u8>, Option<i64>>,
}

impl ImportContext {
    fn post_import(&self, context: &impl XmtpSharedContext) -> Result<(), DeviceSyncError> {
        use xmtp_db::diesel::prelude::*;
        use xmtp_db::schema::groups::dsl;

        // We want to update the group timestamps to either be what they were before the import,
        // or what they are in the archive group field.
        for (group_id, timestamp) in &self.group_timestamps {
            if let Err(err) = context.db().raw_query_write(|conn| {
                xmtp_db::diesel::update(dsl::groups.find(group_id))
                    .set(dsl::last_message_ns.eq(*timestamp))
                    .execute(conn)
            }) {
                tracing::warn!("Unable to update last_message_ns for group {group_id:?}: {err:?}");
            }
        }

        Ok(())
    }
}

pub async fn insert_importer(
    importer: &mut ArchiveImporter,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError> {
    let mut import_ctx = ImportContext::default();

    while let Some(element) = importer.next().await {
        let element = element?;
        if let Err(err) = insert(element, context, &mut import_ctx) {
            tracing::warn!("Unable to insert record: {err:?}");
        };
    }

    import_ctx.post_import(context)?;

    Ok(())
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
            context.db().insert_newer_consent_record(consent)?;
        }
        Element::Group(save) => {
            if let Ok(Some(existing_group)) = context.db().find_group(&save.id) {
                let timestamp = match (existing_group.last_message_ns, save.last_message_ns) {
                    (Some(e), Some(s)) => Some(e.max(s)),
                    (None, Some(s)) => Some(s),
                    (Some(e), None) => Some(e),
                    (None, None) => None,
                };

                import_context
                    .group_timestamps
                    .insert(existing_group.id, timestamp);
                // Do not restore groups that already exist.
                return Ok(());
            }

            let conversation_type = save.conversation_type().try_into()?;
            let attributes = save
                .mutable_metadata
                .map(|m| m.attributes)
                .unwrap_or_default();

            // Save the timestamp. We'll need to come back around and re-insert this
            // because triggers will set this field to now_ns in the database which
            // is sub-par UX.
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

            match conversation_type {
                ConversationType::Dm => {
                    let Some(dm_id) = save.dm_id else {
                        return Err(DeviceSyncError::MissingField(
                            MissingField::Conversation(super::ConversationField::DmId),
                            format!("DM with id of {:?} was missing the dm_id field.", save.id),
                        ));
                    };

                    let target_inbox_id = dm_id.other_inbox_id(context.inbox_id());

                    MlsGroup::create_dm_and_insert(
                        context,
                        GroupMembershipState::Restored,
                        target_inbox_id,
                        DMMetadataOptions {
                            message_disappearing_settings,
                        },
                        Some(&save.id),
                    )?;
                }
                _ => {
                    MlsGroup::insert(
                        context,
                        Some(&save.id),
                        GroupMembershipState::Restored,
                        ConversationType::Group,
                        PolicySet::default(),
                        GroupMetadataOptions {
                            name: attributes.get("group_name").cloned(),
                            image_url_square: attributes.get("group_image_url_square").cloned(),
                            description: attributes.get("description").cloned(),
                            app_data: attributes.get("app_data").cloned(),
                            message_disappearing_settings,
                        },
                        None,
                    )?;
                }
            }
        }
        Element::GroupMessage(message) => {
            let message: StoredGroupMessage = message.try_into()?;
            message.store_or_ignore(&context.db())?;
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
    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions, utils::test::wait_for_min_intents,
    };
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
    use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_archive_timestamps() {
        tester!(alix, disable_workers);
        tester!(alix2, from: alix);
        tester!(bo, disable_workers);

        let alix_group = alix
            .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
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
        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages as i32,
                BackupElementSelection::Consent as i32,
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

        let alix_bo_dm = alix
            .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
            .await?;
        alix_bo_dm
            .send_message(b"old group", Default::default())
            .await?;

        let timestamp = alix
            .db()
            .find_group(&alix_bo_dm.group_id)??
            .last_message_ns?;

        let key = vec![7; 32];
        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages as i32,
                BackupElementSelection::Consent as i32,
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

        let alix2_bo_dm = alix2
            .find_or_create_dm_by_inbox_id(bo.inbox_id(), None)
            .await?;
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

        alix2_bo_dm
            .send_message(b"hi bo", Default::default())
            .await?;

        bo.sync_all_welcomes_and_groups(None).await?;
        let bo_alix2_dm = bo.group(&alix2_bo_dm.group_id)?;
        assert_eq!(bo_alix2_dm.test_last_message_bytes().await??, b"hi bo");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_buffer_export_import() {
        use futures::io::BufReader;
        use futures_util::AsyncReadExt;

        tester!(alix);
        tester!(bo);

        let alix_group = alix.create_group(None, None).unwrap();
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        alix_group
            .send_message(b"hello there", SendMessageOpts::default())
            .await
            .unwrap();

        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages as i32,
                BackupElementSelection::Consent as i32,
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

        // No messages
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        assert_eq!(messages.len(), 0);

        let reader = BufReader::new(Cursor::new(file));
        let reader = Box::pin(reader);
        let mut importer = ArchiveImporter::load(reader, &key).await.unwrap();
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();

        // One message.
        let messages: Vec<StoredGroupMessage> = alix2
            .context
            .db()
            .raw_query_read(|conn| group_messages::table.load(conn))
            .unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_file_backup() {
        use crate::{groups::send_message_opts::SendMessageOpts, tester};
        use diesel::QueryDsl;
        use xmtp_db::group::{ConversationType, GroupQueryArgs};

        tester!(alix, sync_worker, sync_server, triggers);
        tester!(bo);

        let alix_group = alix.create_group(None, None)?;

        // wait for user preference update
        wait_for_min_intents(&alix.context.db(), 2).await?;

        alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;
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
            .raw_query_read(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        let old_consent_record = consent_records.pop()?;

        let mut groups: Vec<StoredGroup> = alix
            .context
            .db()
            .raw_query_read(|conn| groups::table.load(conn))?;
        assert_eq!(groups.len(), 2);
        let old_group = groups.pop()?;

        let old_messages: Vec<StoredGroupMessage> = alix
            .context
            .db()
            .raw_query_read(|conn| group_messages::table.load(conn))?;
        assert_eq!(old_messages.len(), 5);

        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages.into(),
                BackupElementSelection::Consent.into(),
            ],
            exclude_disappearing_messages: false,
        };

        let key = xmtp_common::rand_vec::<32>();
        let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
        let path = Path::new("archive.xmtp");
        let _ = tokio::fs::remove_file(path).await;
        exporter.write_to_file(path).await?;

        tester!(alix2, sync_worker, sync_server);
        alix2.device_sync_client().wait_for_sync_worker_init().await;

        // No consent before
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query_read(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 0);

        let mut importer = ArchiveImporter::from_file(path, &key).await?;
        insert_importer(&mut importer, &alix2.context)
            .await
            .unwrap();

        // Consent is there after the import
        let consent_records: Vec<StoredConsentRecord> = alix2
            .context
            .db()
            .raw_query_read(|conn| consent_records::table.load(conn))?;
        assert_eq!(consent_records.len(), 1);
        // It's the same consent record.
        assert_eq!(consent_records[0], old_consent_record);

        let groups: Vec<StoredGroup> = alix2.context.db().raw_query_read(|conn| {
            groups::table
                .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
                .load(conn)
        })?;
        assert_eq!(groups.len(), 1);
        // It's the same group
        assert_eq!(groups[0].id, old_group.id);

        let messages: Vec<StoredGroupMessage> = alix2.context.db().raw_query_read(|conn| {
            group_messages::table
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
            .add_members_by_inbox_id(&[alix2.inbox_id()])
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
}
