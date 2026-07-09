//! Live integration tests for the bidi callback adapters over a real v3
//! backend (docker node). Native-only, v3-only — see the module gate.

use std::sync::Arc;
use std::time::Duration;

use xmtp_common::StreamHandle;

use crate::Client;
use crate::subscriptions::router_callbacks::{
    resume_bidi_streams, stream_conversation_messages_with_callback_bidi, suspend_bidi_streams,
};
use crate::tester;
use crate::utils::MlsGroupExt;

const WAIT: Duration = Duration::from_secs(20);

/// A message sent after subscribing arrives decoded through the callback.
#[xmtp_common::test(unwrap_try = true)]
async fn callback_stream_delivers_live_messages() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    group.send_msg(b"over the bidi pump").await;
    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the callback")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"over the bidi pump");
}

/// A new conversation surfaces on the conversations callback.
#[xmtp_common::test(unwrap_try = true)]
async fn callback_stream_surfaces_new_conversations() {
    tester!(alix);
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_conversations_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        false,
        move |conversation| {
            let _ = tx.send(conversation);
        },
        || {},
    );
    handle.wait_for_ready().await;

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;

    let conversation = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the conversation callback")
        .expect("callback channel closed")?;
    assert_eq!(conversation.group_id, group.group_id);
}

/// Two clients in one process ride the shared transport: each callback
/// stream still receives exactly its own client's traffic.
#[xmtp_common::test(unwrap_try = true)]
async fn sibling_clients_share_the_process_transport() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let bo_group = alix.create_group(None, None)?;
    bo_group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&bo_group.group_id)?.sync().await?;
    let caro_group = alix.create_group(None, None)?;
    caro_group.invite(&caro).await?;
    caro.sync_welcomes().await?;
    caro.group(&caro_group.group_id)?.sync().await?;

    let (bo_tx, mut bo_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut bo_handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = bo_tx.send(message);
        },
        || {},
    );
    let (caro_tx, mut caro_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut caro_handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(caro.client.clone()),
        None,
        None,
        move |message| {
            let _ = caro_tx.send(message);
        },
        || {},
    );
    bo_handle.wait_for_ready().await;
    caro_handle.wait_for_ready().await;

    bo_group.send_msg(b"for bo").await;
    caro_group.send_msg(b"for caro").await;

    let to_bo = tokio::time::timeout(WAIT, bo_rx.recv())
        .await
        .expect("timed out waiting for bo's callback")
        .expect("bo callback channel closed")?;
    assert_eq!(to_bo.decrypted_message_bytes, b"for bo");
    let to_caro = tokio::time::timeout(WAIT, caro_rx.recv())
        .await
        .expect("timed out waiting for caro's callback")
        .expect("caro callback channel closed")?;
    assert_eq!(to_caro.decrypted_message_bytes, b"for caro");
}

/// A single-conversation callback stream (the context-based, ephemeral-router
/// path) delivers that conversation's messages and only those.
#[xmtp_common::test(unwrap_try = true)]
async fn single_conversation_callback_is_scoped_to_its_group() {
    tester!(alix);
    tester!(bo);

    let streamed = alix.create_group(None, None)?;
    streamed.invite(&bo).await?;
    let other = alix.create_group(None, None)?;
    other.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&streamed.group_id)?.sync().await?;
    bo.group(&other.group_id)?.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = stream_conversation_messages_with_callback_bidi(
        bo.client.context.clone(),
        bo.group(&streamed.group_id)?.group_id,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    // The sibling group's message must not leak into this stream; sent first
    // so a leak would arrive ahead of the expected message.
    other.send_msg(b"for the other stream").await;
    streamed.send_msg(b"for this stream").await;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the callback")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"for this stream");
}

/// The app-lifecycle round trip: a message sent while suspended is replayed
/// by the resume wave and reaches the callback — resume() resolving is the
/// "catch up, then done" signal.
#[xmtp_common::test(unwrap_try = true)]
async fn suspend_resume_replays_what_was_missed() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    suspend_bidi_streams().await?;
    group.send_msg(b"sent while backgrounded").await;
    resume_bidi_streams().await?;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the replayed message")
        .expect("callback channel closed")?;
    assert_eq!(
        delivered.decrypted_message_bytes,
        b"sent while backgrounded"
    );

    // A second cycle: the resume positions must carry over, so the replay
    // brings exactly the newly-missed message, not history.
    suspend_bidi_streams().await?;
    group.send_msg(b"backgrounded again").await;
    resume_bidi_streams().await?;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the second replay")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"backgrounded again");
}

/// The lifecycle helpers are safe to call before anything ever streamed:
/// with no transport in the process they resolve as no-ops. (Real on
/// mobile — backgrounding can beat the first subscription.)
#[xmtp_common::test(unwrap_try = true)]
async fn lifecycle_helpers_are_noops_without_a_transport() {
    // Isolation note: this relies on nextest's process-per-test model — no
    // other test in this process can have initialized the shared transport.
    suspend_bidi_streams().await?;
    resume_bidi_streams().await?;
    suspend_bidi_streams().await?;
}

/// Sync-group traffic is intercepted, exactly like the legacy stream: it
/// nudges the device-sync worker instead of surfacing internal payloads as
/// conversation messages.
#[xmtp_common::test(unwrap_try = true)]
async fn sync_group_messages_are_intercepted_not_delivered() {
    use crate::context::XmtpSharedContext;
    use crate::subscriptions::SyncWorkerEvent;
    use xmtp_db::prelude::*;
    tester!(alix, sync_worker);

    // The device-sync worker creates the sync group in the background.
    let sync_group = xmtp_common::wait_for_some(|| async {
        alix.client.context.db().primary_sync_group().ok().flatten()
    })
    .await
    .expect("the sync worker creates a sync group");
    let group = alix.create_group(None, None)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(alix.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;
    let mut worker_events = alix.client.context.worker_events().subscribe();

    // Into the sync group first — a leak would arrive ahead of the normal
    // message below.
    alix.group(&sync_group.id)?
        .send_msg(b"internal sync payload")
        .await;
    group.send_msg(b"a normal message").await;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the callback")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"a normal message");

    // The intercepted message became a worker nudge instead.
    let nudged = tokio::time::timeout(WAIT, async {
        loop {
            match worker_events.recv().await {
                Ok(SyncWorkerEvent::NewSyncGroupMsg) => break,
                Ok(_) => continue,
                Err(e) => panic!("worker events channel closed: {e}"),
            }
        }
    })
    .await;
    assert!(nudged.is_ok(), "the sync worker must be nudged");
}

/// `stream_all_messages` on an account with no matching conversations stays
/// open instead of error-closing — the transport refuses an empty lease, and
/// that refusal must not surface to the caller.
#[xmtp_common::test(unwrap_try = true)]
async fn stream_all_with_no_conversations_stays_open() {
    use std::sync::atomic::{AtomicBool, Ordering};
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let closed = Arc::new(AtomicBool::new(false));
    let on_close = {
        let closed = closed.clone();
        move || closed.store(true, Ordering::SeqCst)
    };
    let mut handle = Client::stream_all_messages_with_callback_bidi(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        on_close,
    );
    handle.wait_for_ready().await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !closed.load(Ordering::SeqCst),
        "an empty subscription must stay open, not error-close"
    );
    assert!(rx.try_recv().is_err(), "nothing should have been delivered");
}
