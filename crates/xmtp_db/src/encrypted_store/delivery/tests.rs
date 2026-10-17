use super::*;
use crate::group::tests::generate_group;
use crate::group_message::tests::generate_message;
use crate::schema::group_messages;
use crate::{Store, StoreOrIgnore, TestDb, XmtpTestDb};
use xmtp_proto::types::Cursor;

// verifies: PROC-024
#[xmtp_common::test(unwrap_try = true)]
async fn local_order_survives_duplicates_deletion_and_lower_network_ids() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group_a = generate_group(None);
    let group_b = generate_group(None);
    group_a.store(&db)?;
    group_b.store(&db)?;
    let mut first = generate_message(None, Some(&group_a.id), Some(500), None, None, None);
    first.sequence_id = 500;
    first.store(&db)?;
    let first_cursor = db.current_delivery_cursor()?;
    first.store_or_ignore(&db)?;
    assert_eq!(db.current_delivery_cursor()?, first_cursor);
    db.raw_query(|conn| diesel::delete(group_messages::table.find(&first.id)).execute(conn))?;
    let mut second = generate_message(None, Some(&group_b.id), Some(400), None, None, None);
    second.sequence_id = 400;
    second.store(&db)?;
    let rows = db.replay_delivery_messages(first_cursor, &DeliveryScope::All, 0, 8)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, second.id);
    assert!(rows[0].cursor.delivery_sequence > first_cursor.delivery_sequence);
}

// verifies: PROC-024, PROC-025
#[xmtp_common::test(unwrap_try = true)]
async fn optimistic_message_becomes_deliverable_only_after_publication() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut message = generate_message(None, Some(&group.id), Some(10), None, None, None);
    message.delivery_status = DeliveryStatus::Unpublished;
    message.store(&db)?;
    let start = db.current_delivery_cursor()?;
    assert_eq!(start.delivery_sequence, 0);
    assert!(
        db.replay_delivery_messages(start, &DeliveryScope::All, 0, 8)?
            .is_empty()
    );
    db.set_delivery_status_to_published(&message.id, 20, Cursor(50), None)?;
    let rows = db.replay_delivery_messages(start, &DeliveryScope::All, 0, 8)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, message.id);
}

// verifies: PROC-026, PROC-034
#[xmtp_common::test(unwrap_try = true)]
async fn scope_progress_and_replay_remain_independent() {
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let db = store.db();
    let group_a = generate_group(None);
    let group_b = generate_group(None);
    group_a.store(&db)?;
    group_b.store(&db)?;
    let start = db.current_delivery_cursor()?;
    for group in [group_a.id, group_b.id, group_a.id, group_b.id] {
        generate_message(None, Some(&group), Some(1), None, None, None).store(&db)?;
    }
    let owner = db.acquire_delivery_owner(0, 100)?;
    let rows = db.default_delivery_messages(owner, &DeliveryScope::All, 1, 2)?;
    for row in rows {
        db.acknowledge_delivery(owner, row.message.group_id, row.cursor, 1)?;
    }
    let rows =
        db.default_delivery_messages(owner, &DeliveryScope::Groups(vec![group_a.id]), 1, 8)?;
    assert_eq!(rows.len(), 1);
    db.acknowledge_delivery(owner, group_a.id, rows[0].cursor, 1)?;
    db.release_delivery_owner(owner)?;
    drop(db);
    drop(store);
    let reopened = TestDb::create_persistent_store(Some(path)).await;
    let db = reopened.db();
    let owner = db.acquire_delivery_owner(2, 100)?;
    let rows = db.default_delivery_messages(owner, &DeliveryScope::All, 3, 8)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.group_id, group_b.id);
    assert_eq!(
        db.replay_delivery_messages(start, &DeliveryScope::All, 3, 8)?
            .len(),
        4
    );
    assert_eq!(
        db.default_delivery_messages(owner, &DeliveryScope::All, 3, 8)?
            .len(),
        1
    );
}

