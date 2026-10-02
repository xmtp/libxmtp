use super::*;
use xmtp_common::StreamHandle;
use xmtp_db::delivery::acknowledgement::DeliveryAckPhase;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
use xmtp_db::{ConnectionExt, XmtpDb};

#[derive(Clone, Copy, Debug)]
enum CancelBoundary {
    BeforeCheck,
    BeforeAck,
    AfterTake,
    DuringWrite,
    WaitingWriter,
    WriterAcquired,
    TentativeUpdate,
    CommitAdmitted,
    AfterAck,
}

async fn cancel_at_boundary(boundary: CancelBoundary) -> Result<(), Box<dyn std::error::Error>> {
    cancel_at_boundary_impl(boundary, false).await
}

async fn cancel_at_boundary_impl(
    boundary: CancelBoundary,
    resume: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root("reader-ack-cancel");
    std::fs::create_dir_all(&root)?;
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("client.db3"));
    let client = Client::create(signer.clone(), settings.clone()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let group_id = group.id();
    let first_id = group.send_text("first handoff".into(), None).await?;
    let reader = group.message_reader(None).await?;
    let first = xmtp_common::time::timeout(Duration::from_secs(10), reader.next())
        .await??
        .expect("first handoff");
    assert_eq!(first.0.id, first_id);
    let first_sequence = crate::delivery::cursor::parse(
        first.0.delivery_cursor.as_deref().expect("delivery cursor"),
    )?
    .delivery_sequence;
    let before = client
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
    assert!(before < first_sequence);
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    let admitted = Arc::new(reader::AckAdmissionGate {
        arrived: Notify::new(),
        released: parking_lot::Mutex::new(false),
        wake: parking_lot::Condvar::new(),
    });
    // A separate connection can read committed D while the ACK writer is held.
    let durable = xmtp_db::database::native::NativeDb::builder()
        .persistent(root.join("client.db3").to_string_lossy().into_owned())
        .single_connection()
        .build_unencrypted()?;
    let writer_database = if matches!(boundary, CancelBoundary::WaitingWriter) {
        Some(
            xmtp_db::database::native::NativeDb::builder()
                .persistent(root.join("client.db3").to_string_lossy().into_owned())
                .single_connection()
                .build_unencrypted()?,
        )
    } else {
        None
    };
    let writer_gate = Arc::new(reader::AckAdmissionGate {
        arrived: Notify::new(),
        released: parking_lot::Mutex::new(false),
        wake: parking_lot::Condvar::new(),
    });
    let settled = Arc::new(Notify::new());
    let mut gates = reader::RequestGates {
        settled: Some(settled.clone()),
        ..Default::default()
    };
    match boundary {
        CancelBoundary::BeforeCheck => gates.before_check = Some(gate.clone()),
        CancelBoundary::BeforeAck => gates.before_ack = Some(gate.clone()),
        CancelBoundary::AfterTake => gates.after_take = Some(admitted.clone()),
        CancelBoundary::DuringWrite => gates.during_write = Some(admitted.clone()),
        CancelBoundary::WaitingWriter => {
            let waiting = gate.clone();
            let before_writer = admitted.clone();
            gates.ack_observer = Some(Arc::new(move |phase| {
                if phase == DeliveryAckPhase::BeforeWriter {
                    before_writer.wait();
                    waiting.arrived.notify_one();
                }
            }));
        }
        CancelBoundary::WriterAcquired
        | CancelBoundary::TentativeUpdate
        | CancelBoundary::CommitAdmitted => {
            let held = admitted.clone();
            gates.ack_observer = Some(Arc::new(move |phase| {
                let target = match boundary {
                    CancelBoundary::WriterAcquired => DeliveryAckPhase::WriterAcquired,
                    CancelBoundary::TentativeUpdate => DeliveryAckPhase::TentativeUpdate,
                    CancelBoundary::CommitAdmitted => DeliveryAckPhase::CommitAdmitted,
                    _ => unreachable!(),
                };
                if phase == target {
                    held.wait();
                }
            }));
        }
        CancelBoundary::AfterAck => gates.after_ack = Some(gate.clone()),
    }
    *reader.request_gates.lock() = gates;
    let reading = reader.clone();
    let next = xmtp_common::spawn(None, async move { reading.next().await });
    let sync_gate = matches!(
        boundary,
        CancelBoundary::AfterTake
            | CancelBoundary::DuringWrite
            | CancelBoundary::WaitingWriter
            | CancelBoundary::WriterAcquired
            | CancelBoundary::TentativeUpdate
            | CancelBoundary::CommitAdmitted
    );
    let arrived = if sync_gate {
        &admitted.arrived
    } else {
        &gate.arrived
    };
    xmtp_common::time::timeout(Duration::from_secs(10), arrived.notified()).await?;
    let writer = if let Some(database) = &writer_database {
        let db = database.db();
        let held = writer_gate.clone();
        let writer = std::thread::spawn(move || {
            use xmtp_db::diesel::connection::SimpleConnection;
            db.raw_query(|conn| {
                conn.batch_execute("BEGIN IMMEDIATE")?;
                held.wait();
                conn.batch_execute("ROLLBACK")
            })
        });
        xmtp_common::time::timeout(Duration::from_secs(10), writer_gate.arrived.notified()).await?;
        *admitted.released.lock() = true;
        admitted.wake.notify_one();
        xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
        Some(writer)
    } else {
        None
    };
    let token = reader.request_cancel.lock().clone().expect("request token");
    let admission = reader
        .request_ack_admission
        .lock()
        .clone()
        .expect("request admission state");
    assert!(!admission.is_cancelled());
    let at_gate = durable
        .db()
        .get_refresh_state(&group.inner.group_id, EntityKind::Delivery)?
        .map_or(0, |state| state.sequence_id as u64);
    if matches!(boundary, CancelBoundary::DuringWrite) {
        // Wake the gate without releasing it. It must keep waiting.
        xmtp_common::time::timeout(Duration::from_secs(10), async {
            while !admitted.wake.notify_one() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
    }
    let drop_started = xmtp_common::time::Instant::now();
    next.end();
    assert!(matches!(
        xmtp_common::time::timeout(Duration::from_secs(10), next.join()).await?,
        Err(xmtp_common::StreamHandleError::JoinHandleError(ref error)) if error.is_cancelled()
    ));
    xmtp_common::time::timeout(Duration::from_secs(10), token.cancelled()).await?;
    assert!(
        token.is_cancelled(),
        "outer future drop cancelled the token"
    );
    let drop_elapsed = drop_started.elapsed();
    assert!(
        drop_elapsed < Duration::from_secs(5),
        "drop waited for storage"
    );
    let admission_cancelled = admission.is_cancelled();
    eprintln!(
        "ACK_CANCEL {boundary:?} token_received=true drop_ms={} D_before={before} D_gate={at_gate} A={first_sequence}",
        drop_elapsed.as_millis()
    );
    if let Some(writer) = writer {
        *writer_gate.released.lock() = true;
        writer_gate.wake.notify_one();
        writer.join().expect("database writer thread")?;
    }
    if sync_gate {
        *admitted.released.lock() = true;
        admitted.wake.notify_one();
    } else {
        gate.release.notify_one();
    }
    xmtp_common::time::timeout(Duration::from_secs(10), settled.notified()).await?;
    let after = client
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
    let second_id = group
        .send_text("after cancellation settled".into(), None)
        .await?;
    let resumed_cursor = if resume {
        assert_eq!(after, before, "cancelled tentative update was rolled back");
        let second = xmtp_common::time::timeout(Duration::from_secs(10), reader.next())
            .await??
            .expect("same-reader retry");
        assert_eq!(second.0.id, second_id);
        let cursor = durable
            .db()
            .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
            .0;
        assert_eq!(
            cursor, first_sequence,
            "retry must persist the retained prior ACK"
        );
        cursor
    } else {
        after
    };
    // End only after the worker settles, so end cannot suppress its ACK.
    xmtp_common::time::timeout(Duration::from_secs(10), reader.end()).await??;
    client.end().await?;
    drop((reader, group, client, durable, writer_database));
    let reopened = Client::create(signer, settings).await?;
    let crate::Conversation::Group { group } = reopened
        .conversations()
        .get_by_id(group_id)
        .await?
        .expect("stored group")
    else {
        panic!("group")
    };
    let reopen_cursor = reopened
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
    let reader = group.message_reader(None).await?;
    let replayed = xmtp_common::time::timeout(Duration::from_secs(10), reader.next())
        .await??
        .expect("durable replay");
    let replayed_id = replayed.0.id;
    reader.end().await?;
    reopened.end().await?;
    drop((reader, group, reopened));
    std::fs::remove_dir_all(root)?;
    eprintln!(
        "ACK_CANCEL {boundary:?} worker_settled=true D_after={after} D_reopen={reopen_cursor} replay_A={} replay_B={}",
        replayed_id == first_id,
        replayed_id == second_id
    );
    let ack_won = matches!(
        boundary,
        CancelBoundary::CommitAdmitted | CancelBoundary::AfterAck
    );
    let expected_cursor = if ack_won { first_sequence } else { before };
    assert_eq!(
        admission_cancelled, !ack_won,
        "final atomic admission state"
    );
    assert_eq!(
        at_gate,
        if matches!(boundary, CancelBoundary::AfterAck) {
            first_sequence
        } else {
            before
        },
        "cursor at held boundary"
    );
    assert_eq!(after, expected_cursor, "cursor after worker settlement");
    assert_eq!(reopen_cursor, resumed_cursor, "cursor after store reopen");
    assert_eq!(
        replayed_id,
        if ack_won || resume {
            second_id
        } else {
            first_id
        },
        "cancellation before ACK must replay the prior handoff"
    );
    Ok(())
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_before_initial_check_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::BeforeCheck).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_before_ack_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::BeforeAck).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_after_ack_keeps_committed_cursor() {
    cancel_at_boundary(CancelBoundary::AfterAck).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_after_take_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::AfterTake).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_before_storage_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::DuringWrite).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_waiting_for_writer_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::WaitingWriter).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_after_writer_acquisition_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::WriterAcquired).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_after_tentative_update_rolls_back_cursor() {
    cancel_at_boundary(CancelBoundary::TentativeUpdate).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_after_commit_admission_keeps_cursor() {
    cancel_at_boundary(CancelBoundary::CommitAdmitted).await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_tentative_ack_is_retained_for_same_reader_retry() {
    cancel_at_boundary_impl(CancelBoundary::TentativeUpdate, true).await?;
}

// verifies: PROC-052, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn failed_subsequent_ack_write_is_typed_and_replays_prior_item() {
    failed_ack(false).await?;
}

// verifies: PROC-052, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn failed_subsequent_ack_commit_is_typed_and_replays_prior_item() {
    failed_ack(true).await?;
}

