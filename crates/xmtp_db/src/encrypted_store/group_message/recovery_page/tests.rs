use super::super::page::{BEFORE_BODIES, Explain};
use super::*;
use crate::delivery::QueryDelivery;
use crate::group::tests::generate_group;
use crate::group_message::tests::generate_message;
use crate::{Store, TestDb, XmtpTestDb};
use diesel::sql_types::Binary;

fn args(limit: i64, direction: SortDirection) -> RecoveryQueryArgs {
    RecoveryQueryArgs {
        messages: MsgQueryArgs {
            limit: Some(limit),
            direction: Some(direction),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn ids(page: &RecoveryPageRows) -> Vec<Vec<u8>> {
    page.rows.iter().map(|row| row.stored.id.clone()).collect()
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_ties_chronology_status_kind_and_scope() {
    for stitched in [false, true] {
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let mut groups = vec![generate_group(None), generate_group(None)];
        for group in &mut groups {
            if stitched {
                group.conversation_type = ConversationType::Dm;
                group.dm_id = Some("recovery-pair".into());
            }
            group.store(&db)?;
        }
        let mut expected = Vec::new();
        for index in 0..124_u64 {
            let source = &groups[index as usize % 2];
            let time = match index {
                0 => 9,
                123 => 11,
                _ => 10,
            };
            let kind = if index % 3 == 0 {
                GroupMessageKind::MembershipChange
            } else {
                GroupMessageKind::Application
            };
            let mut message =
                generate_message(Some(kind), Some(&source.id), Some(time), None, None, None);
            message.id = (1000 - index).to_be_bytes().repeat(4);
            message.delivery_status = if index % 2 == 0 {
                DeliveryStatus::Unpublished
            } else {
                DeliveryStatus::Failed
            };
            let status = message.delivery_status;
            let id = message.id.clone();
            message.store(&db)?;
            if stitched || source.id == groups[0].id {
                expected.push((time, id, status, kind));
            }
        }
        // Published and expired rows must never enter the pending page or sentinel.
        generate_message(None, Some(&groups[0].id), Some(10), None, None, None).store(&db)?;
        let mut expired =
            generate_message(None, Some(&groups[0].id), Some(10), None, Some(1), None);
        expired.delivery_status = DeliveryStatus::Failed;
        expired.store(&db)?;
        expected.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            for status in [
                None,
                Some(DeliveryStatus::Unpublished),
                Some(DeliveryStatus::Failed),
            ] {
                for kind in [
                    None,
                    Some(GroupMessageKind::Application),
                    Some(GroupMessageKind::MembershipChange),
                ] {
                    let mut query = args(50, direction.clone());
                    query.messages.delivery_status = status;
                    query.messages.kind = kind;
                    let mut actual = Vec::new();
                    loop {
                        let page = db.recovery_page_rows(&groups[0].id, &query)?;
                        assert!(page.candidate_keys <= page.physical_groups * 51);
                        assert_eq!(page.base_bodies_loaded, page.rows.len());
                        assert!(page.rows.iter().all(|row| row.cursor.is_none()));
                        actual.extend(ids(&page));
                        if !page.has_more {
                            break;
                        }
                        if direction == SortDirection::Ascending {
                            query.after = page.last_position;
                        } else {
                            query.before = page.last_position;
                        }
                        assert!(actual.len() <= expected.len());
                    }
                    let mut wanted: Vec<_> = expected
                        .iter()
                        .filter(|row| {
                            status.is_none_or(|value| value == row.2)
                                && kind.is_none_or(|value| value == row.3)
                        })
                        .map(|row| row.1.clone())
                        .collect();
                    if direction == SortDirection::Descending {
                        wanted.reverse();
                    }
                    assert_eq!(
                        actual, wanted,
                        "{stitched:?} {direction:?} {status:?} {kind:?}"
                    );
                }
            }
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_removed_boundary_reopen_foreign_and_raw_key() {
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut expected = Vec::new();
    for index in 0..6_u8 {
        let mut message = generate_message(None, Some(&group.id), Some(10), None, None, None);
        message.id = vec![index]; // Deliberately invalid SDK MessageIds remain valid raw boundaries.
        message.delivery_status = DeliveryStatus::Unpublished;
        expected.push(message.id.clone());
        message.store(&db)?;
    }
    let first = db.recovery_page_rows(&group.id, &args(2, SortDirection::Ascending))?;
    let deleted = first.last_position.unwrap();
    db.delete_message_by_id(&deleted.message_id)?;
    let mut query = args(2, SortDirection::Ascending);
    query.after = Some(deleted);
    let second = db.recovery_page_rows(&group.id, &query)?;
    let published = second.last_position.unwrap();
    db.set_delivery_status_to_published(&published.message_id, 10, Cursor(7), None)?;
    query.after = Some(published.clone());
    db.raw_query(|conn| diesel::sql_query("VACUUM").execute(conn))?;
    drop(db);
    drop(store);
    let reopened = TestDb::create_persistent_store(Some(path)).await;
    let db = reopened.db();
    assert_eq!(
        ids(&db.recovery_page_rows(&group.id, &query)?),
        expected[4..]
    );
    let mut reverse = args(50, SortDirection::Descending);
    reverse.before = Some(published.clone());
    assert_eq!(
        ids(&db.recovery_page_rows(&group.id, &reverse)?),
        vec![expected[2].clone(), expected[0].clone()]
    );
    reverse.after = Some(published.clone());
    let empty = db.recovery_page_rows(&group.id, &reverse)?;
    assert!(empty.rows.is_empty() && !empty.has_more && empty.first_position.is_none());
    query.after.as_mut().unwrap().database_id = [99; 16];
    assert!(matches!(
        db.recovery_page_rows(&group.id, &query),
        Err(StorageError::Stream(StreamStorageError::ForeignCursor))
    ));
    query.after = Some(published);
    db.rotate_stream_database_id()?;
    assert!(matches!(
        db.recovery_page_rows(&group.id, &query),
        Err(StorageError::Stream(StreamStorageError::ForeignCursor))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_indexed_seek_and_stitched_body_bound() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let mut groups = vec![
        generate_group(None),
        generate_group(None),
        generate_group(None),
    ];
    for group in &mut groups {
        group.conversation_type = ConversationType::Dm;
        group.dm_id = Some("cost-pair".into());
        group.store(&db)?;
    }
    db.raw_query(|conn| diesel::sql_query(
        "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) \
        INSERT INTO group_messages (id,group_id,decrypted_message_bytes,sent_at_ns,sender_installation_id,\
        sender_inbox_id,delivery_status,authority_id,sequence_id) \
        SELECT CAST(printf('%032d',x) AS BLOB), CASE WHEN x<=50000 THEN ? WHEN x<=75000 THEN ? ELSE ? END,\
        x'01', 10, x'01','peer', CASE WHEN x%2=0 THEN 1 ELSE 3 END, 'xmtp.org', 0 FROM n")
        .bind::<Binary,_>(groups[0].id.as_ref()).bind::<Binary,_>(groups[1].id.as_ref())
        .bind::<Binary,_>(groups[2].id.as_ref()).execute(conn))?;
    let identity = db.current_delivery_cursor()?.database_id;
    for bound in [99999, 1000] {
        let mut query = args(50, SortDirection::Descending);
        query.before = Some(RecoveryPosition {
            sent_at_ns: 10,
            database_id: identity,
            message_id: format!("{bound:032}").into_bytes(),
        });
        let page = db.recovery_page_rows(&groups[0].id, &query)?;
        assert_eq!(page.physical_groups, 3);
        assert!(page.candidate_keys <= 3 * 51);
        assert_eq!(page.base_bodies_loaded, 50);
        assert_eq!(
            ids(&page),
            (bound - 50..bound)
                .rev()
                .map(|id| format!("{id:032}").into_bytes())
                .collect::<Vec<_>>()
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
            assert!(
                text.contains("SEARCH") && text.contains("group_messages_pending_history_position"),
                "{text}"
            );
            assert!(!text.contains("TEMP B-TREE"), "{text}");
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_snapshot_keeps_scope_keys_status_and_bodies() {
    let path = xmtp_common::tmp_path();
    let reader = TestDb::create_persistent_store(Some(path.clone())).await;
    let writer = TestDb::create_persistent_store(Some(path)).await;
    let db = reader.db();
    let mut group = generate_group(None);
    group.conversation_type = ConversationType::Dm;
    group.dm_id = Some("snapshot-pending-pair".into());
    group.store(&db)?;
    let mut first = generate_message(None, Some(&group.id), Some(10), None, None, None);
    first.delivery_status = DeliveryStatus::Failed;
    first.store(&db)?;
    let mut second = generate_message(None, Some(&group.id), Some(20), None, None, None);
    second.delivery_status = DeliveryStatus::Unpublished;
    second.store(&db)?;
    let identity = db.current_delivery_cursor()?.database_id;
    let deleted = first.id.clone();
    let published = second.id.clone();
    BEFORE_BODIES.with_borrow_mut(|hook| {
        *hook = Some(Box::new(move || {
            let db = writer.db();
            db.delete_message_by_id(&deleted).unwrap();
            db.set_delivery_status_to_published(&published, 20, Cursor(7), None)
                .unwrap();
            let mut added = generate_group(None);
            added.conversation_type = ConversationType::Dm;
            added.dm_id = Some("snapshot-pending-pair".into());
            added.store(&db).unwrap();
            let mut message = generate_message(None, Some(&added.id), Some(15), None, None, None);
            message.delivery_status = DeliveryStatus::Failed;
            message.store(&db).unwrap();
            db.rotate_stream_database_id().unwrap();
        }))
    });
    let result = db.recovery_page_rows(&group.id, &args(50, SortDirection::Ascending));
    BEFORE_BODIES.with_borrow_mut(|hook| *hook = None);
    assert!(
        result.is_ok(),
        "one recovery read snapshot must retain selected rows"
    );
    let page = result?;
    assert_eq!(page.physical_groups, 1);
    assert_eq!(ids(&page), vec![first.id, second.id]);
    assert_eq!(page.rows[0].stored.delivery_status, DeliveryStatus::Failed);
    assert_eq!(
        page.rows[1].stored.delivery_status,
        DeliveryStatus::Unpublished
    );
    assert_eq!(page.first_position.unwrap().database_id, identity);
    assert_ne!(db.current_delivery_cursor()?.database_id, identity);
}
