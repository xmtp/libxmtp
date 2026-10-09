use super::*;
use crate::delivery::{DeliveryScope, QueryDelivery};
use crate::group::tests::generate_group;
use crate::group_message::tests::generate_message;
use crate::{Store, StoreOrIgnore, TestDb, XmtpTestDb};
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
use diesel::sql_types::{Integer, Text};

fn args(limit: i64, direction: SortDirection) -> MsgQueryArgs {
    MsgQueryArgs {
        limit: Some(limit),
        direction: Some(direction),
        sort_by: Some(SortBy::SentAtDelivery),
        ..Default::default()
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_equal_time_no_gaps() {
    for count in [80, 501] {
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let group = generate_group(None);
        group.store(&db)?;
        let mut expected = Vec::new();
        for index in 0..count {
            let mut message = generate_message(None, Some(&group.id), Some(10), None, None, None);
            message.id = (u64::MAX - index as u64).to_be_bytes().repeat(4);
            expected.push(message.id.clone());
            message.store(&db)?;
        }
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let mut query = args(50, direction.clone());
            let mut actual = Vec::new();
            loop {
                let page = db.history_page_rows(&group.id, &query)?;
                assert!(page.rows.len() <= 50);
                assert_eq!(page.base_bodies_loaded, page.rows.len());
                assert!(page.candidate_keys <= 51);
                actual.extend(page.rows.into_iter().map(|row| row.stored.id));
                if !page.has_more {
                    break;
                }
                if direction == SortDirection::Ascending {
                    query.history_after = page.last_position;
                } else {
                    query.history_before = page.last_position;
                }
                assert!(actual.len() <= count);
            }
            let mut ordered = expected.clone();
            if direction == SortDirection::Descending {
                ordered.reverse();
            }
            assert_eq!(actual, ordered);
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_deleted_anchor_and_reopen() {
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut ids = Vec::new();
    for index in 1..=8 {
        let message = generate_message(None, Some(&group.id), Some(index), None, None, None);
        ids.push(message.id.clone());
        message.store(&db)?;
    }
    let page = db.history_page_rows(&group.id, &args(3, SortDirection::Descending))?;
    let anchor = page.last_position.unwrap();
    db.delete_message_by_id(&ids[5])?;
    let newest_cursor = db.current_delivery_cursor()?;
    db.delete_message_by_id(&ids[7])?;
    let added = generate_message(None, Some(&group.id), Some(9), None, None, None);
    added.store(&db)?;
    assert!(db.current_delivery_cursor()?.delivery_sequence > newest_cursor.delivery_sequence);
    db.raw_query(|conn| diesel::sql_query("VACUUM").execute(conn))?;
    drop(db);
    drop(store);
    let reopened = TestDb::create_persistent_store(Some(path)).await;
    let db = reopened.db();
    let mut query = args(3, SortDirection::Descending);
    query.history_before = Some(anchor);
    let remaining = db.history_page_rows(&group.id, &query)?;
    assert_eq!(
        remaining
            .rows
            .iter()
            .map(|row| row.stored.id.clone())
            .collect::<Vec<_>>(),
        vec![ids[4].clone(), ids[3].clone(), ids[2].clone()]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_import_backfill_filters_and_legacy_order() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut pending = generate_message(
        None,
        Some(&group.id),
        Some(100),
        Some(ContentType::Text),
        None,
        Some("own".into()),
    );
    pending.delivery_status = DeliveryStatus::Unpublished;
    pending.store(&db)?;
    let published = generate_message(
        None,
        Some(&group.id),
        Some(100),
        Some(ContentType::Text),
        None,
        Some("peer".into()),
    );
    published.store(&db)?;
    let mut imported = generate_message(
        None,
        Some(&group.id),
        Some(1),
        Some(ContentType::Text),
        None,
        Some("peer".into()),
    );
    imported.sequence_id = 0;
    imported.store(&db)?;
    imported.store_or_ignore(&db)?;
    db.set_delivery_status_to_published(&pending.id, 100, Cursor(50), None)?;
    let query = args(50, SortDirection::Ascending);
    let page = db.history_page_rows(&group.id, &query)?;
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.stored.id.clone())
            .collect::<Vec<_>>(),
        vec![
            imported.id.clone(),
            published.id.clone(),
            pending.id.clone()
        ]
    );
    assert!(page.rows.iter().all(|row| row.cursor.is_some()));
    let legacy = db.get_group_messages(&group.id, &MsgQueryArgs::default())?;
    assert_eq!(
        legacy.into_iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![
            imported.id.clone(),
            pending.id.clone(),
            published.id.clone()
        ]
    );
    let filtered = MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        content_types: Some(vec![ContentType::Text]),
        exclude_sender_inbox_ids: Some(vec!["own".into()]),
        sent_after_ns: Some(2),
        ..query
    };
    assert_eq!(
        db.history_page_rows(&group.id, &filtered)?
            .rows
            .into_iter()
            .map(|row| row.stored.id)
            .collect::<Vec<_>>(),
        vec![published.id]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_cursor_validation_and_no_acknowledgement() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    for index in 1..=4 {
        generate_message(None, Some(&group.id), Some(index), None, None, None).store(&db)?;
    }
    let owner = db.acquire_delivery_owner(0, 100)?;
    let scope = DeliveryScope::Groups(vec![group.id]);
    let unhandled = db.default_delivery_messages(owner, &scope, 1, 20)?;
    let page = db.history_page_rows(&group.id, &args(2, SortDirection::Ascending))?;
    let position = page.last_position.unwrap();
    let still_unhandled = db.default_delivery_messages(owner, &scope, 1, 20)?;
    assert_eq!(
        unhandled
            .iter()
            .map(|row| row.message.id.clone())
            .collect::<Vec<_>>(),
        still_unhandled
            .iter()
            .map(|row| row.message.id.clone())
            .collect::<Vec<_>>()
    );
    for cursor in [
        crate::delivery::DeliveryCursor {
            database_id: [99; 16],
            ..position.cursor
        },
        crate::delivery::DeliveryCursor {
            delivery_sequence: 1000,
            ..position.cursor
        },
        crate::delivery::DeliveryCursor {
            delivery_sequence: 0,
            ..position.cursor
        },
    ] {
        let mut query = args(2, SortDirection::Ascending);
        query.history_before = Some(HistoryPosition { cursor, ..position });
        assert!(db.history_page_rows(&group.id, &query).is_err());
    }
    let mut empty = args(2, SortDirection::Ascending);
    empty.history_before = Some(position);
    empty.history_after = Some(position);
    let empty = db.history_page_rows(&group.id, &empty)?;
    assert!(empty.rows.is_empty());
    assert!(!empty.has_more);
    assert!(empty.last_position.is_none());
    db.release_delivery_owner(owner)?;
    db.rotate_stream_database_id()?;
    let mut old = args(2, SortDirection::Ascending);
    old.history_after = Some(position);
    assert!(db.history_page_rows(&group.id, &old).is_err());
}

struct Explain<T>(T);
impl<T> QueryId for Explain<T> {
    type QueryId = ();
    const HAS_STATIC_QUERY_ID: bool = false;
}
impl<T> Query for Explain<T> {
    type SqlType = (Integer, Integer, Integer, Text);
}
impl<T: QueryFragment<Sqlite>> QueryFragment<Sqlite> for Explain<T> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, Sqlite>) -> diesel::QueryResult<()> {
        pass.push_sql("EXPLAIN QUERY PLAN ");
        self.0.walk_ast(pass)
    }
}
impl<T> RunQueryDsl<diesel::SqliteConnection> for Explain<T> {}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_indexed_seek_and_stitched_dm_body_bound() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let mut groups = vec![
        generate_group(None),
        generate_group(None),
        generate_group(None),
    ];
    for group in &mut groups {
        group.conversation_type = ConversationType::Dm;
        group.dm_id = Some("fixture-pair".into());
        group.store(&db)?;
    }
    db.raw_query(|conn| diesel::sql_query(
        "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) \
        INSERT INTO group_messages (id,group_id,decrypted_message_bytes,sent_at_ns,sender_installation_id,\
        sender_inbox_id,delivery_status,authority_id,sequence_id,delivery_sequence) \
        SELECT CAST(printf('%032d',x) AS BLOB), CASE WHEN x<=50000 THEN ? WHEN x<=75000 THEN ? ELSE ? END,\
        x'01', 10, x'01','peer', ?, 'xmtp.org', 0, x FROM n")
        .bind::<diesel::sql_types::Binary,_>(groups[0].id.as_ref())
        .bind::<diesel::sql_types::Binary,_>(groups[1].id.as_ref())
        .bind::<diesel::sql_types::Binary,_>(groups[2].id.as_ref())
        .bind::<Integer,_>(DeliveryStatus::Published as i32).execute(conn))?;
    db.raw_query(|conn| {
        diesel::update(crate::schema::refresh_state::table)
            .filter(
                crate::schema::refresh_state::entity_kind
                    .eq(crate::refresh_state::EntityKind::DeliveryAllocator),
            )
            .set(crate::schema::refresh_state::sequence_id.eq(100000))
            .execute(conn)
    })?;
    let current = db.current_delivery_cursor()?;
    for bound in [99999, 1000] {
        let mut query = args(50, SortDirection::Descending);
        query.history_before = Some(HistoryPosition {
            sent_at_ns: 10,
            cursor: crate::delivery::DeliveryCursor {
                delivery_sequence: bound,
                ..current
            },
        });
        let page = db.history_page_rows(&groups[0].id, &query)?;
        assert_eq!(page.physical_groups, 3);
        assert!(page.candidate_keys <= 3 * 51);
        assert_eq!(page.base_bodies_loaded, 50);
        assert_eq!(
            page.rows
                .iter()
                .map(|row| row.cursor.unwrap().delivery_sequence)
                .collect::<Vec<_>>(),
            (bound - 50..bound).rev().collect::<Vec<_>>()
        );
        for group in &groups {
            let plan = db.raw_query(|conn| {
                Explain(key_query!(&group.id, &query, 51, 100))
                    .load::<(i32, i32, i32, String)>(conn)
            })?;
            let text = plan
                .iter()
                .map(|row| row.3.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("SEARCH"), "{text}");
            assert!(
                text.contains("group_messages_group_history_position"),
                "{text}"
            );
            assert!(!text.contains("TEMP B-TREE"), "{text}");
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_snapshot_keeps_scope_keys_and_bodies() {
    let path = xmtp_common::tmp_path();
    let reader = TestDb::create_persistent_store(Some(path.clone())).await;
    let writer = TestDb::create_persistent_store(Some(path)).await;
    let db = reader.db();
    let mut group = generate_group(None);
    group.conversation_type = ConversationType::Dm;
    group.dm_id = Some("snapshot-pair".into());
    group.store(&db)?;
    let first = generate_message(None, Some(&group.id), Some(10), None, None, None);
    first.store(&db)?;
    let second = generate_message(None, Some(&group.id), Some(20), None, None, None);
    second.store(&db)?;
    let original = db.current_delivery_cursor()?;
    let deleted_id = first.id.clone();
    BEFORE_BODIES.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            let db = writer.db();
            db.delete_message_by_id(&deleted_id).unwrap();
            let mut added = generate_group(None);
            added.conversation_type = ConversationType::Dm;
            added.dm_id = Some("snapshot-pair".into());
            added.store(&db).unwrap();
            generate_message(None, Some(&added.id), Some(15), None, None, None)
                .store(&db)
                .unwrap();
            db.rotate_stream_database_id().unwrap();
        }))
    });
    let page = db.history_page_rows(&group.id, &args(50, SortDirection::Ascending));
    BEFORE_BODIES.with_borrow_mut(|hook| *hook = None);
    let page = page?;
    assert_eq!(page.physical_groups, 1);
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.stored.id.clone())
            .collect::<Vec<_>>(),
        vec![first.id, second.id]
    );
    assert_eq!(
        page.first_position.unwrap().cursor.database_id,
        original.database_id
    );
    assert_ne!(
        db.current_delivery_cursor()?.database_id,
        original.database_id
    );
}
