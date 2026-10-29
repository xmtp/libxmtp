use super::*;
use crate::sql_key_store::SqlKeyStore;
use crate::{
    TestDb, TransactionOutcome, TransactionalKeyStore, XmtpMlsStorageProvider, XmtpTestDb,
};
use diesel::connection::SimpleConnection;

fn topic(id: u8, kind: NetworkEntityKind) -> StreamTopic {
    StreamTopic {
        entity_id: vec![id],
        kind,
    }
}

fn batch(ids: &[u64]) -> Vec<NewIncomingEnvelope> {
    ids.iter()
        .map(|id| NewIncomingEnvelope {
            sequence_id: Cursor(*id),
            envelope: vec![1; 4],
        })
        .collect()
}

/// Block a pending row so only a new coordinator generation rechecks it.
fn hold(
    db: &impl QueryIncomingEnvelope,
    topic: &StreamTopic,
    sequence: u64,
) -> Result<bool, StorageError> {
    db.set_incoming_retry(
        topic,
        Cursor(sequence),
        &IncomingRetry {
            retry_at_ns: 200,
            blocked: true,
            error_code: Some("TEST_BLOCKED".into()),
            retry_expires_at_ns: None,
        },
    )
}

fn limits() -> IncomingLimits {
    IncomingLimits {
        batch: PendingBudget { rows: 8, bytes: 32 },
        topic: PendingBudget { rows: 8, bytes: 32 },
        kind: PendingBudget {
            rows: 16,
            bytes: 64,
        },
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_discovery_is_immutable_and_excludes_local_and_imported_groups() {
    use crate::{
        Store,
        group::{QueryGroup, tests::generate_group},
    };
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let db = store.db();
    let discovered = generate_group(None);
    let later = generate_group(None);
    let local = generate_group(None);
    let mut imported = generate_group(None);
    imported.sequence_id = Some(1);
    for group in [&discovered, &later, &local, &imported] {
        group.store(&db)?;
    }
    db.record_welcome_discovery(discovered.id, Cursor(10))?;
    db.install_group_anchor(discovered.id, Cursor(50), JoinAnchorMode::Advance)?;
    let mut rejoin = discovered.clone();
    rejoin.sequence_id = Some(100);
    assert_eq!(db.insert_or_replace_group(rejoin)?.sequence_id, Some(100));
    db.record_welcome_discovery(discovered.id, Cursor(100))?;
    db.record_welcome_discovery(later.id, Cursor(20))?;
    // An older out-of-order rejoin also cannot rewrite the first successful discovery.
    db.record_welcome_discovery(later.id, Cursor(5))?;
    assert!(db.group_ids_discovered_through(Cursor(0))?.is_empty());
    assert!(db.group_ids_discovered_through(Cursor(9))?.is_empty());
    assert_eq!(
        db.group_ids_discovered_through(Cursor(10))?,
        vec![discovered.id]
    );
    assert_eq!(
        db.group_ids_discovered_through(Cursor(100))?,
        vec![discovered.id, later.id]
    );
    assert!(db.record_welcome_discovery(local.id, Cursor(0)).is_err());
    drop(db);
    drop(store);
    let store = TestDb::create_persistent_store(Some(path)).await;
    assert_eq!(
        store.db().group_ids_discovered_through(Cursor(10))?,
        vec![discovered.id]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_discovery_rolls_back_with_installation_state() {
    use crate::{Store, group::tests::generate_group};
    let store = TestDb::create_persistent_store(None).await;
    let group = generate_group(None);
    group.store(&store.db())?;
    let keys = SqlKeyStore::new(store.conn());
    let outcome: TransactionOutcome<()> = keys.transaction::<_, StorageError, _>(|conn| {
        let provider = conn.key_store();
        provider
            .db()
            .record_welcome_discovery(group.id, Cursor(10))?;
        provider
            .db()
            .install_group_anchor(group.id, Cursor(50), JoinAnchorMode::Advance)?;
        Ok(TransactionOutcome::Rollback)
    })?;
    assert!(matches!(outcome, TransactionOutcome::Rollback));
    assert!(
        store
            .db()
            .group_ids_discovered_through(Cursor(10))?
            .is_empty()
    );
    assert_eq!(
        store.db().topic_progress(&StreamTopic::group(group.id))?,
        TopicProgress::default()
    );
}

// verifies: PROC-002
#[xmtp_common::test(unwrap_try = true)]
async fn admission_rolls_back_the_batch_and_received_position() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = topic(1, NetworkEntityKind::Group);
    db.admit_ordered_batch(&group, Cursor(0), &batch(&[10]), limits())?;
    db.raw_query(|conn| conn.batch_execute("CREATE TEMP TRIGGER fail_admission BEFORE INSERT ON incoming_envelopes WHEN NEW.sequence_id = 30 BEGIN SELECT RAISE(ABORT, 'injected receipt failure'); END;"))?;
    assert!(
        db.admit_ordered_batch(&group, Cursor(10), &batch(&[20, 30]), limits())
            .is_err()
    );
    assert_eq!(db.topic_progress(&group)?.received, Cursor(10));
    assert_eq!(db.pending_topic_usage(NetworkEntityKind::Group)?[0].rows, 1);
    assert_eq!(db.first_pending_envelope(&group)?.unwrap().sequence_id, 10);
}

// verifies: PROC-002, PROC-005, PROC-013
#[xmtp_common::test(unwrap_try = true)]
async fn overlaps_and_sparse_ids_preserve_the_pending_head() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = topic(1, NetworkEntityKind::Group);
    let other = topic(2, NetworkEntityKind::Group);
    db.admit_ordered_batch(&group, Cursor(0), &batch(&[10, 30]), limits())?;
    let repeated = db.admit_ordered_batch(&group, Cursor(0), &batch(&[10, 30, 50]), limits())?;
    assert_eq!(repeated.inserted, 1);
    assert!(matches!(
        db.admit_ordered_batch(&group, Cursor(60), &batch(&[70]), limits()),
        Err(StorageError::Stream(
            StreamStorageError::MissingPrefix { .. }
        ))
    ));
    hold(&db, &group, 10)?;
    assert!(matches!(
        db.complete_pending_envelope(&group, Cursor(30)),
        Err(StorageError::Stream(StreamStorageError::HeadChanged))
    ));
    db.admit_ordered_batch(&other, Cursor(0), &batch(&[60]), limits())?;
    db.complete_pending_envelope(&other, Cursor(60))?;
    assert_eq!(db.topic_progress(&group)?.processed, Cursor(0));
    assert_eq!(db.topic_progress(&other)?.processed, Cursor(60));
    db.complete_pending_envelope(&group, Cursor(10))?;
    assert_eq!(db.first_pending_envelope(&group)?.unwrap().sequence_id, 30);
    assert!(!db.set_incoming_retry(
        &group,
        Cursor(10),
        &IncomingRetry {
            retry_at_ns: 900,
            blocked: false,
            error_code: None,
            retry_expires_at_ns: None,
        }
    )?);
}

// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn bounds_leave_progress_unchanged_and_reserve_dependency_capacity() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = topic(1, NetworkEntityKind::Group);
    let other = topic(2, NetworkEntityKind::Group);
    let mut bounded = limits();
    bounded.topic = PendingBudget { rows: 1, bytes: 4 };
    bounded.kind = PendingBudget { rows: 2, bytes: 8 };
    db.admit_ordered_batch(&group, Cursor(0), &batch(&[10]), bounded)?;
    assert!(matches!(
        db.admit_ordered_batch(&group, Cursor(10), &batch(&[20]), bounded),
        Err(StorageError::Stream(StreamStorageError::Capacity {
            scope: BudgetScope::Topic
        }))
    ));
    db.admit_ordered_batch(&other, Cursor(0), &batch(&[20]), bounded)?;
    assert!(matches!(
        db.admit_ordered_batch(
            &topic(3, NetworkEntityKind::Group),
            Cursor(0),
            &batch(&[30]),
            bounded
        ),
        Err(StorageError::Stream(StreamStorageError::Capacity {
            scope: BudgetScope::Kind
        }))
    ));
    for kind in [NetworkEntityKind::Welcome, NetworkEntityKind::Identity] {
        db.admit_ordered_batch(&topic(1, kind), Cursor(0), &batch(&[40]), bounded)?;
    }
    assert_eq!(db.topic_progress(&group)?.received, Cursor(10));
    assert_eq!(
        db.topic_progress(&topic(3, NetworkEntityKind::Group))?,
        TopicProgress::default()
    );
    let mut batch_limited = bounded;
    batch_limited.batch.bytes = 3;
    assert!(matches!(
        db.admit_ordered_batch(&other, Cursor(20), &batch(&[50]), batch_limited),
        Err(StorageError::Stream(StreamStorageError::Capacity {
            scope: BudgetScope::Batch
        }))
    ));
}

/// Held work never drains, so it is charged to its own allowance and cannot
/// stop admission of ready work.
// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn held_work_uses_a_separate_allowance() {
    enum Hold {
        Blocked,
        Pointer,
        Unreadable,
    }
    let deadline = xmtp_common::time::now_ns().saturating_add(xmtp_common::NS_IN_DAY);
    // One Welcome topic per installation: its topic budget equals the kind budget.
    let mut limits = limits();
    limits.topic = limits.kind;
    let ids = |round: u64| (1..=8).map(|i| round * 80 + i * 10).collect::<Vec<_>>();
    for (kind, shape) in [
        (NetworkEntityKind::Group, Hold::Blocked),
        (NetworkEntityKind::Welcome, Hold::Blocked),
        (NetworkEntityKind::Welcome, Hold::Pointer),
        (NetworkEntityKind::Welcome, Hold::Unreadable),
    ] {
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let group = kind == NetworkEntityKind::Group;
        let apply = |held: &StreamTopic, id: u64| match shape {
            Hold::Blocked => hold(&db, held, id),
            Hold::Pointer => db.set_incoming_retry(
                held,
                Cursor(id),
                &IncomingRetry {
                    retry_at_ns: 0,
                    blocked: false,
                    error_code: Some("welcome_pointee".into()),
                    retry_expires_at_ns: Some(deadline),
                },
            ),
            Hold::Unreadable => db
                .set_unreadable_welcome_deadline(&held.entity_id, Cursor(id), deadline)
                .map(|deadline| deadline.is_some()),
        };
        // Fill the kind budget with held rows: two blocked group heads with 7
        // tails each, or 16 held Welcomes on one topic.
        for round in 0..2 {
            let held = topic(if group { round as u8 + 1 } else { 1 }, kind);
            let after = db.topic_progress(&held)?.received;
            db.admit_ordered_batch(&held, after, &batch(&ids(round)), limits)?;
            for id in &ids(round)[..if group { 1 } else { 8 }] {
                assert!(apply(&held, *id)?);
            }
        }
        let ready = topic(if group { 3 } else { 1 }, kind);
        let after = db.topic_progress(&ready)?.received;
        db.admit_ordered_batch(&ready, after, &batch(&ids(2)), limits)?;
        if group {
            let first = topic(1, kind);
            assert!(matches!(
                db.admit_ordered_batch(&first, Cursor(80), &batch(&[90]), limits),
                Err(StorageError::Stream(StreamStorageError::Capacity {
                    scope: BudgetScope::Kind
                }))
            ));
            db.admit_ordered_batch(&topic(4, kind), Cursor(0), &batch(&[10]), limits)?;
            // A released head moves its rows to the ready pool.
            assert!(db.set_incoming_retry(
                &first,
                Cursor(10),
                &IncomingRetry {
                    retry_at_ns: 0,
                    blocked: false,
                    error_code: None,
                    retry_expires_at_ns: None,
                },
            )?);
            assert!(matches!(
                db.admit_ordered_batch(&topic(5, kind), Cursor(0), &batch(&[10]), limits),
                Err(StorageError::Stream(StreamStorageError::Capacity {
                    scope: BudgetScope::Kind
                }))
            ));
        } else {
            assert!(db.oldest_held_welcome_over(&ready, limits.kind)?.is_none());
            // A held Welcome on another topic fills the kind's held bytes.
            let other = topic(2, kind);
            db.admit_ordered_batch(&other, Cursor(0), &batch(&[10]), limits)?;
            assert!(apply(&other, 10)?);
            let bytes_only = PendingBudget {
                rows: u64::MAX,
                ..limits.kind
            };
            let oldest = db.oldest_held_welcome_over(&ready, bytes_only)?;
            assert_eq!(oldest.map(|row| row.sequence_id), Some(10));
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_completion_retains_holes_and_the_first_retry_deadline() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let welcome = topic(1, NetworkEntityKind::Welcome);
    db.admit_ordered_batch(&welcome, Cursor(0), &batch(&[10, 30, 80]), limits())?;
    let retry = IncomingRetry {
        retry_at_ns: 100,
        blocked: false,
        error_code: None,
        retry_expires_at_ns: Some(500),
    };
    db.set_incoming_retry(&welcome, Cursor(10), &retry)?;
    db.set_incoming_retry(
        &welcome,
        Cursor(10),
        &IncomingRetry {
            retry_expires_at_ns: Some(900),
            ..retry
        },
    )?;
    assert_eq!(
        db.first_pending_envelope(&welcome)?
            .unwrap()
            .retry_expires_at_ns,
        Some(500)
    );
    assert_eq!(db.ready_welcomes(0, 8)?.len(), 2);
    db.complete_pending_envelope(&welcome, Cursor(30))?;
    assert!(!db.welcome_barrier_complete(&welcome, Cursor(30))?);
    assert!(db.has_pending_welcomes()?);
    db.complete_pending_envelope(&welcome, Cursor(10))?;
    assert!(db.welcome_barrier_complete(&welcome, Cursor(30))?);
    assert!(!db.welcome_barrier_complete(&welcome, Cursor(80))?);
    assert_eq!(db.topic_progress(&welcome)?.processed, Cursor(79));
}

/// Pointer fetching and unreadable input have independent, immutable deadlines.
/// A restart or an intervening retry cause must not reset either retention clock.
/// Completion must not let a stale Welcome retry modify another queue's row.
#[xmtp_common::test(unwrap_try = true)]
async fn unreadable_welcome_deadline_survives_other_holds_and_restarts() {
    let path = xmtp_common::tmp_path();
    let store = TestDb::create_persistent_store(Some(path.clone())).await;
    let db = store.db();
    let welcome = topic(1, NetworkEntityKind::Welcome);
    db.admit_ordered_batch(&welcome, Cursor(0), &batch(&[10]), limits())?;
    let group = topic(1, NetworkEntityKind::Group);
    db.admit_ordered_batch(&group, Cursor(0), &batch(&[10]), limits())?;
    let retry = IncomingRetry {
        retry_at_ns: 100,
        blocked: false,
        error_code: Some("welcome_pointee".into()),
        retry_expires_at_ns: Some(500),
    };
    db.set_incoming_retry(&welcome, Cursor(10), &retry)?;
    assert_eq!(
        db.set_unreadable_welcome_deadline(&welcome.entity_id, Cursor(10), 900)?,
        Some(900)
    );
    db.set_incoming_retry(
        &welcome,
        Cursor(10),
        &IncomingRetry {
            error_code: Some("identity_dependency".into()),
            retry_expires_at_ns: None,
            ..retry
        },
    )?;
    drop(db);
    drop(store);
    let store = TestDb::create_persistent_store(Some(path)).await;
    let db = store.db();
    assert_eq!(
        db.set_unreadable_welcome_deadline(&welcome.entity_id, Cursor(10), 1200)?,
        Some(900)
    );
    let pending = db.pending_envelope(&welcome, Cursor(10))?.unwrap();
    assert_eq!(pending.retry_expires_at_ns, Some(500));
    assert_eq!(pending.unreadable_expires_at_ns, Some(900));
    db.complete_pending_envelope(&welcome, Cursor(10))?;
    assert_eq!(
        db.set_unreadable_welcome_deadline(&welcome.entity_id, Cursor(10), 1500)?,
        None
    );
    assert!(!db.has_pending_welcomes()?);
    assert_eq!(
        db.pending_envelope(&group, Cursor(10))?
            .unwrap()
            .unreadable_expires_at_ns,
        None
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn blocked_welcomes_require_a_generation_scan_not_a_timer_retry() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let welcome = topic(1, NetworkEntityKind::Welcome);
    db.admit_ordered_batch(&welcome, Cursor(0), &batch(&[10, 30]), limits())?;
    let retry = IncomingRetry {
        retry_at_ns: 100,
        blocked: true,
        error_code: Some("TEST_BLOCKED".into()),
        retry_expires_at_ns: Some(500),
    };
    db.set_incoming_retry(&welcome, Cursor(10), &retry)?;
    let ready = db.ready_welcomes_bounded(99, 8, 32, &[])?;
    assert_eq!(
        ready.iter().map(|row| row.sequence_id).collect::<Vec<_>>(),
        vec![30]
    );
    let ready = db.ready_welcomes_bounded(100, 8, 32, &[])?;
    assert_eq!(
        ready.iter().map(|row| row.sequence_id).collect::<Vec<_>>(),
        vec![30]
    );
    let blocked = db.blocked_welcomes_bounded(&welcome, Cursor(0), 1, 4)?;
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].sequence_id, 10);
    assert!(blocked[0].blocked);
    assert_eq!(blocked[0].error_code, retry.error_code);
    assert_eq!(blocked[0].retry_expires_at_ns, Some(500));
    assert!(
        db.blocked_welcomes_bounded(&welcome, Cursor(10), 8, 32)?
            .is_empty()
    );
    assert!(
        db.blocked_welcomes_bounded(&welcome, Cursor(0), 8, 3)
            .is_err()
    );
    assert_eq!(db.ready_welcomes(100, 8)?.len(), 1);
    assert_eq!(
        db.topic_progress(&welcome)?,
        TopicProgress {
            processed: Cursor(0),
            received: Cursor(30),
        }
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn pending_states_include_only_actual_topic_ids_through_the_target() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let welcome = topic(1, NetworkEntityKind::Welcome);
    db.admit_ordered_batch(&welcome, Cursor(0), &batch(&[10, 40, 90]), limits())?;
    db.admit_ordered_batch(
        &topic(2, NetworkEntityKind::Welcome),
        Cursor(0),
        &batch(&[20]),
        limits(),
    )?;
    db.admit_ordered_batch(
        &topic(1, NetworkEntityKind::Identity),
        Cursor(0),
        &batch(&[30]),
        limits(),
    )?;
    hold(&db, &welcome, 40)?;
    assert_eq!(
        db.pending_states_through(&welcome, Cursor(40))?,
        vec![
            PendingEnvelopeState {
                sequence_id: Cursor(10),
                blocked: false,
                error_code: None,
                retry_at_ns: 0,
            },
            PendingEnvelopeState {
                sequence_id: Cursor(40),
                blocked: true,
                error_code: Some("TEST_BLOCKED".into()),
                retry_at_ns: 200,
            },
        ]
    );
    assert!(db.pending_states_through(&welcome, Cursor(0))?.is_empty());
}

/// Scheduled Welcomes must not consume ready-read budgets or lose their
/// retention state, so a slow dependency cannot starve independent joins.
// verifies: PROC-013
#[xmtp_common::test(unwrap_try = true)]
async fn scheduled_welcomes_do_not_consume_ready_read_budgets() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let welcome = topic(1, NetworkEntityKind::Welcome);
    let mut envelopes = batch(&[10, 30]);
    envelopes[0].envelope = vec![1; 12];
    db.admit_ordered_batch(&welcome, Cursor(0), &envelopes, limits())?;
    db.set_incoming_retry(
        &welcome,
        Cursor(10),
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: false,
            error_code: Some("welcome_pointee".into()),
            retry_expires_at_ns: Some(500),
        },
    )?;
    db.set_unreadable_welcome_deadline(&welcome.entity_id, Cursor(10), 900)?;
    let ready = db.ready_welcomes_bounded(0, 1, 4, &[Cursor(10)])?;
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].sequence_id, 30);
    let pending = db.pending_envelope(&welcome, Cursor(10))?.unwrap();
    assert_eq!(pending.envelope, envelopes[0].envelope);
    assert_eq!(pending.retry_at_ns, 0);
    assert!(!pending.blocked);
    assert_eq!(pending.error_code.as_deref(), Some("welcome_pointee"));
    assert_eq!(pending.retry_expires_at_ns, Some(500));
    assert_eq!(pending.unreadable_expires_at_ns, Some(900));
    // A new coordinator has no in-flight exclusions and can recover the row.
    assert_eq!(db.ready_welcomes_bounded(0, 1, 12, &[])?[0].sequence_id, 10);
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_read_budget_preserves_the_due_prefix_and_pending_state() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let first = topic(1, NetworkEntityKind::Welcome);
    let second = topic(2, NetworkEntityKind::Welcome);
    let mut envelopes = batch(&[10, 30]);
    envelopes[1].envelope = vec![2; 12];
    db.admit_ordered_batch(&first, Cursor(0), &envelopes, limits())?;
    db.admit_ordered_batch(&second, Cursor(0), &batch(&[10, 40]), limits())?;
    let ready = db.ready_welcomes_bounded(0, 8, 8, &[])?;
    assert_eq!(
        ready
            .iter()
            .map(|row| (row.entity_id.clone(), row.sequence_id))
            .collect::<Vec<_>>(),
        vec![
            (first.entity_id.clone(), 10),
            (second.entity_id.clone(), 10)
        ]
    );
    assert_eq!(ready.iter().map(|row| row.envelope.len()).sum::<usize>(), 8);
    assert_eq!(db.ready_welcomes_bounded(0, 1, 32, &[])?.len(), 1);
    assert!(db.ready_welcomes_bounded(0, 0, 0, &[])?.is_empty());
    db.complete_pending_envelope(&first, Cursor(10))?;
    db.complete_pending_envelope(&second, Cursor(10))?;
    let before = db.topic_progress(&first)?;
    assert!(matches!(
        db.ready_welcomes_bounded(0, 8, 8, &[]),
        Err(StorageError::Stream(StreamStorageError::Capacity {
            scope: BudgetScope::Batch
        }))
    ));
    assert_eq!(db.topic_progress(&first)?, before);
    assert_eq!(db.first_pending_envelope(&first)?.unwrap().sequence_id, 30);
    let ready = db.ready_welcomes_bounded(0, 8, 16, &[])?;
    assert_eq!(
        ready.iter().map(|row| row.sequence_id).collect::<Vec<_>>(),
        vec![30, 40]
    );
    assert_eq!(
        ready.iter().map(|row| row.envelope.len()).sum::<usize>(),
        16
    );
}