// verifies: DMS-009, DMS-016, PROC-026, PROC-034
#[xmtp_common::test(unwrap_try = true)]
async fn scoped_delivery_reads_the_stitched_dm_union_before_limits() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let mut first = generate_group(None);
    first.conversation_type = ConversationType::Dm;
    first.dm_id = Some("dm:one:two".into());
    let mut second = generate_group(None);
    second.conversation_type = ConversationType::Dm;
    second.dm_id = first.dm_id.clone();
    let mut other_dm = generate_group(None);
    other_dm.conversation_type = ConversationType::Dm;
    other_dm.dm_id = Some("dm:three:four".into());
    let regular = generate_group(None);
    for group in [&first, &second, &other_dm, &regular] {
        group.store(&db)?;
    }

    let start = db.current_delivery_cursor()?;
    let first_message = generate_message(None, Some(&first.id), None, None, None, None);
    let second_message = generate_message(None, Some(&second.id), None, None, None, None);
    first_message.store(&db)?;
    second_message.store(&db)?;
    generate_message(None, Some(&other_dm.id), None, None, None, None).store(&db)?;
    generate_message(None, Some(&regular.id), None, None, None, None).store(&db)?;

    let owner = db.acquire_delivery_owner(0, 100)?;
    for id in [first.id, second.id] {
        let scope = DeliveryScope::Groups(vec![id]);
        let expected = [&first_message.id, &second_message.id];
        let defaults = db.default_delivery_messages(owner, &scope, 1, 8)?;
        assert_eq!(
            defaults
                .iter()
                .map(|row| &row.message.id)
                .collect::<Vec<_>>(),
            expected
        );
        let replay = db.replay_delivery_messages(start, &scope, 1, 8)?;
        assert_eq!(
            replay.iter().map(|row| &row.message.id).collect::<Vec<_>>(),
            expected
        );
        let latest = db.delivery_history_snapshot(&scope, 1, 1)?;
        assert_eq!(latest.messages.len(), 1);
        assert_eq!(latest.messages[0].message.id, second_message.id);
    }
    assert!(
        db.default_delivery_messages(owner, &DeliveryScope::Groups(vec![]), 1, 8)?
            .is_empty()
    );

    let scope = DeliveryScope::Groups(vec![first.id]);
    let duplicate_row = db.default_delivery_messages(owner, &scope, 1, 8)?[1].clone();
    db.acknowledge_delivery(owner, second.id, duplicate_row.cursor, 1)?;
    let remaining = db.default_delivery_messages(owner, &scope, 1, 8)?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].message.id, first_message.id);
}

// verifies: DMS-009, DMS-016
#[xmtp_common::test(unwrap_try = true)]
async fn stitched_history_snapshot_keeps_the_recent_delivery_tail() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let mut first = generate_group(None);
    first.conversation_type = ConversationType::Dm;
    first.dm_id = Some("dm:one:two".into());
    let mut second = generate_group(None);
    second.conversation_type = ConversationType::Dm;
    second.dm_id = first.dm_id.clone();
    let unrelated = generate_group(None);
    for group in [&first, &second, &unrelated] {
        group.store(&db)?;
    }

    let oldest_delivery = generate_message(None, Some(&first.id), Some(400), None, None, None);
    let middle_delivery = generate_message(None, Some(&second.id), Some(300), None, None, None);
    let newest_delivery = generate_message(None, Some(&first.id), Some(100), None, None, None);
    oldest_delivery.store(&db)?;
    middle_delivery.store(&db)?;
    let middle_cursor = db.current_delivery_cursor()?;
    newest_delivery.store(&db)?;
    let newest_cursor = db.current_delivery_cursor()?;

    // Ordinary history selects one timestamp-ordered page across both groups.
    for id in [first.id, second.id] {
        let history = db.get_group_messages(
            &id,
            &crate::group_message::MsgQueryArgs {
                limit: Some(2),
                ..Default::default()
            },
        )?;
        assert_eq!(
            history.iter().map(|row| &row.id).collect::<Vec<_>>(),
            [&newest_delivery.id, &middle_delivery.id]
        );
    }

    // Expired and unrelated rows cannot consume the snapshot's global limit.
    generate_message(None, Some(&second.id), Some(500), None, Some(10), None).store(&db)?;
    generate_message(None, Some(&unrelated.id), Some(550), None, None, None).store(&db)?;
    let removed = generate_message(None, Some(&unrelated.id), Some(600), None, None, None);
    removed.store(&db)?;
    let boundary = db.current_delivery_cursor()?;
    db.raw_query(|conn| diesel::delete(group_messages::table.find(&removed.id)).execute(conn))?;
    assert!(boundary.delivery_sequence > newest_cursor.delivery_sequence);

    for id in [first.id, second.id] {
        let snapshot = db.delivery_history_snapshot(&DeliveryScope::Groups(vec![id]), 20, 2)?;
        assert_eq!(
            snapshot
                .messages
                .iter()
                .map(|row| &row.message.id)
                .collect::<Vec<_>>(),
            [&middle_delivery.id, &newest_delivery.id]
        );
        assert_eq!(
            snapshot
                .messages
                .iter()
                .map(|row| row.cursor)
                .collect::<Vec<_>>(),
            [middle_cursor, newest_cursor]
        );
        assert_eq!(snapshot.cursor, boundary);
    }

    let late = generate_message(None, Some(&second.id), Some(50), None, None, None);
    late.store(&db)?;
    let replay =
        db.replay_delivery_messages(boundary, &DeliveryScope::Groups(vec![first.id]), 20, 2)?;
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].message.id, late.id);
    assert_eq!(replay[0].cursor, db.current_delivery_cursor()?);
    assert!(replay[0].cursor.delivery_sequence > boundary.delivery_sequence);
}

