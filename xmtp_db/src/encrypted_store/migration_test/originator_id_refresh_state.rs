use diesel::QueryableByName;
use diesel::sql_types::{BigInt, Blob, Bool, Integer, Text};

use crate::{prelude::QueryRefreshState, refresh_state::EntityKind};
use xmtp_proto::types::Cursor;

use super::*;

fn update_cursor(db: impl ConnectionExt, id: &[u8], kind: i32, cursor: i64) {
    db.raw_query_write(|conn| {
        sql_query(
            r#"
                INSERT INTO refresh_state (entity_id, entity_kind, cursor)
                VALUES ($1, $2, $3)
            "#,
        )
        .bind::<Blob, _>(id)
        .bind::<Integer, _>(kind)
        .bind::<BigInt, _>(cursor)
        .execute(conn)?;
        Ok(())
    })
    .unwrap()
}

fn update_cursor_new(
    db: impl ConnectionExt,
    id: &[u8],
    kind: i32,
    sequence_id: i64,
    originator_id: i32,
) {
    db.raw_query_write(|conn| {
        sql_query(
            r#"
                INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id)
                VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind::<Blob, _>(id)
        .bind::<Integer, _>(kind)
        .bind::<BigInt, _>(sequence_id)
        .bind::<Integer, _>(originator_id)
        .execute(conn)?;
        Ok(())
    })
    .unwrap()
}

