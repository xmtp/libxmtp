use super::*;
use xmtp_common::StreamHandle;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};

#[derive(Clone, Copy, Debug)]
enum CancelBoundary {
    BeforeCheck,
    BeforeAck,
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
    let settled = Arc::new(Notify::new());
    let mut gates = reader::RequestGates {
        settled: Some(settled.clone()),
        ..Default::default()
    };
    match boundary {
        CancelBoundary::BeforeCheck => gates.before_check = Some(gate.clone()),
        CancelBoundary::BeforeAck => gates.before_ack = Some(gate.clone()),
        CancelBoundary::AfterAck => gates.after_ack = Some(gate.clone()),
    }
    *reader.request_gates.lock() = gates;
    let reading = reader.clone();
    let next = xmtp_common::spawn(None, async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    let token = reader.request_cancel.lock().clone().expect("request token");
    let at_gate = client
        .inner
        .context
        .db()
        .get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?
        .0;
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
    eprintln!(
        "ACK_CANCEL {boundary:?} token_received=true D_before={before} D_gate={at_gate} A={first_sequence}"
    );
    gate.release.notify_one();
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
    let ack_won = matches!(boundary, CancelBoundary::AfterAck);
    let expected_cursor = if ack_won { first_sequence } else { before };
    assert_eq!(at_gate, expected_cursor, "cursor at held boundary");
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
