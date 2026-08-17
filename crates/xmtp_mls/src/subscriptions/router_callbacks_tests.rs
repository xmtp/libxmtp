//! Live integration tests for the bidi callback adapters over a real v3
//! backend (docker node). Native-only, v3-only — see the module gate.

use std::sync::Arc;
use std::time::Duration;

use xmtp_common::StreamHandle;

use crate::Client;
use crate::subscriptions::router_callbacks::{
    StreamOrigin, bidi_streams_active, bidi_unsupported_latched, is_bidi_dead_end,
    is_bidi_unsupported, latch_bidi_unsupported, pump_stream, resume_bidi_streams,
    shared_transport, shared_transport_count, stream_conversation_messages_with_callback_bidi,
    suspend_bidi_streams,
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

    let group = alix.create_group(None, None).await?;
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

    let group = bo.create_group(None, None).await?;
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

    let group = bo.create_group(None, None).await?;

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

    let group = alix.create_group(None, None).await?;
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

    let group = alix.create_group(None, None).await?;
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

    let bo_group = alix.create_group(None, None).await?;
    bo_group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    bo.group(&bo_group.group_id)?.sync().await?;
    let caro_group = alix.create_group(None, None).await?;
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

    let streamed = alix.create_group(None, None).await?;
    streamed.invite(&bo).await?;
    let other = alix.create_group(None, None).await?;
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
/// by the resume wave and reaches the callback. Resume is fire-and-forget,
/// so the replay arrives behind the call — the awaited channel reads below
/// are the arrival signal, not resume() resolving.
#[xmtp_common::test(unwrap_try = true)]
async fn suspend_resume_replays_what_was_missed() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None).await?;
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
    let group = alix.create_group(None, None).await?;
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
        alix.client
            .context
            .db()
            .primary_sync_group()
            .await
            .ok()
            .flatten()
    })
    .await
    .expect("the sync worker creates a sync group");
    let group = alix.create_group(None, None).await?;

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
                // Lagged is recoverable — keep draining for the nudge.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("worker events channel closed: {e}"),
            }
        }
    })
    .await;
    assert!(nudged.is_ok(), "the sync worker must be nudged");
}

/// A decode failure as the wire really produces one (0xff is an invalid
/// wire type) — `prost::DecodeError` has no public constructor.
fn garbled_frame() -> prost::DecodeError {
    <xmtp_proto::mls_v1::GroupMessage as prost::Message>::decode(&[0xff][..])
        .expect_err("an invalid wire type must not decode")
}

