use diesel::QueryableByName;
use diesel::sql_types::{BigInt, Blob, Integer, Nullable, Text};
use std::time::Instant;

use super::*;

/// Helper function to insert a group message before the migration
/// (without the inserted_at_ns field)
fn insert_message_before_migration(db: impl ConnectionExt, group_id: &[u8], payload: Vec<u8>) {
    db.raw_query_write(|conn| {
        sql_query(
            r#"
            INSERT INTO group_messages (
                id, group_id, decrypted_message_bytes, sent_at_ns, kind,
                sender_installation_id, sender_inbox_id, delivery_status,
                content_type, version_major, version_minor, authority_id,
                reference_id, originator_id, sequence_id
            )
            VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
        )
        .bind::<Blob, _>(xmtp_common::rand_vec::<32>())
        .bind::<Blob, _>(group_id)
        .bind::<Blob, _>(payload)
        .bind::<BigInt, _>(xmtp_common::rand_i64())
        .bind::<Integer, _>(1) // kind: application message
        .bind::<Blob, _>(xmtp_common::rand_vec::<32>())
        .bind::<Text, _>("test_inbox")
        .bind::<Integer, _>(2) // delivery_status
        .bind::<Integer, _>(0) // content_type
        .bind::<Integer, _>(0) // version_major
        .bind::<Integer, _>(0) // version_minor
        .bind::<Text, _>("authority")
        .bind::<Nullable<Blob>, _>(None::<Vec<u8>>) // reference_id
        .bind::<BigInt, _>(0) // originator_id
        .bind::<BigInt, _>(xmtp_common::rand_i64()) // sequence_id
        .execute(conn)?;
        Ok(())
    })
    .unwrap();
}

/// Helper function to create a group before the migration
fn insert_group_before_migration(db: impl ConnectionExt, group_id: &[u8]) {
    db.raw_query_write(|conn| {
        sql_query(
            r#"
            INSERT INTO groups (
                id, created_at_ns, membership_state, installations_last_checked,
                added_by_inbox_id, rotated_at_ns, conversation_type
            )
            VALUES($1, $2, $3, $4, $5, $6, $7)
        "#,
        )
        .bind::<Blob, _>(group_id)
        .bind::<BigInt, _>(0)
        .bind::<Integer, _>(2)
        .bind::<BigInt, _>(0)
        .bind::<Text, _>("test_inbox")
        .bind::<BigInt, _>(0)
        .bind::<Integer, _>(1)
        .execute(conn)?;
        Ok(())
    })
    .unwrap();
}

#[xmtp_common::test]
async fn migration_performance_10k_messages() {
    let db = crate::TestDb::create_database(None).await;

    // Migrate to the migration right before add_inserted_at_ns_to_group_messages
    migrate_before(
        db.conn(),
        "2025-11-15-232503_add_inserted_at_ns_to_group_messages",
    );

    // Create a test group
    let group_id = vec![1, 2, 3, 4, 5];
    insert_group_before_migration(db.conn(), &group_id);

    // Insert 10,000 messages with 500-byte payload each
    tracing::info!("Inserting 10,000 test messages with 500-byte payloads...");
    let payload = vec![0u8; 500];
    for i in 0..10_000 {
        if i % 1000 == 0 {
            tracing::info!("Inserted {} messages", i);
        }
        insert_message_before_migration(db.conn(), &group_id, payload.clone());
    }
    tracing::info!("Finished inserting test messages");

    // Time the migration
    let start = Instant::now();
    tracing::info!("Starting migration...");

    // Run just the add_inserted_at_ns migration
    db.conn()
        .raw_query_write(|conn| {
            conn.run_next_migration(MIGRATIONS).unwrap();
            Ok(())
        })
        .unwrap();

    let duration = start.elapsed();
    tracing::info!("Migration completed in {:?}", duration);

    // Assert the migration took less than 1 second
    assert!(
        duration.as_secs() < 1,
        "Migration took {:?}, which exceeds the 1 second limit",
        duration
    );

    // Verify the migration worked correctly by checking a few messages have inserted_at_ns
    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = BigInt)]
        count: i64,
    }

    let result: CountResult = db
        .conn()
        .raw_query_read(|conn| {
            sql_query(
                "SELECT COUNT(*) as count FROM group_messages WHERE inserted_at_ns IS NOT NULL",
            )
            .get_result(conn)
        })
        .unwrap();

    assert_eq!(
        result.count, 10_000,
        "All 10,000 messages should have inserted_at_ns set"
    );
}