// verifies: DMS-009
#[xmtp_common::test(unwrap_try = true)]
async fn scoped_delivery_includes_a_duplicate_added_after_the_first_read() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let mut first = generate_group(None);
    first.conversation_type = ConversationType::Dm;
    first.dm_id = Some("dm:one:two".into());
    first.store(&db)?;
    let owner = db.acquire_delivery_owner(0, 100)?;
    let scope = DeliveryScope::Groups(vec![first.id]);
    assert!(
        db.default_delivery_messages(owner, &scope, 1, 8)?
            .is_empty()
    );

    let mut later = generate_group(None);
    later.conversation_type = ConversationType::Dm;
    later.dm_id = first.dm_id.clone();
    later.store(&db)?;
    let message = generate_message(None, Some(&later.id), None, None, None, None);
    message.store(&db)?;
    let rows = db.default_delivery_messages(owner, &scope, 1, 8)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, message.id);
}

// verifies: PROC-031
#[xmtp_common::test(unwrap_try = true)]
async fn expired_owner_cannot_acknowledge_or_scan_after_takeover() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    generate_message(None, Some(&group.id), None, None, None, None).store(&db)?;
    let old = db.acquire_delivery_owner(0, 10)?;
    let cursor = db.default_delivery_messages(old, &DeliveryScope::All, 1, 8)?[0].cursor;
    assert!(matches!(
        db.acquire_delivery_owner(9, 20),
        Err(StorageError::Stream(StreamStorageError::AlreadyActive))
    ));
    let new = db.acquire_delivery_owner(10, 30)?;
    assert!(matches!(
        db.acknowledge_delivery(old, group.id, cursor, 11),
        Err(StorageError::Stream(StreamStorageError::NotCurrentOwner))
    ));
    assert!(
        db.default_delivery_messages(old, &DeliveryScope::All, 11, 8)
            .is_err()
    );
    assert!(db.renew_delivery_owner(old, 11, 100).is_err());
    db.release_delivery_owner(old)?;
    assert_eq!(
        db.default_delivery_messages(new, &DeliveryScope::All, 11, 8)?
            .len(),
        1
    );
    db.acknowledge_delivery(new, group.id, cursor, 11)?;
    assert!(
        db.default_delivery_messages(new, &DeliveryScope::All, 11, 8)?
            .is_empty()
    );
}

// verifies: PROC-033
#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshot_cursor_and_restore_identity_prevent_gaps() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    generate_message(None, Some(&group.id), None, None, None, None).store(&db)?;
    let snapshot = db.delivery_history_snapshot(&DeliveryScope::All, 0, 8)?;
    assert_eq!(snapshot.messages.len(), 1);
    let new = generate_message(None, Some(&group.id), None, None, None, None);
    new.store(&db)?;
    let rows = db.replay_delivery_messages(snapshot.cursor, &DeliveryScope::All, 0, 8)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, new.id);
    let owner = db.acquire_delivery_owner(0, 100)?;
    db.rotate_stream_database_id()?;
    assert!(matches!(
        db.replay_delivery_messages(snapshot.cursor, &DeliveryScope::All, 1, 8),
        Err(StorageError::Stream(StreamStorageError::ForeignCursor))
    ));
    assert!(db.check_delivery_owner(owner, 1).is_err());
}