// verifies: PROC-008
#[xmtp_common::test(unwrap_try = true)]
async fn outer_state_rollback_restores_pending_rows_and_processed_progress() {
    let store = TestDb::create_persistent_store(None).await;
    let topic = topic(1, NetworkEntityKind::Identity);
    let db = store.db();
    db.admit_ordered_batch(&topic, Cursor(0), &batch(&[10, 50]), limits())?;
    let key_store = SqlKeyStore::new(store.conn());
    let result: TransactionOutcome<()> = key_store.transaction::<_, StorageError, _>(|conn| {
        let provider = conn.key_store();
        provider
            .db()
            .complete_pending_envelope(&topic, Cursor(10))?;
        Ok(TransactionOutcome::Rollback)
    })?;
    assert!(matches!(result, TransactionOutcome::Rollback));
    assert_eq!(db.topic_progress(&topic)?.processed, Cursor(0));
    assert_eq!(db.first_pending_envelope(&topic)?.unwrap().sequence_id, 10);
}

// verifies: PROC-010
#[xmtp_common::test(unwrap_try = true)]
async fn validated_join_anchor_preserves_the_received_tail() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let group = GroupId::from([7; 16]);
    let topic = StreamTopic::group(group);
    db.admit_ordered_batch(&topic, Cursor(0), &batch(&[10, 30, 50]), limits())?;
    db.install_group_anchor(group, Cursor(30), JoinAnchorMode::Advance)?;
    assert_eq!(
        db.topic_progress(&topic)?,
        TopicProgress {
            processed: Cursor(30),
            received: Cursor(50)
        }
    );
    assert_eq!(db.first_pending_envelope(&topic)?.unwrap().sequence_id, 50);
    assert!(matches!(
        db.install_group_anchor(group, Cursor(30), JoinAnchorMode::Advance),
        Err(StorageError::Stream(StreamStorageError::StaleJoinAnchor))
    ));
    db.install_group_anchor(group, Cursor(30), JoinAnchorMode::InactiveReadd)?;
    assert_eq!(
        db.topic_progress(&topic)?,
        TopicProgress {
            processed: Cursor(30),
            received: Cursor(50)
        }
    );
    assert_eq!(db.first_pending_envelope(&topic)?.unwrap().sequence_id, 50);
    for mode in [JoinAnchorMode::Advance, JoinAnchorMode::InactiveReadd] {
        assert!(matches!(
            db.install_group_anchor(group, Cursor(20), mode),
            Err(StorageError::Stream(StreamStorageError::StaleJoinAnchor))
        ));
    }
    assert!(matches!(
        db.install_group_anchor(group, Cursor(50), JoinAnchorMode::InactiveReadd),
        Err(StorageError::Stream(StreamStorageError::StaleJoinAnchor))
    ));
    let fresh_group = GroupId::from([8; 16]);
    assert!(matches!(
        db.install_group_anchor(fresh_group, Cursor(0), JoinAnchorMode::InactiveReadd),
        Err(StorageError::Stream(StreamStorageError::StaleJoinAnchor))
    ));
    db.install_group_anchor(fresh_group, Cursor(0), JoinAnchorMode::Advance)?;
}