async fn failed_ack(at_commit: bool) -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_db::diesel::{RunQueryDsl, sql_query};
    let root = temp_root("reader-ack-write-error");
    std::fs::create_dir_all(&root)?;
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("client.db3"));
    let client = Client::create(signer.clone(), settings.clone()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let group_id = group.id();
    let first_id = group
        .send_text("unacknowledged handoff".into(), None)
        .await?;
    let reader = group.message_reader(None).await?;
    assert_eq!(reader.next().await?.expect("first handoff").0.id, first_id);
    group
        .send_text("must wait for the prior ACK".into(), None)
        .await?;
    let admitted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if at_commit {
        let observed = admitted.clone();
        reader.request_gates.lock().ack_observer = Some(Arc::new(move |phase| {
            if phase == DeliveryAckPhase::CommitAdmitted {
                observed.store(true, std::sync::atomic::Ordering::Release);
            }
        }));
    }
    client.inner.context.db().raw_query(|conn| {
        use xmtp_db::diesel::connection::SimpleConnection;
        if at_commit {
            conn.batch_execute(
                "CREATE TABLE ack_commit_parent (id INTEGER PRIMARY KEY); \
                 CREATE TABLE ack_commit_child (id INTEGER PRIMARY KEY, parent INTEGER \
                 REFERENCES ack_commit_parent(id) DEFERRABLE INITIALLY DEFERRED); \
                 CREATE TRIGGER fail_delivery_ack AFTER INSERT ON refresh_state \
                 WHEN NEW.entity_kind = 10 BEGIN INSERT INTO ack_commit_child VALUES (1, 1); END",
            )
        } else {
            conn.batch_execute(
                "CREATE TRIGGER fail_delivery_ack BEFORE INSERT ON refresh_state \
                 WHEN NEW.entity_kind = 10 BEGIN SELECT RAISE(FAIL, 'forced ACK write error'); END",
            )
        }
    })?;
    let result = xmtp_common::time::timeout(Duration::from_secs(10), reader.next()).await?;
    assert_eq!(
        admitted.load(std::sync::atomic::Ordering::Acquire),
        at_commit,
        "deferred constraint fails at commit, after final admission"
    );
    client
        .inner
        .context
        .db()
        .raw_query(|conn| sql_query("DROP TRIGGER fail_delivery_ack").execute(conn))?;
    assert!(
        matches!(result, Err(crate::XmtpError::Storage(ref details)) if details.code == "Storage" && matches!(details.category, crate::ErrorCategory::Storage)),
        "ACK failure must keep its typed storage cause: {result:?}"
    );
    assert!(reader.is_ended_for_test());
    assert!(reader.next().await?.is_none());
    assert_eq!(
        client
            .inner
            .context
            .db()
            .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
            .0,
        0
    );
    reader.end().await?;
    client.end().await?;
    drop((reader, group, client));
    let reopened = Client::create(signer, settings).await?;
    let crate::Conversation::Group { group } = reopened
        .conversations()
        .get_by_id(group_id)
        .await?
        .expect("stored group")
    else {
        panic!("group")
    };
    let reader = group.message_reader(None).await?;
    assert_eq!(reader.next().await?.expect("durable replay").0.id, first_id);
    reader.end().await?;
    reopened.end().await?;
    drop((reader, group, reopened));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