// verifies: PROC-024
#[xmtp_common::test(unwrap_try = true)]
async fn allocator_exhaustion_rolls_back_message_insertion() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    db.raw_query(|conn| {
        diesel::update(progress::table.find((ALLOCATOR_ID, EntityKind::DeliveryAllocator)))
            .set(progress::sequence_id.eq(i64::MAX))
            .execute(conn)
    })?;
    let message = generate_message(None, Some(&group.id), None, None, None, None);
    assert!(matches!(
        message.store(&db),
        Err(StorageError::Stream(StreamStorageError::DeliveryExhausted))
    ));
    assert!(db.get_group_message(&message.id)?.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn local_byte_limit_retains_an_ordered_prefix_and_does_not_consume_oversized_rows() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let start = db.current_delivery_cursor()?;
    let first = generate_message(None, Some(&group.id), None, None, None, None);
    let mut second = generate_message(None, Some(&group.id), None, None, None, None);
    second.decrypted_message_bytes = vec![1; 4096];
    first.store(&db)?;
    second.store(&db)?;
    let owner = db.acquire_delivery_owner(0, 100)?;
    let rows = db.default_delivery_messages_bounded(owner, &DeliveryScope::All, 1, 8, 2048)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, first.id);
    db.acknowledge_delivery(owner, group.id, rows[0].cursor, 1)?;
    assert!(matches!(
        db.default_delivery_messages_bounded(owner, &DeliveryScope::All, 1, 8, 2048),
        Err(StorageError::Stream(
            StreamStorageError::LocalReadCapacity { .. }
        ))
    ));
    let rows = db.default_delivery_messages_bounded(owner, &DeliveryScope::All, 1, 8, 8192)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.id, second.id);
    let replay = db.replay_delivery_messages_bounded(start, &DeliveryScope::All, 1, 8, 2048)?;
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].message.id, first.id);
}

struct AdvanceClockOnConnection<C> {
    inner: C,
    clock: std::sync::Arc<std::sync::atomic::AtomicI64>,
    next_time: i64,
}