fn message(db: impl ConnectionExt, group_id: &[u8], kind: i32, sequence_id: i64) {
    db.raw_query_write(|conn| {
        sql_query(r#"
            INSERT INTO group_messages (id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, sender_inbox_id, delivery_status, content_type, version_major, version_minor, authority_id, sequence_id)
            VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#)
            .bind::<Blob, _>(xmtp_common::rand_vec::<32>())
            .bind::<Blob, _>(group_id)
            .bind::<Blob, _>(vec![0])
            .bind::<BigInt, _>(0)
            .bind::<Integer, _>(kind)
            .bind::<Blob, _>(vec![0])
            .bind::<Text, _>("test_inbox")
            .bind::<Integer, _>(2)
            .bind::<Integer, _>(0)
            .bind::<Integer, _>(0)
            .bind::<Integer, _>(0)
            .bind::<Text, _>("authority")
            .bind::<BigInt, _>(sequence_id)
            .execute(conn)?;
            Ok(())

    }).unwrap();
}

fn group(db: impl ConnectionExt, group_id: &[u8]) {
    db.raw_query_write(|conn| {
        sql_query(r#"
            INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, added_by_inbox_id, rotated_at_ns, conversation_type, maybe_forked, fork_details, should_publish_commit_log)
            VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#)
            .bind::<Blob, _>(group_id)
            .bind::<BigInt, _>(0)
            .bind::<Integer, _>(2)
            .bind::<Integer, _>(2)
            .bind::<Text, _>("test")
            .bind::<BigInt, _>(0)
            .bind::<Integer, _>(0)
            .bind::<Bool, _>(false)
            .bind::<Text, _>("details")
            .bind::<Bool, _>(false)
            .execute(conn)?;
            Ok(())

    }).unwrap();
}

#[xmtp_common::test]
async fn up_cursor_matches_commit() {
    let db = crate::TestDb::create_database(None).await;
    migrate_before(db.conn(), "2025-08-20-175213_d14n_originator_refresh_state");

    group(db.conn(), &[0, 0, 0]);
    update_cursor(db.conn(), &[0, 0, 0], 2, 100); // cursor=100
    message(db.conn(), &[0, 0, 0], 2, 100); // commit at seq_id=100 (matches cursor)
    message(db.conn(), &[0, 0, 0], 1, 150); // app message at seq_id=150 (streamed msg, not synced!)

    finish_migrations(db.conn());

    // Both cursors should be set to 100
    let commit_cursor = db
        .db()
        .get_last_cursor_for_originator(&[0, 0, 0], EntityKind::CommitMessage, 0)
        .unwrap();
    assert_eq!(
        commit_cursor,
        Cursor::mls_commits(100),
        "CommitMessage cursor should be 100 (matched commit)"
    );

    let app_cursor = db
        .db()
        .get_last_cursor_for_originator(&[0, 0, 0], EntityKind::ApplicationMessage, 10)
        .unwrap();
    assert_eq!(
        app_cursor,
        Cursor::v3_messages(100),
        "ApplicationMessage cursor should be 100 (can't trust local app@150)"
    );
}

#[xmtp_common::test]
async fn up_cursor_matches_app() {
    let db = crate::TestDb::create_database(None).await;
    migrate_before(db.conn(), "2025-08-20-175213_d14n_originator_refresh_state");

    group(db.conn(), &[1, 1, 1]);
    update_cursor(db.conn(), &[1, 1, 1], 2, 150); // cursor=150
    message(db.conn(), &[1, 1, 1], 2, 99); // commit at seq_id=99
    message(db.conn(), &[1, 1, 1], 1, 150); // app message at seq_id=150 (matches cursor)

    finish_migrations(db.conn());

    // App cursor uses the old cursor value (150)
    let app_cursor = db
        .db()
        .get_last_cursor_for_originator(&[1, 1, 1], EntityKind::ApplicationMessage, 10)
        .unwrap();
    assert_eq!(
        app_cursor,
        Cursor::v3_messages(150),
        "ApplicationMessage cursor should be 150 (matched app)"
    );

    // Commit cursor uses MAX(commits) = 99
    let commit_cursor = db
        .db()
        .get_last_cursor_for_originator(&[1, 1, 1], EntityKind::CommitMessage, 0)
        .unwrap();
    assert_eq!(
        commit_cursor,
        Cursor::mls_commits(99),
        "CommitMessage cursor should be 99 (MAX of commits)"
    );
}

#[xmtp_common::test]
async fn up_cursor_matches_nothing() {
    let db = crate::TestDb::create_database(None).await;
    migrate_before(db.conn(), "2025-08-20-175213_d14n_originator_refresh_state");

    group(db.conn(), &[2, 2, 2]);
    update_cursor(db.conn(), &[2, 2, 2], 2, 50); // cursor=50 (no message at this seq_id)
    message(db.conn(), &[2, 2, 2], 2, 75); // commit at seq_id=75
    message(db.conn(), &[2, 2, 2], 1, 100); // app message at seq_id=100

    finish_migrations(db.conn());

    // App cursor uses the old cursor value (50)
    let app_cursor = db
        .db()
        .get_last_cursor_for_originator(&[2, 2, 2], EntityKind::ApplicationMessage, 10)
        .unwrap();
    assert_eq!(
        app_cursor,
        Cursor::v3_messages(50),
        "ApplicationMessage cursor should be 50 (from old cursor)"
    );

    // Commit cursor uses MAX(commits) = 75
    let commit_cursor = db
        .db()
        .get_last_cursor_for_originator(&[2, 2, 2], EntityKind::CommitMessage, 0)
        .unwrap();
    assert_eq!(
        commit_cursor,
        Cursor::mls_commits(75),
        "CommitMessage cursor should be 75 (MAX of commits)"
    );
}

#[xmtp_common::test]
async fn up_welcome_unchanged() {
    // Verify that Welcome entries remain unchanged during migration

    let db = crate::TestDb::create_database(None).await;
    migrate_before(db.conn(), "2025-08-20-175213_d14n_originator_refresh_state");

    group(db.conn(), &[0, 0, 0]);
    update_cursor(db.conn(), &[0, 0, 0], 1, 100); // Welcome cursor

    finish_migrations(db.conn());

    let welcome_cursor = db
        .db()
        .get_last_cursor_for_originator(&[0, 0, 0], EntityKind::Welcome, 11)
        .unwrap();
    assert_eq!(
        welcome_cursor,
        Cursor::welcomes(100),
        "Welcome entry should remain unchanged"
    );
}

#[xmtp_common::test]
async fn down() {
    let db = crate::TestDb::create_database(None).await;

    // Migrate to (and including) the target migration to get to the NEW schema
    migrate_to(db.conn(), "2025-08-20-175213_d14n_originator_refresh_state");

    // Insert refresh state entries in NEW schema format
    // Group [0,0,0]: CommitMessage and ApplicationMessage entries
    update_cursor_new(db.conn(), &[0, 0, 0], 7, 100, 0); // CommitMessage, seq_id=100, originator=0
    update_cursor_new(db.conn(), &[0, 0, 0], 2, 50, 10); // ApplicationMessage, seq_id=50, originator=10

    // Group [1,1,1]: CommitMessage and ApplicationMessage entries
    update_cursor_new(db.conn(), &[1, 1, 1], 7, 99, 0); // CommitMessage, seq_id=99, originator=0
    update_cursor_new(db.conn(), &[1, 1, 1], 2, 150, 10); // ApplicationMessage, seq_id=150, originator=10

    // Add a Welcome entry
    update_cursor_new(db.conn(), &[0, 0, 0], 1, 75, 11); // Welcome, seq_id=75, originator=11

    // Now revert the migration (down)
    db.conn()
        .raw_query_write(|conn| {
            conn.revert_last_migration(MIGRATIONS).unwrap();
            Ok(())
        })
        .unwrap();

    // After reverting, verify the old schema structure:
    // - entity_kind=7 (CommitMessage) should be converted back to entity_kind=2 (Group)
    // - Multiple entries for same entity_id/entity_kind should be deduplicated to MAX(cursor)
    // - Primary key should be (entity_id, entity_kind) without originator_id

    // Query using the old schema (cursor column instead of sequence_id, no originator_id)
    #[derive(QueryableByName, Debug)]
    struct OldRefreshState {
        #[diesel(sql_type = Blob)]
        entity_id: Vec<u8>,
        #[diesel(sql_type = Integer)]
        entity_kind: i32,
        #[diesel(sql_type = BigInt)]
        cursor: i64,
    }

    let results: Vec<OldRefreshState> = db.conn().raw_query_read(|conn| {
        sql_query("SELECT entity_id, entity_kind, cursor FROM refresh_state ORDER BY entity_id, entity_kind")
            .load(conn)
    }).unwrap();

    // Group [0,0,0] should have:
    // - entity_kind=1 (Welcome) with cursor=75
    // - entity_kind=2 (Group, merged from CommitMessage and ApplicationMessage) with cursor=MAX(100, 50)=100

    // Group [1,1,1] should have:
    // - entity_kind=2 (Group, merged from CommitMessage and ApplicationMessage) with cursor=MAX(99, 150)=150

    assert_eq!(
        results.len(),
        3,
        "Should have 3 deduplicated entries after down migration (2 groups + 1 welcome)"
    );

    // Verify group [0,0,0] entries
    let g0_welcome = results
        .iter()
        .find(|r| r.entity_id == vec![0, 0, 0] && r.entity_kind == 1);
    assert!(
        g0_welcome.is_some(),
        "Group [0,0,0] should have Welcome entry"
    );
    assert_eq!(
        g0_welcome.unwrap().cursor,
        75,
        "Welcome cursor should be 75"
    );

    let g0_group = results
        .iter()
        .find(|r| r.entity_id == vec![0, 0, 0] && r.entity_kind == 2);
    assert!(
        g0_group.is_some(),
        "Group [0,0,0] should have Group entry after down migration"
    );
    assert_eq!(
        g0_group.unwrap().cursor,
        100,
        "Group cursor should be MAX(100, 50) = 100"
    );

    // Verify group [1,1,1] entry
    let g1_group = results
        .iter()
        .find(|r| r.entity_id == vec![1, 1, 1] && r.entity_kind == 2);
    assert!(
        g1_group.is_some(),
        "Group [1,1,1] should have Group entry after down migration"
    );
    assert_eq!(
        g1_group.unwrap().cursor,
        150,
        "Group cursor should be MAX(99, 150) = 150"
    );
}