/// The latch classifier. Latch-worthy is the backend's own refusal of the
/// surface, in each shape it really arrives in: a gRPC `UNIMPLEMENTED`
/// buried under the v3 client's blanket-retryable wrapping, the in-process
/// d14n/migration stub refusal (`OtherUnretryable`), and the tombstoned
/// shared transport (`Closed`). Not latch-worthy: a transient dial failure,
/// an unretryable-but-ambiguous open (still a dead end for the one stream
/// that hit it), and a closed router.
///
/// The live end of this scenario — a shared-transport opener actually
/// refusing and the pump latching mid-task — has no seam to inject through:
/// a destination's shared transport is built from the first streamer's real
/// api client by design, and a test-only override would weaken that
/// invariant for one test's sake. This classification test plus the `pump_*`
/// fallback tests below cover both halves instead, and the transport's own
/// tests cover the opener refusing with `Open`.
#[xmtp_common::test]
fn only_a_backend_refusal_latches() {
    use crate::subscriptions::stream_router::RouterError;
    use xmtp_api_d14n::{OpenError, TransportError};
    use xmtp_api_grpc::error::GrpcError;
    use xmtp_common::RetryableError;
    use xmtp_proto::api::ApiClientError;

    // The real v3 layering of a node without the surface: tonic status →
    // GrpcError (blanket-retryable) → NetworkError → ApiClientError — what
    // `subscribe_bidi` hands the opener.
    let unimplemented = OpenError::new(
        ApiClientError::client(GrpcError::from(tonic::Status::unimplemented(
            "unknown service xmtp.mls.api.v1.MlsApi",
        )))
        .endpoint("/xmtp.mls.api.v1.MlsApi/Subscribe"),
    );
    assert!(
        unimplemented.is_retryable(),
        "the grpc wrapping reports UNIMPLEMENTED as retryable — the trap \
         the classifier must not fall into"
    );
    assert!(
        is_bidi_unsupported(&RouterError::Transport(TransportError::Open(unimplemented))),
        "UNIMPLEMENTED is the backend's own verdict and must latch, \
         regardless of the wrapper's retryability"
    );

    // The in-process d14n/migration stub refusal.
    let stub = RouterError::Transport(TransportError::Open(OpenError::new(
        ApiClientError::OtherUnretryable(
            "the v3 bidi subscription is not available on this client".into(),
        ),
    )));
    assert!(is_bidi_unsupported(&stub), "the stub refusal must latch");
    assert!(
        is_bidi_dead_end(&stub),
        "an unretryable open is also a dead end for the stream that hit it"
    );

    // A transient dial failure: worth redialing, neither latch nor fallback.
    #[derive(Debug, thiserror::Error)]
    #[error("dial failed")]
    struct Transient;
    impl RetryableError for Transient {
        fn is_retryable(&self) -> bool {
            true
        }
    }
    let transient = RouterError::Transport(TransportError::Open(OpenError::new(Transient)));
    assert!(
        !is_bidi_unsupported(&transient),
        "a retryable open failure is worth redialing, not latching"
    );
    assert!(!is_bidi_dead_end(&transient));

    // Unretryable but not a capability verdict (a garbled handshake, say):
    // the stream falls back, the process does not latch.
    let garbled = RouterError::Transport(TransportError::Open(OpenError::new(
        ApiClientError::DecodeError(garbled_frame()),
    )));
    assert!(
        !is_bidi_unsupported(&garbled),
        "an unretryable decode failure is not the backend refusing the surface"
    );
    assert!(
        is_bidi_dead_end(&garbled),
        "but it is a dead end, so the stream falls back"
    );

    // The tombstoned shared transport: a destination's handle is immortal,
    // so its ledger only dies by shutting down after a refusal — `Closed` is
    // the refusal as every later stream to that destination sees it.
    assert!(
        is_bidi_unsupported(&RouterError::Transport(TransportError::Closed)),
        "a closed shared transport is the post-refusal tombstone and must latch"
    );

    // A closed router is client teardown, not backend capability.
    assert!(
        !is_bidi_unsupported(&RouterError::Closed),
        "a closed router is client teardown"
    );
    assert!(!is_bidi_dead_end(&RouterError::Closed));
}

/// With the latch pre-set — the process already saw this destination
/// refuse — a dispatcher-entered stream serves via the legacy path and
/// delivery still works. No env mutation: with the latch set, the
/// dispatcher's legacy arm is the same code path a gate-off process takes,
/// so the default test environment exercises exactly the arm a latched
/// gate-on process uses.
#[xmtp_common::test(unwrap_try = true)]
async fn latched_dispatch_delivers_via_legacy() {
    use crate::context::XmtpSharedContext;
    use xmtp_proto::api_client::XmtpMlsBidiStreams;

    tester!(alix);
    tester!(bo);

    // The destination the dispatcher will look up is the tester's real
    // backend; nextest runs one test per process, so latching it leaks
    // nowhere.
    let host = bo.client.context.api().api_client.host().to_owned();
    latch_bidi_unsupported(&host);
    assert!(
        !bidi_streams_active(&host),
        "the latch must keep bidi inactive for this destination"
    );

    let group = alix.create_group(None, None).await?;
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

    group.send_msg(b"latched onto legacy").await;
    let delivered = tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for the legacy-path delivery")
        .expect("callback channel closed")?;
    assert_eq!(delivered.decrypted_message_bytes, b"latched onto legacy");
}