// verifies: PROC-002
#[xmtp_common::test(unwrap_try = true)]
async fn independent_database_handles_resume_durable_receipt() {
    let path = xmtp_common::tmp_path();
    let first = TestDb::create_persistent_store(Some(path.clone())).await;
    let second = TestDb::create_persistent_store(Some(path.clone())).await;
    let group = topic(1, NetworkEntityKind::Group);
    first
        .db()
        .admit_ordered_batch(&group, Cursor(0), &batch(&[10, 40]), limits())?;
    let admitted =
        second
            .db()
            .admit_ordered_batch(&group, Cursor(0), &batch(&[10, 40, 80]), limits())?;
    assert_eq!(admitted.inserted, 1);
    drop(first);
    drop(second);
    let reopened = TestDb::create_persistent_store(Some(path)).await;
    assert_eq!(reopened.db().topic_progress(&group)?.received, Cursor(80));
    assert_eq!(
        reopened
            .db()
            .pending_topic_usage(NetworkEntityKind::Group)?[0]
            .rows,
        3
    );
}

// verifies: PROC-008, PROC-011
#[xmtp_common::test(unwrap_try = true)]
async fn terminal_rejection_is_bounded_and_rolls_back_with_processed_progress() {
    let store = TestDb::create_persistent_store(None).await;
    let db = store.db();
    let topic = topic(1, NetworkEntityKind::Group);
    db.admit_ordered_batch(&topic, Cursor(0), &batch(&[10, 20]), limits())?;
    assert!(
        db.record_terminal_rejection(&topic, Cursor(20), "TEST_BAD")
            .is_err()
    );
    let key_store = SqlKeyStore::new(store.conn());
    let result: TransactionOutcome<()> = key_store.transaction::<_, StorageError, _>(|conn| {
        let provider = conn.key_store();
        provider
            .db()
            .record_terminal_rejection(&topic, Cursor(10), "TEST_BAD")?;
        provider
            .db()
            .complete_pending_envelope(&topic, Cursor(10))?;
        Ok(TransactionOutcome::Rollback)
    })?;
    assert!(matches!(result, TransactionOutcome::Rollback));
    assert!(db.read_last_rejection(&topic)?.is_none());
    assert_eq!(db.topic_progress(&topic)?.processed, Cursor(0));
    db.record_terminal_rejection(&topic, Cursor(10), "TEST_BAD")?;
    db.complete_pending_envelope(&topic, Cursor(10))?;
    db.record_terminal_rejection(&topic, Cursor(20), "TEST_NEW")?;
    db.complete_pending_envelope(&topic, Cursor(20))?;
    let rejection = db.read_last_rejection(&topic)?.unwrap();
    assert_eq!(rejection.sequence_id, Cursor(20));
    assert_eq!(rejection.code, "TEST_NEW");
}
