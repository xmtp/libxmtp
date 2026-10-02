use super::*;
use xmtp_common::StreamHandle;
use xmtp_db::ConnectionExt;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};

#[derive(Clone, Copy, Debug)]
enum CancelBoundary {
    BeforeCheck,
    BeforeAck,
    AfterTake,
    DuringWrite,
    AfterAck,
}

async fn cancel_at_boundary(boundary: CancelBoundary) -> Result<(), Box<dyn std::error::Error>> {
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
        CancelBoundary::AfterAck => gates.after_ack = Some(gate.clone()),
    }
    *reader.request_gates.lock() = gates;
    let reading = reader.clone();
    let next = xmtp_common::spawn(None, async move { reading.next().await });
    let sync_gate = matches!(
        boundary,
        CancelBoundary::AfterTake | CancelBoundary::DuringWrite
    );
    let arrived = if sync_gate {
        &admitted.arrived
    } else {
        &gate.arrived
    };
    xmtp_common::time::timeout(Duration::from_secs(10), arrived.notified()).await?;
    let token = reader.request_cancel.lock().clone().expect("request token");
    let write_lock = reader
        .request_ack_admission
        .lock()
        .clone()
        .expect("request write lock");
    assert_eq!(
        write_lock.try_lock().is_none(),
        matches!(boundary, CancelBoundary::DuringWrite),
        "only the synchronous write can hold the cancellation lock"
    );
    let at_gate = client
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
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
    if matches!(boundary, CancelBoundary::DuringWrite) {
        assert!(
            drop_elapsed < Duration::from_secs(5),
            "drop waited for the held write"
        );
    }
    eprintln!(
        "ACK_CANCEL {boundary:?} token_received=true drop_ms={} D_before={before} D_gate={at_gate} A={first_sequence}",
        drop_elapsed.as_millis()
    );
    if sync_gate {
        *admitted.released.lock() = true;
        admitted.wake.notify_one();
    } else {
        gate.release.notify_one();
    }
    xmtp_common::time::timeout(Duration::from_secs(10), settled.notified()).await?;
    // End only after the worker settles, so end cannot suppress its ACK.
    xmtp_common::time::timeout(Duration::from_secs(10), reader.end()).await??;
    let after = client
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
    let second_id = group
        .send_text("after cancellation settled".into(), None)
        .await?;
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
        CancelBoundary::DuringWrite | CancelBoundary::AfterAck
    );
    let expected_cursor = if ack_won { first_sequence } else { before };
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
    assert_eq!(reopen_cursor, expected_cursor, "cursor after store reopen");
    assert_eq!(
        replayed_id,
        if ack_won { second_id } else { first_id },
        "cancellation before ACK must replay the prior handoff"
    );
    Ok(())
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_before_initial_check_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::BeforeCheck).await?;
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_before_ack_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::BeforeAck).await?;
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_subsequent_read_after_ack_keeps_committed_cursor() {
    cancel_at_boundary(CancelBoundary::AfterAck).await?;
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_after_take_replays_prior_item() {
    cancel_at_boundary(CancelBoundary::AfterTake).await?;
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_subsequent_read_during_write_drops_without_waiting() {
    cancel_at_boundary(CancelBoundary::DuringWrite).await?;
}

// verifies: PROC-028, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn failed_subsequent_ack_write_is_typed_and_replays_prior_item() {
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
    client.inner.context.db().raw_query(|conn| {
        sql_query(
            "CREATE TRIGGER fail_delivery_ack BEFORE INSERT ON refresh_state \
             WHEN NEW.entity_kind = 10 BEGIN SELECT RAISE(FAIL, 'forced ACK write error'); END",
        )
        .execute(conn)
    })?;
    let result = xmtp_common::time::timeout(Duration::from_secs(10), reader.next()).await?;
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
}
