//! Live callback tests against the self-hosted backend.

use std::sync::Arc;
use std::time::Duration;

use xmtp_common::StreamHandle;

use crate::Client;
use crate::context::XmtpSharedContext;
use crate::subscriptions::router_callbacks::{
    resume_bidi_streams, shared_transport, shared_transport_count,
    stream_conversation_messages_with_callback_dispatch, suspend_bidi_streams,
};
use crate::tester;
use crate::utils::MlsGroupExt;

const WAIT: Duration = Duration::from_secs(20);

/// The reflex headline: a conversation joined AFTER subscribing reaches the
/// live stream without a re-subscribe — its welcome arrives over the leased
/// welcome topic and the reflex leases the new group's topic on the same
/// wire. The message is sent before the reflex could possibly have leased,
/// so delivery also proves the cursored add replays it (catch-up ==
/// subscribe).
#[xmtp_common::test(unwrap_try = true)]
async fn welcomed_group_joins_the_live_stream() {
    tester!(alix);
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    group.send_msg(b"through the reflex").await;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the reflex-subscribed delivery")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"through the reflex");
}

/// A group this client creates itself streams its messages — no welcome
/// ever arrives for it, so delivery proves the `LocalEvents::NewGroup`
/// fan-in leased its topic.
#[xmtp_common::test(unwrap_try = true)]
async fn self_created_group_streams_its_messages() {
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    let group = bo.create_group(None, None)?;
    group.send_msg(b"own group, own stream").await;

    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the local-group delivery")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"own group, own stream");
}

/// A conversation this client creates itself surfaces on its own
/// conversations stream (legacy multiplexes `LocalEvents::NewGroup`; the
/// bidi stream must too — the creator never receives a welcome).
#[xmtp_common::test(unwrap_try = true)]
async fn self_created_conversation_surfaces_on_the_stream() {
    tester!(bo);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_conversations_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        false,
        move |conversation| {
            let _ = tx.send(conversation);
        },
        || {},
    );
    handle.wait_for_ready().await;

    let group = bo.create_group(None, None)?;

    let conversation = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the local conversation")
        .expect("callback channel closed")?;
    assert_eq!(conversation.group_id, group.group_id);
}

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
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
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
    let mut handle = Client::stream_conversations_with_callback_dispatch(
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
    tester!(caro, api_client: bo.context.api().api_client.clone());
    let before = shared_transport_count();

    let bo_group = alix.create_group(None, None)?;
    bo_group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&bo_group.group_id)?.sync().await?;
    let caro_group = alix.create_group(None, None)?;
    caro_group.invite(&caro).await?;
    caro.sync_welcomes().await?;
    caro.group(&caro_group.group_id)?.sync().await?;

    let (bo_tx, mut bo_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut bo_handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = bo_tx.send(message);
        },
        || {},
    );
    let (caro_tx, mut caro_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut caro_handle = Client::stream_all_messages_with_callback_dispatch(
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
    assert_eq!(shared_transport_count(), before + 1);

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
    let mut handle = stream_conversation_messages_with_callback_dispatch(
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
/// by the resume wave and reaches the callback. Resume is fire-and-forget,
/// so the replay arrives behind the call — the awaited channel reads below
/// are the arrival signal, not resume() resolving.
#[xmtp_common::test(unwrap_try = true)]
async fn suspend_resume_replays_what_was_missed() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
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

/// The launched-into-background case: `suspend_bidi_streams()` lands before
/// any stream exists — no transport yet, only the intent is recorded — so
/// the first stream's wire opens parked instead of live. A message sent
/// while parked arrives only after `resume_bidi_streams()`. (Process-per-
/// test keeps the recorded intent from leaking anywhere.)
#[xmtp_common::test(unwrap_try = true)]
async fn suspend_before_the_first_stream_parks_the_wire() {
    suspend_bidi_streams().await?;

    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?.sync().await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        None,
        move |message| {
            let _ = tx.send(message);
        },
        || {},
    );
    handle.wait_for_ready().await;

    // The wire is parked: nothing may deliver, no matter how long we wait.
    group.send_msg(b"sent before resume").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        rx.try_recv().is_err(),
        "a wire born suspended must not deliver before resume"
    );

    // Resume opens the wire; the parked lease catches up from its cursors
    // and the missed message arrives.
    resume_bidi_streams().await?;
    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the post-resume delivery")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"sent before resume");
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
    // End resumed: a trailing suspend would leave the recorded intent set,
    // making any later first wire in this process park (harmless under
    // process-per-test, a trap under any single-process runner).
    resume_bidi_streams().await?;
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
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
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
                // Lagged is recoverable — keep draining for the nudge.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
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
    let mut handle = Client::stream_all_messages_with_callback_dispatch(
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

/// A bidi api that never opens — just enough identity to key a transport.
#[derive(Clone)]
struct FixedHostApi(&'static str);

#[xmtp_common::async_trait]
impl xmtp_proto::api_client::XmtpMlsBidiStreams for FixedHostApi {
    type SubscribeStream = futures::stream::BoxStream<
        'static,
        std::result::Result<
            xmtp_proto::backend_v1::SubscribeResponse,
            xmtp_proto::api::ApiClientError,
        >,
    >;
    type Error = xmtp_proto::api::ApiClientError;

    fn host(&self) -> &str {
        self.0
    }

    async fn subscribe_bidi(
        &self,
        _requests: futures::stream::BoxStream<'static, xmtp_proto::backend_v1::SubscribeRequest>,
    ) -> std::result::Result<Self::SubscribeStream, Self::Error> {
        Err(xmtp_proto::api::ApiClientError::OtherUnretryable(
            "this test api never opens".into(),
        ))
    }
}

/// Transports key by dialed URL: a second client to the same host shares
/// the wire, a different host — the same backend behind a proxy, say — gets
/// its own. (nextest's process-per-test model keeps the count clean.)
#[xmtp_common::test]
async fn transports_key_by_destination() {
    let before = shared_transport_count();
    let api = Arc::new(FixedHostApi("test://backend-a"));
    let _a = shared_transport(api.clone());
    let _a_again = shared_transport(api);
    assert_eq!(
        shared_transport_count(),
        before + 1,
        "the same host shares one transport"
    );
    let _b = shared_transport(Arc::new(FixedHostApi("test://backend-b")));
    assert_eq!(
        shared_transport_count(),
        before + 2,
        "a different host gets its own"
    );
}

/// Separate API clients at one host keep separate authentication and transport state.
#[xmtp_common::test(unwrap_try = true)]
async fn separate_api_clients_at_one_host_use_separate_wires() {
    tester!(alix);
    tester!(bo);
    let before = shared_transport_count();
    let mut alix_handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(alix.client.clone()),
        None,
        None,
        |_| {},
        || {},
    );
    let mut bo_handle = Client::stream_all_messages_with_callback_dispatch(
        Arc::new(bo.client.clone()),
        None,
        None,
        |_| {},
        || {},
    );
    alix_handle.wait_for_ready().await;
    bo_handle.wait_for_ready().await;
    assert_eq!(shared_transport_count(), before + 2);
}
