//! Local delivery tests: ownership and acks.

use super::*;

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn iterator_drop_retains_the_last_item_until_a_later_next_request() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_callback_releases_owner_without_consuming_its_item() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(create().is_err());
    item.acknowledgement.reject();
    let mut replacement = create()?;
    assert_eq!(
        replacement.next_delivery().await?.unwrap().message.id,
        message.id
    );
    assert!(matches!(
        reader.next_delivery().await,
        Err(LocalDeliveryError::AcknowledgementRejected)
    ));
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn cancelling_a_pending_next_does_not_bypass_explicit_acknowledgement() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(reader.next_delivery().now_or_never().is_none());
    assert!(reader.next_delivery().now_or_never().is_none());
    item.acknowledgement.acknowledge()?;
    assert_eq!(reader.next_delivery().await?.unwrap().message.id, second.id);
}

// verifies: PROC-031
#[xmtp_common::test(unwrap_try = true)]
async fn stale_host_queue_token_cannot_dispatch_or_acknowledge() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    generate_stored_msg(Cursor(100), group.group_id).store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    alix.context.db().rotate_stream_database_id()?;
    assert!(item.acknowledgement.check_owner().is_err());
    assert!(item.acknowledgement.acknowledge().is_err());
    assert!(reader.next_delivery().await.is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn removed_and_readded_scope_discards_old_queued_tokens_without_acknowledging() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let old = reader.next_delivery().await?.unwrap();
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    reader
        .control()
        .update_scope(DeliveryScope::Groups(vec![group.group_id]));
    assert!(matches!(
        old.acknowledgement.check_owner(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    assert!(matches!(
        old.acknowledgement.acknowledge(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    drop(old);
    let selected_again = reader.next_delivery().await?.unwrap();
    assert_eq!(selected_again.message.id, message.id);
    selected_again.acknowledgement.check_owner()?;
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    // This callback already started. Removing its group cannot undo its successful return.
    selected_again.acknowledgement.acknowledge()?;
    assert!(
        alix.context
            .db()
            .default_delivery_messages(
                reader.session.owner().unwrap(),
                &DeliveryScope::All,
                now_ns(),
                8
            )?
            .is_empty()
    );
}

// verifies: PROC-031, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_ends_reader_and_only_a_new_stream_can_acquire_delivery() {
    use diesel::RunQueryDsl;
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    item.acknowledgement.check_owner()?;
    let old = reader.session.owner().unwrap();
    alix.context.db().raw_query(|conn| {
        diesel::sql_query("UPDATE user_preferences SET delivery_owner_until_ns = 0").execute(conn)
    })?;
    assert!(item.acknowledgement.acknowledge().is_err());
    assert!(reader.session.is_closed());
    let mut replacement = create()?;
    assert_ne!(replacement.session.owner(), Some(old));
    assert!(item.acknowledgement.check_owner().is_err());
    assert_eq!(
        replacement.next_delivery().await?.unwrap().message.id,
        message.id
    );
}

// verifies: PROC-031, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn failed_old_acknowledgement_cannot_release_a_competing_owner() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    generate_stored_msg(Cursor(100), group.group_id).store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    alix.context
        .db()
        .release_delivery_owner(reader.session.owner().unwrap())?;
    let other = alix.context.db().acquire_delivery_owner_with_clock(
        LocalDeliveryConfig::default().lease_duration_ns()?,
        now_ns,
    )?;
    assert!(item.acknowledgement.acknowledge().is_err());
    alix.context
        .db()
        .check_delivery_owner_with_clock(other, now_ns)?;
    assert!(
        !alix
            .context
            .db()
            .default_delivery_messages(other, &DeliveryScope::All, now_ns(), 1)?
            .is_empty()
    );
}

// verifies: PROC-034, PROC-052
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn replay_cancelled_ack_keeps_same_instance_position_and_item() {
    replay_ack_boundary(false).await?;
}

// verifies: PROC-034, PROC-052
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 4)]
async fn replay_admitted_ack_keeps_same_instance_progress_after_cancel() {
    replay_ack_boundary(true).await?;
}

#[cfg(not(target_arch = "wasm32"))]
async fn replay_ack_boundary(
    after_admission: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use xmtp_db::delivery::acknowledgement::{DeliveryAckPhase, DeliveryAckRequest};
    use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let from = alix.context.db().current_delivery_cursor()?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            Some(from),
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.expect("A handoff");
    assert_eq!(item.message.id, first.id);
    item.acknowledgement.check_owner()?;
    let before = alix
        .context
        .db()
        .get_last_cursor(group.group_id, EntityKind::Delivery)?;
    let request = Arc::new(DeliveryAckRequest::default());
    let (arrived_tx, arrived_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    request.set_observer(Arc::new(move |phase| {
        let target = if after_admission {
            DeliveryAckPhase::ReplayAdmitted
        } else {
            DeliveryAckPhase::ReplayBeforeAdmission
        };
        if phase == target {
            arrived_tx.send(()).unwrap();
            release_rx
                .lock()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
    }));
    let retained = Arc::new(item.acknowledgement);
    let ack = retained.clone();
    let acknowledging = request.clone();
    let worker = std::thread::spawn(move || ack.acknowledge_cancellable(&acknowledging));
    arrived_rx.recv_timeout(Duration::from_secs(10))?;
    request.cancel();
    release_tx.send(())?;
    let admitted = worker.join().expect("ACK thread")?;
    let next = reader.next_delivery().now_or_never();
    let position = reader.replay_position;
    println!(
        "REPLAY_CANCEL after_admission={after_admission} admitted={admitted} position_unchanged={} replacement_pending={}",
        position == Some(from),
        next.is_none()
    );
    assert_eq!(
        admitted, after_admission,
        "cancellation and replay ACK need one atomic order"
    );
    if after_admission {
        assert_eq!(
            next.expect("admitted replay ACK must release B")?
                .expect("B item")
                .message
                .id,
            second.id
        );
        assert_eq!(
            position,
            Some(item.cursor),
            "admitted ACK must advance only through A"
        );
    } else {
        assert_eq!(
            position,
            Some(from),
            "same-instance replay position must not advance"
        );
        assert!(
            matches!(
                *reader.pending.as_ref().expect("pending A").state.lock(),
                acknowledgement::AcknowledgementState::Dispatched
            ),
            "A must remain unacknowledged"
        );
        assert!(
            next.is_none(),
            "the same reader must not return B before an ACK retry"
        );
        assert!(retained.acknowledge_cancellable(&DeliveryAckRequest::default())?);
        assert_eq!(
            reader
                .next_delivery()
                .await?
                .expect("ACK retry B")
                .message
                .id,
            second.id
        );
        assert_eq!(reader.replay_position, Some(item.cursor));
    }
    reader.close();
    let mut restarted = create()?;
    assert_eq!(
        restarted
            .next_delivery()
            .await?
            .expect("restart A")
            .message
            .id,
        first.id
    );
    restarted.close();
    assert_eq!(
        alix.context
            .db()
            .get_last_cursor(group.group_id, EntityKind::Delivery)?,
        before,
        "replay must not change D"
    );
    Ok(())
}