#[cfg(not(target_arch = "wasm32"))]
struct AwaitHistoryConnection<C> {
    inner: C,
    waiting: std::sync::mpsc::SyncSender<()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<C: ConnectionExt> ConnectionExt for AwaitHistoryConnection<C> {
    fn raw_query<T, F>(&self, work: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut diesel::SqliteConnection) -> Result<T, diesel::result::Error>,
    {
        self.waiting.send(()).expect("history query started");
        self.inner.raw_query(work)
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        self.inner.disconnect()
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        self.inner.reconnect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn history_excludes_message_expired_while_waiting_for_connection() {
    use std::sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
        mpsc::sync_channel,
    };

    let store = TestDb::create_ephemeral_store().await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut expired = generate_message(None, Some(&group.id), Some(1), None, None, None);
    expired.expire_at_ns = Some(100);
    expired.store(&db)?;
    let retained = generate_message(None, Some(&group.id), Some(2), None, None, None);
    retained.store(&db)?;

    let (held_tx, held_rx) = sync_channel(0);
    let (release_tx, release_rx) = sync_channel(0);
    let held_db = db.clone();
    let holder = std::thread::spawn(move || {
        held_db.raw_query(|_| {
            held_tx.send(()).expect("connection held");
            release_rx.recv().expect("release connection");
            Ok(())
        })
    });
    held_rx.recv()?;
    let clock = Arc::new(AtomicI64::new(99));
    let read_clock = Arc::clone(&clock);
    let (waiting_tx, waiting_rx) = sync_channel(0);
    let delayed = AwaitHistoryConnection {
        inner: db.clone(),
        waiting: waiting_tx,
    };
    let reader = std::thread::spawn(move || {
        delayed.delivery_history_snapshot_projected_with_clock(
            &DeliveryScope::All,
            &DeliveryFilter::default(),
            || read_clock.load(Ordering::SeqCst),
            8,
            u64::MAX,
            |_, snapshot| Ok(snapshot),
        )
    });
    waiting_rx.recv()?;
    clock.store(101, Ordering::SeqCst);
    release_tx.send(())?;
    holder.join().expect("connection holder")?;
    let snapshot = reader.join().expect("history reader")?;
    assert_eq!(
        snapshot.messages.len(),
        1,
        "history returned expired content"
    );
    assert_eq!(snapshot.messages[0].message.id, retained.id);
    assert!(db.get_group_message(&expired.id)?.is_some());
}

impl<C: ConnectionExt> ConnectionExt for AdvanceClockOnConnection<C> {
    fn raw_query<T, F>(&self, work: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut diesel::SqliteConnection) -> Result<T, diesel::result::Error>,
    {
        self.inner.raw_query(|conn| {
            self.clock
                .store(self.next_time, std::sync::atomic::Ordering::SeqCst);
            work(conn)
        })
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        self.inner.disconnect()
    }
    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        self.inner.reconnect()
    }
}

// verifies: PROC-031
#[xmtp_common::test(unwrap_try = true)]
async fn delayed_connection_uses_the_new_clock_before_acknowledgement_and_renewal() {
    use std::sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    };
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    generate_message(None, Some(&group.id), None, None, None, None).store(&db)?;
    let owner = db.acquire_delivery_owner(0, 10)?;
    let cursor = db.current_delivery_cursor()?;
    let clock = Arc::new(AtomicI64::new(9));
    let delayed = AdvanceClockOnConnection {
        inner: &db,
        clock: Arc::clone(&clock),
        next_time: 10,
    };
    assert!(matches!(
        delayed.acknowledge_delivery_with_clock(owner, group.id, cursor, || clock
            .load(Ordering::SeqCst)),
        Err(StorageError::Stream(StreamStorageError::NotCurrentOwner))
    ));
    clock.store(9, Ordering::SeqCst);
    assert!(matches!(
        delayed.renew_delivery_owner_with_clock(owner, 20, || clock.load(Ordering::SeqCst)),
        Err(StorageError::Stream(StreamStorageError::NotCurrentOwner))
    ));
    let replacement = db.acquire_delivery_owner(10, 30)?;
    assert_eq!(
        db.default_delivery_messages(replacement, &DeliveryScope::All, 11, 8)?
            .len(),
        1
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshot_filters_before_its_limit_in_the_same_database_snapshot() {
    use crate::consent_record::StoredConsentRecord;
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let allowed = generate_group(None);
    let denied = generate_group(None);
    allowed.store(&db)?;
    denied.store(&db)?;
    StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(allowed.id),
    )
    .store(&db)?;
    StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Denied,
        hex::encode(denied.id),
    )
    .store(&db)?;
    let included = generate_message(None, Some(&allowed.id), None, None, None, None);
    included.store(&db)?;
    generate_message(None, Some(&denied.id), None, None, None, None).store(&db)?;
    let snapshot = db.delivery_history_snapshot_filtered(
        &DeliveryScope::All,
        &DeliveryFilter {
            conversation_type: Some(allowed.conversation_type),
            consent_states: Some(vec![ConsentState::Allowed]),
        },
        0,
        1,
        8192,
    )?;
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].message.id, included.id);
    assert_eq!(snapshot.cursor, db.current_delivery_cursor()?);
    assert!(snapshot.cursor.delivery_sequence > snapshot.messages[0].cursor.delivery_sequence);
}

type AppRowsObserver = Box<dyn FnMut(Option<usize>)>;

thread_local! {
    static APP_ROWS_PROBE: std::cell::RefCell<Option<AppRowsObserver>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn observe_app_rows(batch: Option<usize>) {
    APP_ROWS_PROBE.with_borrow_mut(|probe| {
        if let Some(probe) = probe {
            probe(batch);
        }
    });
}

// verifies: PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn app_rows_keep_snapshot_across_publication_and_deletion() {
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let writer = TestDb::create_persistent_store(Some(path)).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut pending = generate_message(None, Some(&group.id), Some(1), None, None, None);
    pending.delivery_status = DeliveryStatus::Unpublished;
    pending.store(&db)?;
    let retained = generate_message(None, Some(&group.id), Some(2), None, None, None);
    retained.store(&db)?;
    let committed = db.current_delivery_cursor()?;
    let pending_id = pending.id.clone();
    let retained_id = retained.id.clone();
    APP_ROWS_PROBE.with_borrow_mut(|probe| {
        *probe = Some(Box::new(move |batch| {
            if batch.is_none() {
                let db = writer.db();
                db.set_delivery_status_to_published(&pending_id, 3, Cursor(3), None)
                    .unwrap();
                db.raw_query(|conn| {
                    diesel::delete(group_messages::table.find(&retained_id)).execute(conn)
                })
                .unwrap();
            }
        }))
    });
    let rows = db.app_visible_message_rows(&group.id, &MsgQueryArgs::default())?;
    APP_ROWS_PROBE.with_borrow_mut(|probe| *probe = None);
    let old_pending = rows
        .iter()
        .find(|row| row.stored.id == pending.id)
        .expect("unpublished snapshot row");
    assert_eq!(
        old_pending.stored.delivery_status,
        DeliveryStatus::Unpublished
    );
    assert_eq!(old_pending.cursor, None);
    let old_retained = rows
        .iter()
        .find(|row| row.stored.id == retained.id)
        .expect("retained snapshot row");
    assert_eq!(old_retained.cursor, Some(committed));
    assert!(db.app_visible_message_row(&retained.id, 0)?.is_none());
    assert!(
        db.app_visible_message_row(&pending.id, 0)?
            .expect("published")
            .cursor
            .is_some()
    );
}