/// Drive [`pump_stream`] with a subscribe future failing as `error` and a
/// small in-memory legacy fallback; returns the items the callback saw and
/// how many times `on_close` fired. Ready must fire off the fallback —
/// `wait_for_ready` would hang otherwise, so the timeout doubles as that
/// assertion.
async fn pump_through_fallback(
    host: &str,
    error: crate::subscriptions::stream_router::RouterError,
) -> (Vec<u8>, usize) {
    use crate::subscriptions::StreamKind;
    use crate::subscriptions::stream_router::RouterStream;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;
    use xmtp_proto::types::InstallationId;

    let subscribe = async move { Err::<RouterStream<u8>, _>(error) };
    // A fresh two-item stream per subscribe call, like the real factories.
    let fallback = || async { Ok(futures::stream::iter(vec![Ok(1u8), Ok(2u8)])) };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let closes = Arc::new(AtomicUsize::new(0));
    let on_close = {
        let closes = closes.clone();
        move || {
            closes.fetch_add(1, Ordering::SeqCst);
        }
    };
    let origin = StreamOrigin {
        kind: StreamKind::All,
        installation: InstallationId::from([0u8; 32]),
        host: host.to_owned(),
        cancel: CancellationToken::new(),
    };
    let mut handle = pump_stream(
        origin,
        subscribe,
        fallback,
        move |item| {
            let _ = tx.send(item);
        },
        on_close,
    );
    tokio::time::timeout(WAIT, handle.wait_for_ready())
        .await
        .expect("ready must fire off the fallback");
    let result = tokio::time::timeout(WAIT, handle.join())
        .await
        .expect("the fallback stream ends, so the pump task must too");
    assert!(
        matches!(result, Ok(Ok(()))),
        "the fallback must end cleanly: {result:?}"
    );

    let mut items = Vec::new();
    while let Ok(item) = rx.try_recv() {
        items.push(item.expect("fallback items are Ok"));
    }
    (items, closes.load(Ordering::SeqCst))
}

/// The pump's fallback branch on the real remote refusal shape: the
/// subscribe fails with a wrapped gRPC `UNIMPLEMENTED`, and the pump must
/// latch the process, signal ready off the fallback, deliver the fallback's
/// items through the watchdog runner, and fire `on_close` exactly once at
/// its natural end.
#[xmtp_common::test(unwrap_try = true)]
async fn pump_latches_and_serves_the_fallback_on_a_grpc_refusal() {
    use crate::subscriptions::stream_router::RouterError;
    use xmtp_api_d14n::{OpenError, TransportError};
    use xmtp_api_grpc::error::GrpcError;
    use xmtp_proto::api::ApiClientError;

    let refusal = OpenError::new(
        ApiClientError::client(GrpcError::from(tonic::Status::unimplemented(
            "unknown service xmtp.mls.api.v1.MlsApi",
        )))
        .endpoint("/xmtp.mls.api.v1.MlsApi/Subscribe"),
    );
    let (items, closes) = pump_through_fallback(
        "test://grpc-refusal",
        RouterError::Transport(TransportError::Open(refusal)),
    )
    .await;
    assert!(
        bidi_unsupported_latched("test://grpc-refusal"),
        "the refusal must latch its destination"
    );
    assert_eq!(items, vec![1, 2], "the fallback's items reach the callback");
    assert_eq!(closes, 1, "on_close fires exactly once, at the natural end");
}

/// Same branch on the in-process d14n/migration stub refusal
/// (`OtherUnretryable`).
#[xmtp_common::test(unwrap_try = true)]
async fn pump_latches_and_serves_the_fallback_on_the_stub_refusal() {
    use crate::subscriptions::stream_router::RouterError;
    use xmtp_api_d14n::{OpenError, TransportError};
    use xmtp_proto::api::ApiClientError;

    let refusal = OpenError::new(ApiClientError::OtherUnretryable(
        "the v3 bidi subscription is not available on this client".into(),
    ));
    let (items, closes) = pump_through_fallback(
        "test://stub-refusal",
        RouterError::Transport(TransportError::Open(refusal)),
    )
    .await;
    assert!(
        bidi_unsupported_latched("test://stub-refusal"),
        "the stub refusal must latch its destination"
    );
    assert_eq!(items, vec![1, 2], "the fallback's items reach the callback");
    assert_eq!(closes, 1, "on_close fires exactly once, at the natural end");
}

