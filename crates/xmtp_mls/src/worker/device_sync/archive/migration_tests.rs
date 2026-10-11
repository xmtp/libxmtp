//! The legacy exporter must feed the existing SDK importer.
use super::*;
use crate::tester;
use diesel::{connection::SimpleConnection, prelude::*};
use prost::Message as _;
use xmtp_db::{
    ConnectionExt,
    consent_record::{ConsentState, ConsentType, QueryConsentRecord},
    group::{GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, StoredGroupMessage},
    schema::{group_messages, groups},
};
use xmtp_legacy_migration::{PrepareMigrationArchiveArgs, prepare_migration_archive};
use xmtp_proto::xmtp::message_contents::{ContentTypeId, EncodedContent};

const KEY: [u8; 32] = [7; 32];
const DM: &str = "a06859a4aebe75c970b8875698fbeddb";

// verifies: MIG-001, MIG-003, MIG-005, MIG-006, ARCH-007, ARCH-010, CONS-002
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_migration_imports_into_empty_and_populated_stores_and_retries() {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("legacy.db3");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xmtp_legacy_migration/fixtures/consent-states.db3"
        ),
        &source,
    )?;
    let mut legacy = SqliteConnection::establish(source.to_str().unwrap())?;
    diesel::sql_query("UPDATE group_messages SET delivery_status=1 WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(vec![8; 32])
        .execute(&mut legacy)?;
    diesel::sql_query("UPDATE group_messages SET delivery_status=3 WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(vec![10; 32])
        .execute(&mut legacy)?;
    // Keep the pinned legacy deletion authority and content-type rules.
    legacy.batch_execute("PRAGMA foreign_keys=OFF;")?;
    for id in 20..=26u8 {
        diesel::sql_query("INSERT INTO group_messages(id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, sender_inbox_id, delivery_status, content_type, authority_id, originator_id, sequence_id) SELECT ?, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, sender_inbox_id, 2, ?, authority_id, originator_id, sequence_id FROM group_messages WHERE id=?")
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .bind::<diesel::sql_types::Integer, _>(if id == 20 { 3 } else if id == 24 { 0 } else { 1 })
            .bind::<diesel::sql_types::Binary, _>(vec![1; 32])
            .execute(&mut legacy)?;
        diesel::sql_query("INSERT INTO message_deletions(id, group_id, deleted_message_id, deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) SELECT ?, ?, ?, ?, ?, 1700000000000000999")
            .bind::<diesel::sql_types::Binary, _>(vec![id + 100; 32])
            .bind::<diesel::sql_types::Binary, _>(if id == 23 { vec![0x66; 16] } else { hex::decode(DM)? })
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .bind::<diesel::sql_types::Text, _>(if id == 20 || id == 24 || id >= 25 { "01".repeat(32) } else { "02".repeat(32) })
            .bind::<diesel::sql_types::Bool, _>(id == 21 || id == 23)
            .execute(&mut legacy)?;
    }
    let non_deletable = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "example.test".into(),
            type_id: "custom".into(),
            version_major: 1,
            version_minor: 0,
        }),
        content: b"opaque non-deletable fixture".to_vec(),
        ..Default::default()
    }
    .encode_to_vec();
    diesel::sql_query("UPDATE group_messages SET decrypted_message_bytes=? WHERE id=?")
        .bind::<diesel::sql_types::Binary, _>(&non_deletable)
        .bind::<diesel::sql_types::Binary, _>(vec![24; 32])
        .execute(&mut legacy)?;
    // The pinned legacy classifier uses type_id only, including other
    // authorities and major versions. Current catalogue rules do not apply.
    for (id, authority, major) in [(25, "custom.example", 1), (26, "xmtp.org", 2)] {
        let content = EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: authority.into(),
                type_id: "text".into(),
                version_major: major,
                version_minor: 0,
            }),
            content: b"deleted legacy text".to_vec(),
            ..Default::default()
        }
        .encode_to_vec();
        diesel::sql_query("UPDATE group_messages SET decrypted_message_bytes=? WHERE id=?")
            .bind::<diesel::sql_types::Binary, _>(&content)
            .bind::<diesel::sql_types::Binary, _>(vec![id; 32])
            .execute(&mut legacy)?;
    }
    drop(legacy);
    let output = directory.path().join("migration.xmtp");
    let report = prepare_migration_archive(PrepareMigrationArchiveArgs {
        database_path: source.to_str().unwrap().into(),
        database_key: None,
        archive_key: KEY.to_vec(),
        output_path: output.to_str().unwrap().into(),
    })
    .await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (2, 6, 3)
    );
    for populated in [false, true] {
        tester!(destination, disable_workers);
        let original = if populated {
            let group = destination.create_group(None, None)?;
            let message = group
                .send_message(b"destination history", Default::default())
                .await?;
            Some((group.group_id, message))
        } else {
            None
        };
        for _ in 0..2 {
            let mut importer = ArchiveImporter::from_file(&output, &KEY).await?;
            insert_importer(&mut importer, &destination.context).await?;
            let restored: Vec<StoredGroup> = destination.db().raw_query(|conn| {
                groups::table
                    .filter(groups::membership_state.eq(GroupMembershipState::Restored as i32))
                    .load(conn)
            })?;
            assert_eq!(restored.len(), 2);
            for (prefix, state) in [
                ("02", ConsentState::Unknown),
                ("03", ConsentState::Allowed),
                ("04", ConsentState::Denied),
            ] {
                let consent = destination
                    .db()
                    .get_consent_record(prefix.repeat(32), ConsentType::InboxId)?
                    .expect("imported consent record");
                assert_eq!(consent.state, state);
                assert_eq!(consent.consented_at_ns, 1700000000000000009);
            }
            let dm = restored
                .iter()
                .find(|g| hex::encode(g.id) == DM)
                .expect("original DM id");
            assert_eq!(
                dm.dm_id.as_deref(),
                Some(format!("dm:{}:{}", "01".repeat(32), "02".repeat(32)).as_str())
            );
            assert_eq!(destination.group(&dm.id)?.group_name()?, "Migration DM");
            let messages: Vec<StoredGroupMessage> = destination.db().raw_query(|conn| {
                group_messages::table
                    .filter(group_messages::group_id.eq_any(restored.iter().map(|group| &group.id)))
                    .order(group_messages::id)
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })?;
            assert_eq!(
                messages
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<Vec<_>>(),
                [1, 8, 10, 22, 23, 24].map(|id| vec![id; 32])
            );
            for message in messages {
                assert_eq!(
                    message.delivery_status,
                    match message.id[0] {
                        1 | 22 | 23 | 24 => DeliveryStatus::Published,
                        8 => DeliveryStatus::Unpublished,
                        10 => DeliveryStatus::Failed,
                        id => panic!("unexpected migrated message: {id}"),
                    }
                );
                assert_eq!(
                    message.sent_at_ns,
                    if message.id == vec![10; 32] {
                        1500000000000000000
                    } else {
                        1700000000000000123
                    }
                );
                assert_eq!(
                    hex::encode(message.decrypted_message_bytes),
                    if message.id == vec![24; 32] {
                        hex::encode(&non_deletable)
                    } else {
                        "0a120a08786d74702e6f7267120474657874180122166d6967726174696f6e20666978747572652074657874".to_owned()
                    }
                );
            }
            if let Some((id, message)) = &original {
                let group = destination
                    .db()
                    .find_group(id)?
                    .expect("existing destination group");
                assert_ne!(group.membership_state, GroupMembershipState::Restored);
                let count: i64 = destination.db().raw_query(|conn| {
                    group_messages::table
                        .filter(group_messages::id.eq(message))
                        .count()
                        .get_result(conn)
                })?;
                assert_eq!(count, 1);
            }
        }
    }
}