// verifies: PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn app_rows_cursor_queries_are_bounded_batches() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let mut expected = Vec::new();
    for index in 0..1001 {
        let message = generate_message(None, Some(&group.id), Some(index), None, None, None);
        message.store(&db)?;
        expected.push((message.id, Some(db.current_delivery_cursor()?)));
    }
    let batches = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = batches.clone();
    APP_ROWS_PROBE.with_borrow_mut(|probe| {
        *probe = Some(Box::new(move |batch| {
            if let Some(size) = batch {
                observed.borrow_mut().push(size);
            }
        }))
    });
    let rows = db.app_visible_message_rows(
        &group.id,
        &MsgQueryArgs {
            direction: Some(crate::group_message::SortDirection::Ascending),
            ..Default::default()
        },
    )?;
    APP_ROWS_PROBE.with_borrow_mut(|probe| *probe = None);
    assert_eq!(rows.len(), expected.len());
    for (index, (row, expected)) in rows.into_iter().zip(expected).enumerate() {
        assert_eq!((row.stored.id, row.cursor), expected, "row {index}");
    }
    assert_eq!(batches.borrow().len(), 3);
    assert!(batches.borrow().iter().all(|size| *size <= 500));
}

// verifies: DMS-016
#[xmtp_common::test(unwrap_try = true)]
async fn history_projection_keeps_relations_and_deletions_in_the_cursor_snapshot() {
    use crate::group_message::RelationQuery;
    use crate::message_deletion::{QueryMessageDeletion, StoredMessageDeletion};
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let writer = TestDb::create_persistent_store(Some(path)).await;
    let db = store.db();
    let group = generate_group(None);
    group.store(&db)?;
    let parent = generate_message(None, Some(&group.id), Some(1), None, None, None);
    parent.store(&db)?;
    let boundary = db.current_delivery_cursor()?;
    let projected = db.delivery_history_snapshot_projected(
        &DeliveryScope::Groups(vec![group.id]),
        &Default::default(),
        0,
        8,
        u64::MAX,
        |conn, snapshot| {
            // Commit after selection, before relation and deletion queries.
            let mut reply = generate_message(None, Some(&group.id), Some(2), None, None, None);
            reply.reference_id = Some(parent.id.clone());
            reply.store(&writer.db())?;
            StoredMessageDeletion {
                id: reply.id,
                group_id: group.id,
                deleted_message_id: parent.id.clone(),
                deleted_by_inbox_id: parent.sender_inbox_id.clone(),
                is_super_admin_deletion: false,
                deleted_at_ns: 3,
            }
            .store(&writer.db())?;
            let storage = conn.key_store();
            let projected_db = storage.db();
            let counts = projected_db.get_inbound_relation_counts(
                &group.id,
                &[&parent.id],
                RelationQuery::default(),
            )?;
            let deletions = projected_db.get_deletions_for_messages(vec![parent.id.clone()])?;
            assert!(
                counts.is_empty(),
                "projection observed a reply after its cursor"
            );
            assert!(
                deletions.is_empty(),
                "projection observed a deletion after its cursor"
            );
            Ok(snapshot)
        },
    )?;
    assert_eq!(projected.cursor, boundary);
    assert_eq!(projected.messages.len(), 1);
    assert_eq!(projected.messages[0].message.id, parent.id);
    assert_eq!(projected.messages[0].cursor, boundary);
    assert_eq!(
        db.get_inbound_relation_counts(&group.id, &[&parent.id], RelationQuery::default())?
            .get(&parent.id),
        Some(&1)
    );
    assert_eq!(db.get_deletions_for_messages(vec![parent.id])?.len(), 1);
    assert!(db.current_delivery_cursor()?.delivery_sequence > boundary.delivery_sequence);
}

