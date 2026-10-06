//! The standalone exporter must feed the existing SDK importer without changes.
use super::*;
use crate::tester;
use diesel::prelude::*;
use futures::TryStreamExt;
use xmtp_db::{
    ConnectionExt,
    group::{GroupMembershipState, StoredGroup},
    group_message::StoredGroupMessage,
    schema::{group_messages, groups},
};
use xmtp_legacy_migration::{PrepareMigrationArchiveArgs, prepare_migration_archive};

const KEY: [u8; 32] = [7; 32];
const DM: &str = "a06859a4aebe75c970b8875698fbeddb";

// verifies: MIG-001, MIG-003, MIG-005, MIG-006, ARCH-007, ARCH-010
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_migration_imports_into_empty_and_populated_stores_and_retries() {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("legacy.db3");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xmtp_legacy_migration/fixtures/stable.db3"
        ),
        &source,
    )?;
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
        (2, 3, 1)
    );
    for populated in [false, true] {
        tester!(destination, disable_workers);
        let original = if populated {
            let group = destination.create_group(None, None)?;
            let message = group
                .send_message(b"destination history", Default::default())
                .await?;
            Some((group.group_id.clone(), message))
        } else {
            None
        };
        let preview = ArchiveImporter::from_file(&output, &KEY).await?;
        let records: Vec<_> = preview.try_collect().await?;
        let mut partial = futures::stream::iter(
            records
                .into_iter()
                .take(2)
                .map(Ok::<_, std::io::Error>)
                .chain([Err(std::io::Error::other("interrupted archive read"))]),
        );
        assert!(
            insert_elements(&mut partial, &destination.context)
                .await
                .is_err()
        );
        for _ in 0..2 {
            let mut importer = ArchiveImporter::from_file(&output, &KEY).await?;
            insert_importer(&mut importer, &destination.context).await?;
            let restored: Vec<StoredGroup> = destination.db().raw_query(|conn| {
                groups::table
                    .filter(groups::membership_state.eq(GroupMembershipState::Restored as i32))
                    .load(conn)
            })?;
            assert_eq!(restored.len(), 2);
            let dm = restored
                .iter()
                .find(|g| hex::encode(&g.id) == DM)
                .expect("original DM id");
            assert!(
                dm.dm_id
                    .as_ref()
                    .expect("DM pair")
                    .contains(&"01".repeat(32))
            );
            assert_eq!(destination.group(&dm.id)?.group_name()?, "Migration DM");
            let messages: Vec<StoredGroupMessage> = destination.db().raw_query(|conn| {
                group_messages::table
                    .filter(group_messages::id.eq_any([vec![1; 32], vec![8; 32], vec![10; 32]]))
                    .order(group_messages::id)
                    .select(StoredGroupMessage::as_select())
                    .load(conn)
            })?;
            assert_eq!(messages.len(), 3);
            for message in messages {
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
                    "0a120a08786d74702e6f7267120474657874180122166d6967726174696f6e20666978747572652074657874"
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
