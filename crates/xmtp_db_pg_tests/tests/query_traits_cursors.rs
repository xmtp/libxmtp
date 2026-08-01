//! `QueryRefreshState` and `QueryIdentityUpdates` over Postgres.
//!
//! These two carry the sync loop's cursor arithmetic, so the tests lean on the
//! ordering and monotonicity rules rather than just round-tripping rows.

use std::collections::HashMap;
use xmtp_db::identity_update::{QueryIdentityUpdates, StoredIdentityUpdate};
use xmtp_db::pg::PgDb;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::Cursor;

// --- refresh_state ----------------------------------------------------------

const ORIG_A: u32 = 10;
const ORIG_B: u32 = 11;

#[tokio::test]
async fn cursor_lookup_seeds_missing_originators() {
    let db = fresh_db("rs_seed").await;
    let id = b"entity-1".to_vec();
    let kind = EntityKind::ApplicationMessage;

    assert!(
        db.get_refresh_state(&id, kind, ORIG_A)
            .await
            .unwrap()
            .is_none(),
        "nothing stored yet"
    );

    let cursors = db
        .get_last_cursor_for_originators(&id, kind, &[ORIG_A, ORIG_B])
        .await
        .unwrap();
    assert_eq!(
        cursors,
        vec![Cursor::new(0, ORIG_A), Cursor::new(0, ORIG_B)]
    );

    // Reading seeds a zero row as a side effect, which the sync loop relies on.
    assert!(
        db.get_refresh_state(&id, kind, ORIG_A)
            .await
            .unwrap()
            .is_some(),
        "reading a missing cursor must seed it"
    );
}

/// Results come back positionally: callers index by the order they asked in.
#[tokio::test]
async fn cursor_lookup_preserves_requested_order() {
    let db = fresh_db("rs_order").await;
    let id = b"entity-2".to_vec();
    let kind = EntityKind::ApplicationMessage;

    db.update_cursor(&id, kind, Cursor::new(5, ORIG_A))
        .await
        .unwrap();
    db.update_cursor(&id, kind, Cursor::new(9, ORIG_B))
        .await
        .unwrap();

    let cursors = db
        .get_last_cursor_for_originators(&id, kind, &[ORIG_B, ORIG_A])
        .await
        .unwrap();
    assert_eq!(
        cursors,
        vec![Cursor::new(9, ORIG_B), Cursor::new(5, ORIG_A)],
        "order follows the request, not the table"
    );
}

/// The cursor only ever moves forward. A stale writer is rejected by the
/// database's `DO UPDATE ... WHERE`, not by a read-then-compare that could race.
#[tokio::test]
async fn update_cursor_never_rewinds() {
    let db = fresh_db("rs_monotonic").await;
    let id = b"entity-3".to_vec();
    let kind = EntityKind::Welcome;

    assert!(
        db.update_cursor(&id, kind, Cursor::new(10, ORIG_A))
            .await
            .unwrap(),
        "first write inserts"
    );
    assert!(
        db.update_cursor(&id, kind, Cursor::new(20, ORIG_A))
            .await
            .unwrap(),
        "advancing returns true"
    );
    assert!(
        !db.update_cursor(&id, kind, Cursor::new(15, ORIG_A))
            .await
            .unwrap(),
        "a lower cursor is refused and reports false"
    );
    assert!(
        !db.update_cursor(&id, kind, Cursor::new(20, ORIG_A))
            .await
            .unwrap(),
        "an equal cursor is not an advance"
    );

    let state = db
        .get_refresh_state(&id, kind, ORIG_A)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.sequence_id, 20);
}

/// Cursors are keyed by (entity_id, entity_kind, originator) — not by any one of
/// them alone.
#[tokio::test]
async fn cursors_are_scoped_by_entity_kind_and_originator() {
    let db = fresh_db("rs_scope").await;
    let id = b"entity-4".to_vec();

    db.update_cursor(&id, EntityKind::Welcome, Cursor::new(7, ORIG_A))
        .await
        .unwrap();

    assert!(
        db.get_refresh_state(&id, EntityKind::ApplicationMessage, ORIG_A)
            .await
            .unwrap()
            .is_none(),
        "a different entity_kind is a different cursor"
    );
    assert!(
        db.get_refresh_state(&id, EntityKind::Welcome, ORIG_B)
            .await
            .unwrap()
            .is_none(),
        "a different originator is a different cursor"
    );
}