// verifies: SYNC-005, PROC-026, PROC-033, PROC-034
#[xmtp_common::test(unwrap_try = true)]
async fn all_delivery_hides_sync_before_limits_without_consuming_sync_progress() {
    use crate::consent_record::StoredConsentRecord;
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = generate_group(None);
    let mut dm = generate_group(None);
    dm.conversation_type = ConversationType::Dm;
    let mut sync = generate_group(None);
    sync.conversation_type = ConversationType::Sync;
    for group in [&group, &dm, &sync] {
        group.store(&db)?;
    }
    StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(sync.id),
    )
    .store(&db)?;
    let start = db.current_delivery_cursor()?;
    let mut large_sync = generate_message(None, Some(&sync.id), None, None, None, None);
    large_sync.decrypted_message_bytes = vec![0x5a; 1024 * 1024];
    large_sync.store(&db)?;
    let first = generate_message(None, Some(&group.id), None, None, None, None);
    first.store(&db)?;
    let mut key_update = generate_message(None, Some(&sync.id), None, None, None, None);
    key_update.decrypted_message_bytes = vec![0x6b; 42];
    key_update.store(&db)?;
    let second = generate_message(None, Some(&dm.id), None, None, None, None);
    second.store(&db)?;
    let mut newest_sync = generate_message(None, Some(&sync.id), None, None, None, None);
    newest_sync.decrypted_message_bytes = vec![0x7c; 42];
    newest_sync.store(&db)?;
    let boundary = db.current_delivery_cursor()?;
    let selection = DeliveryFilter {
        conversation_type: None,
        consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
    };
    let snapshot =
        db.delivery_history_snapshot_filtered(&DeliveryScope::All, &selection, 0, 2, 8192)?;
    assert_eq!(
        snapshot
            .messages
            .iter()
            .map(|row| &row.message.id)
            .collect::<Vec<_>>(),
        [&first.id, &second.id]
    );
    assert_eq!(snapshot.cursor, boundary);
    let replay = db.replay_delivery_messages_bounded(start, &DeliveryScope::All, 0, 2, 8192)?;
    assert_eq!(
        replay.iter().map(|row| &row.message.id).collect::<Vec<_>>(),
        [&first.id, &second.id]
    );
    let owner = db.acquire_delivery_owner(0, 100)?;
    let candidates =
        db.default_delivery_messages_bounded(owner, &DeliveryScope::All, 1, 2, 8192)?;
    assert_eq!(
        candidates
            .iter()
            .map(|row| &row.message.id)
            .collect::<Vec<_>>(),
        [&first.id, &second.id]
    );
    assert_eq!(db.current_delivery_cursor()?, boundary);
    db.acknowledge_delivery(owner, group.id, candidates[0].cursor, 1)?;
    let remaining = db.default_delivery_messages(owner, &DeliveryScope::All, 1, 2)?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].message.id, second.id);
    // Device-sync reads use the explicit stored-group path, not app delivery.
    let explicit = db.get_group_messages(&sync.id, &MsgQueryArgs::default())?;
    assert_eq!(explicit.len(), 3);
    assert!(explicit.iter().any(|row| row.id == key_update.id));
    assert!(
        explicit
            .iter()
            .any(|row| row.decrypted_message_bytes == vec![0x6b; 42])
    );
    // Even an explicit type on app delivery keeps its existing virtual-group exclusion.
    let explicit_delivery = db.delivery_history_snapshot_filtered(
        &DeliveryScope::All,
        &DeliveryFilter {
            conversation_type: Some(ConversationType::Sync),
            ..Default::default()
        },
        0,
        3,
        2 * 1024 * 1024,
    )?;
    assert!(explicit_delivery.messages.is_empty());
    db.acknowledge_delivery(owner, dm.id, candidates[1].cursor, 1)?;
    assert!(
        db.default_delivery_messages(owner, &DeliveryScope::All, 1, 8)?
            .is_empty()
    );
    let explicit_after = db.get_group_messages(&sync.id, &MsgQueryArgs::default())?;
    assert_eq!(
        explicit_after.len(),
        3,
        "app delivery must not consume or delete Sync messages"
    );
    db.release_delivery_owner(owner)?;
}
