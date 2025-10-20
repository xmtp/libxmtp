use super::DeviceSyncError;
use crate::{
    context::XmtpSharedContext,
    groups::{MlsGroup, group_permissions::PolicySet},
};
use futures::StreamExt;
pub use xmtp_archive::*;
use xmtp_db::{
    StoreOrIgnore,
    consent_record::StoredConsentRecord,
    group::{ConversationType, GroupMembershipState},
    group_message::StoredGroupMessage,
    prelude::*,
};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

pub async fn insert_importer(
    importer: &mut ArchiveImporter,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError> {
    while let Some(element) = importer.next().await {
        let element = element?;
        if let Err(err) = insert(element, context) {
            tracing::warn!("Unable to insert record: {err:?}");
        };
    }

    Ok(())
}

fn insert(element: BackupElement, context: &impl XmtpSharedContext) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        Element::Consent(consent) => {
            let consent: StoredConsentRecord = consent.try_into()?;
            context.db().insert_newer_consent_record(consent)?;
        }
        Element::Group(save) => {
            if let Ok(Some(_)) = context.db().find_group(&save.id) {
                // Do not restore groups that already exist.
                return Ok(());
            }

            let attributes = save
                .mutable_metadata
                .map(|m| m.attributes)
                .unwrap_or_default();

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
                    ..Default::default()
                },
                None,
            )?;
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
    use crate::utils::{LocalTester, Tester};
    use crate::{
        builder::ClientBuilder, groups::GroupMetadataOptions, utils::test::wait_for_min_intents,
    };
    use diesel::prelude::*;
    use futures::io::Cursor;
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

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_buffer_export_import() {
        use futures::io::BufReader;
        use futures_util::AsyncReadExt;

        let alix = Tester::new().await;
        let bo = Tester::new().await;

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
        assert_eq!(old_messages.len(), 6);

        let opts = BackupOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![
                BackupElementSelection::Messages.into(),
                BackupElementSelection::Consent.into(),
            ],
        };

        let key = xmtp_common::rand_vec::<32>();
        let mut exporter = ArchiveExporter::new(opts, alix.db(), &key);
        let path = Path::new("archive.xmtp");
        let _ = tokio::fs::remove_file(path).await;
        exporter.write_to_file(path).await?;

        let alix2 = Tester::new().await;
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
        alix_group
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
}