#[tokio::test]
async fn latest_cursor_and_batch_lookup() {
    let db = fresh_db("rs_batch").await;
    let a = b"ent-a".to_vec();
    let b = b"ent-b".to_vec();
    let kind = EntityKind::ApplicationMessage;

    db.update_cursor(&a, kind, Cursor::new(3, ORIG_A))
        .await
        .unwrap();
    db.update_cursor(&a, kind, Cursor::new(4, ORIG_B))
        .await
        .unwrap();
    db.update_cursor(&b, kind, Cursor::new(5, ORIG_A))
        .await
        .unwrap();

    let latest = db.latest_cursor_for_id(&a, &[kind], None).await.unwrap();
    assert_eq!(latest.get(&ORIG_A), 3);
    assert_eq!(latest.get(&ORIG_B), 4);

    // Restricting the originator list narrows the result. `get` reports an
    // absent originator as 0, so check the map length too — otherwise "filtered
    // out" and "present with sequence 0" would look identical.
    let only_a = db
        .latest_cursor_for_id(&a, &[kind], Some(&[&ORIG_A]))
        .await
        .unwrap();
    assert_eq!(only_a.get(&ORIG_A), 3);
    assert_eq!(only_a.len(), 1, "ORIG_B must be filtered out entirely");

    let map = db
        .get_last_cursor_for_ids(&[&a, &b], &[kind])
        .await
        .unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&a).unwrap().get(&ORIG_B), 4);
    assert_eq!(map.get(&b).unwrap().get(&ORIG_A), 5);

    let empty: HashMap<_, _> = db
        .get_last_cursor_for_ids::<Vec<u8>>(&[], &[kind])
        .await
        .unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn remote_log_cursors_default_to_zero_and_seed() {
    let db = fresh_db("rs_remote").await;
    let known: &[u8] = b"conv-known";
    let unknown: &[u8] = b"conv-unknown";

    db.update_cursor(known, EntityKind::CommitLogDownload, Cursor::commit_log(12))
        .await
        .unwrap();

    let map = db.get_remote_log_cursors(&[known, unknown]).await.unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(known).unwrap().sequence_id, 12);
    assert_eq!(
        map.get(unknown).unwrap().sequence_id,
        0,
        "a conversation with no cursor reads as zero, not missing"
    );

    // The unknown conversation is seeded in the same pass.
    let seeded = db
        .get_refresh_state(
            unknown,
            EntityKind::CommitLogDownload,
            map.get(unknown).unwrap().originator_id,
        )
        .await
        .unwrap();
    assert!(seeded.is_some(), "missing cursors are seeded on read");

    assert!(db.get_remote_log_cursors(&[]).await.unwrap().is_empty());
}

// --- identity_update --------------------------------------------------------

fn update(inbox: &str, seq: i64) -> StoredIdentityUpdate {
    StoredIdentityUpdate::new(inbox.to_string(), seq, seq * 1000, vec![seq as u8], 1)
}

async fn seed_updates(db: &PgDb, updates: &[StoredIdentityUpdate]) {
    db.insert_or_ignore_identity_updates(updates).await.unwrap();
}

#[tokio::test]
async fn identity_update_ranges_are_half_open() {
    let db = fresh_db("iu_range").await;
    seed_updates(
        &db,
        &[
            update("a", 1),
            update("a", 2),
            update("a", 3),
            update("b", 9),
        ],
    )
    .await;

    let all = db.get_identity_updates("a", None, None).await.unwrap();
    assert_eq!(
        all.iter().map(|u| u.sequence_id).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "ascending, and scoped to the inbox"
    );

    let from = db.get_identity_updates("a", Some(1), None).await.unwrap();
    assert_eq!(
        from.iter().map(|u| u.sequence_id).collect::<Vec<_>>(),
        vec![2, 3],
        "from is exclusive"
    );

    let to = db.get_identity_updates("a", None, Some(2)).await.unwrap();
    assert_eq!(
        to.iter().map(|u| u.sequence_id).collect::<Vec<_>>(),
        vec![1, 2],
        "to is inclusive"
    );

    let both = db
        .get_identity_updates("a", Some(1), Some(2))
        .await
        .unwrap();
    assert_eq!(
        both.iter().map(|u| u.sequence_id).collect::<Vec<_>>(),
        vec![2]
    );
}

#[tokio::test]
async fn identity_update_batch_insert_ignores_duplicates() {
    let db = fresh_db("iu_dup").await;
    seed_updates(&db, &[update("a", 1), update("a", 2)]).await;
    // Overlapping batch: the existing rows must be ignored, not error.
    seed_updates(&db, &[update("a", 2), update("a", 3)]).await;

    assert_eq!(
        db.get_identity_updates("a", None, None)
            .await
            .unwrap()
            .len(),
        3
    );
    seed_updates(&db, &[]).await;
}

#[tokio::test]
async fn identity_update_aggregates() {
    let db = fresh_db("iu_agg").await;
    seed_updates(
        &db,
        &[
            update("a", 1),
            update("a", 5),
            update("b", 2),
            update("b", 3),
        ],
    )
    .await;

    assert_eq!(db.get_latest_sequence_id_for_inbox("a").await.unwrap(), 5);
    assert!(
        db.get_latest_sequence_id_for_inbox("nobody").await.is_err(),
        "an inbox with no updates is an error, not sequence 0"
    );

    let latest = db.get_latest_sequence_id(&["a", "b"]).await.unwrap();
    assert_eq!(latest.get("a"), Some(&5));
    assert_eq!(latest.get("b"), Some(&3));
    assert_eq!(latest.get("nobody"), None);

    let counts = db.count_inbox_updates(&["a", "b"]).await.unwrap();
    assert_eq!(counts.get("a"), Some(&2));
    assert_eq!(counts.get("b"), Some(&2));

    assert!(db.get_latest_sequence_id(&[]).await.unwrap().is_empty());
    assert!(db.count_inbox_updates(&[]).await.unwrap().is_empty());
}