/// A dead-end open that is NOT a capability refusal serves this one stream
/// on the fallback but leaves the process unlatched — later dispatches may
/// still take the bidi path.
#[xmtp_common::test(unwrap_try = true)]
async fn pump_serves_the_fallback_without_latching_on_a_dead_end() {
    use crate::subscriptions::stream_router::RouterError;
    use xmtp_api_d14n::{OpenError, TransportError};
    use xmtp_proto::api::ApiClientError;

    let dead_end = OpenError::new(ApiClientError::DecodeError(garbled_frame()));
    let (items, closes) = pump_through_fallback(
        "test://dead-end",
        RouterError::Transport(TransportError::Open(dead_end)),
    )
    .await;
    assert!(
        !bidi_unsupported_latched("test://dead-end"),
        "a dead end is not a capability verdict and must not latch"
    );
    assert_eq!(items, vec![1, 2], "the fallback's items reach the callback");
    assert_eq!(closes, 1, "on_close fires exactly once, at the natural end");
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

/// A bidi api that never opens — just enough identity to key a transport.
#[derive(Clone)]
struct FixedHostApi(&'static str);

#[xmtp_common::async_trait]
impl xmtp_proto::api_client::XmtpMlsBidiStreams for FixedHostApi {
    type SubscribeStream = futures::stream::BoxStream<
        'static,
        std::result::Result<xmtp_proto::mls_v1::SubscribeResponse, xmtp_proto::api::ApiClientError>,
    >;
    type Error = xmtp_proto::api::ApiClientError;

    fn host(&self) -> &str {
        self.0
    }

    async fn subscribe_bidi(
        &self,
        _requests: futures::stream::BoxStream<'static, xmtp_proto::mls_v1::SubscribeRequest>,
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
    let _a = shared_transport(FixedHostApi("test://backend-a"));
    let _a_again = shared_transport(FixedHostApi("test://backend-a"));
    assert_eq!(
        shared_transport_count(),
        before + 1,
        "the same host shares one transport"
    );
    let _b = shared_transport(FixedHostApi("test://backend-b"));
    assert_eq!(
        shared_transport_count(),
        before + 2,
        "a different host gets its own"
    );
}

/// Latches are per destination: one backend refusing the surface leaves
/// every other destination's dispatch untouched.
#[xmtp_common::test]
fn destinations_latch_independently() {
    latch_bidi_unsupported("test://refusing-backend");
    assert!(bidi_unsupported_latched("test://refusing-backend"));
    assert!(
        !bidi_unsupported_latched("test://healthy-backend"),
        "a sibling destination must not inherit the latch"
    );
}

/// A born-suspended wire defers its backend's capability discovery to the
/// resume re-open — past the point where any stream could see the refusal
/// and latch. Resume is fire-and-forget (its acks are dropped, so it cannot
/// latch), which defers the latch one step further: the refusal tombstones
/// the transport in the background, and the next observation point — a
/// stream re-subscribing via the open path, or the next lifecycle fold —
/// latches the destination so later subscribes ride legacy.
#[xmtp_common::test(unwrap_try = true)]
async fn a_resume_time_refusal_latches_at_the_next_lifecycle_fold() {
    use xmtp_proto::types::Topic;

    suspend_bidi_streams().await?;
    let transport = shared_transport(FixedHostApi("test://born-refused"));
    // Registers and parks — born suspended, the refusing opener never runs.
    let mut lease = transport
        .lease(vec![(Topic::new_group_message(b"g"), 0)], 8)
        .await?;

    // Fire-and-forget: the call settles Ok before the re-open can refuse.
    resume_bidi_streams().await?;

    // The re-open runs the refusing opener in the ledger task; the tombstone
    // is observable as the parked lease ending. (In production a stream's
    // lease ending is what triggers its re-subscribe, which latches via the
    // open path.)
    while lease.next().await.is_some() {}

    // A bare lease has no re-subscribe machinery, so the next lifecycle fold
    // is the observation point here: suspend's settle sees `Closed` for the
    // tombstoned transport and latches the destination.
    suspend_bidi_streams().await?;
    assert!(
        bidi_unsupported_latched("test://born-refused"),
        "a refusal first seen at resume must latch by the next lifecycle fold"
    );
}
