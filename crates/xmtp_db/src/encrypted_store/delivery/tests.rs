use super::*;
use crate::group::tests::generate_group;
use crate::group_message::tests::generate_message;
use crate::schema::group_messages;
use crate::{Store, StoreOrIgnore, TestDb, XmtpTestDb, prelude::*};
use xmtp_proto::types::Cursor;

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
