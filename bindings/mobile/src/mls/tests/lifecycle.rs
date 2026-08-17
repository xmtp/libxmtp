//! Tests for the app-backgrounding lifecycle surface: `suspend_streams` /
//! `resume_streams` (the foreground/background pair) and
//! `FfiXmtpClient::catch_up_to_live` (the bounded one-shot catch-up).
//!
//! The bidi path is gated on `XMTP_BIDI_STREAMS_ENABLED`, read once per process
//! via a `LazyLock`. These tests set it at the top, before the first stream —
//! which is why they must run under `nextest` (process-per-test), as
//! `just test` does. The gate-off test deliberately leaves it unset.

use super::*;
use crate::mls::{FfiCatchUpOptions, resume_streams, suspend_streams};

/// The Application-kind message payloads in a conversation's durable store, in
/// order. Lets a test assert what catch-up/replay actually wrote to disk — the
/// real bytes — rather than trusting a summary counter.
async fn stored_app_payloads(convo: &FfiConversation) -> Vec<Vec<u8>> {
    convo
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.kind == FfiConversationMessageKind::Application)
        .map(|m| m.content)
        .collect()
}

/// Backgrounding round-trip: a kept lease survives `suspend_streams`, withholds
/// everything published while the wire is off the network, and on
/// `resume_streams` replays exactly that from its durable cursor.
///
/// v3-only. Exercises the *live* bidi wire — a `BidiTransport` over the v3
/// `SubscribeEnvelopes` bidi RPC. Under `--features d14n` the d14n gateway
/// *client* does not yet wire up that subscribe RPC — the cold open reports
/// "the v3 bidi subscription is not available on this client" even though the
/// pinned xmtpd backend (`sha-ac17e82`) serves it — so the process latches onto
/// the legacy streams, where `suspend`/`resume` are no-ops and cannot withhold;
/// the assertions below would rightly fail. Un-gate once `xmtp_api_d14n` exposes
/// the bidi subscribe for the d14n binding. The one-shot `catch_up_to_live`
/// tests need no live wire, so they run on both backends.
#[cfg_attr(
    feature = "d14n",
    ignore = "the d14n gateway client does not yet expose the v3 bidi SubscribeEnvelopes RPC (cold open refused → legacy fallback, where suspend/resume are no-ops)"
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn bidi_suspend_and_resume_redelivers() {
    unsafe { std::env::set_var("XMTP_BIDI_STREAMS_ENABLED", "1") };

    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // bo owns the group handle; alix joins via its welcome and streams.
    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    alix.inner_client.sync_welcomes().await.unwrap();

    let cb = Arc::new(RustStreamCallback::default());
    let stream = alix
        .conversations()
        .stream_all_messages(cb.clone(), None)
        .await;
    stream.wait_for_ready().await;

    // Baseline: a live message is delivered over the bidi wire.
    bo_group
        .send(b"before".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    cb.wait_for_delivery(Some(15)).await.unwrap();
    assert_eq!(
        cb.message_contents(),
        vec![b"before".to_vec()],
        "the live message is delivered before suspending"
    );

    // Background: take the shared wire off the network.
    suspend_streams().await.unwrap();

    // Published while suspended — must not reach the stream until resume.
    bo_group
        .send(b"during".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Prove suspend actually withholds. Give the (now offline) wire a generous
    // window to misbehave: were suspend a no-op, the live wire would deliver
    // "during" here and this wait would return `Ok`, failing the test. A timeout
    // (`Err`) is the pass — nothing arrived.
    assert!(
        cb.wait_for_delivery(Some(5)).await.is_err(),
        "suspend must withhold delivery: nothing may arrive until resume"
    );
    assert_eq!(
        cb.message_contents(),
        vec![b"before".to_vec()],
        "the message published while suspended must not have been delivered"
    );

    // Foreground: the kept lease reconnects and replays from its durable cursor.
    resume_streams().await.unwrap();
    cb.wait_for_delivery(Some(15)).await.unwrap();
    assert_eq!(
        cb.message_contents(),
        vec![b"before".to_vec(), b"during".to_vec()],
        "resume must redeliver exactly the withheld message, and only it, in order"
    );

    stream.end();
}

/// One-shot catch-up joins a pending group and replays its history from durable
/// cursors, then stops — and a second call finds nothing owed (idempotent).
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn bidi_catch_up_to_live_replays_and_is_idempotent() {
    unsafe { std::env::set_var("XMTP_BIDI_STREAMS_ENABLED", "1") };

    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // bo creates a group with alix and sends. alix has never synced or streamed,
    // so both the welcome and the message are owed.
    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    bo_group
        .send(b"missed while away".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let summary = alix.catch_up_to_live(None).await.unwrap();
    assert!(summary.completed);
    assert_eq!(
        summary.conversations, 1,
        "catch-up must join the one pending group"
    );
    assert!(
        summary.messages >= 1,
        "catch-up must replay the missed message"
    );

    let convos = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .await
        .unwrap();
    assert_eq!(convos.len(), 1, "the group is now in alix's store");

    // Catch-up wrote the real payload to disk, not just a counter: the missed
    // text is readable with no further sync.
    let payloads = stored_app_payloads(convos[0].conversation().as_ref()).await;
    assert_eq!(
        payloads,
        vec![b"missed while away".to_vec()],
        "the missed message is stored and readable after catch-up"
    );

    // Nothing owed now: idempotent — zero new on both axes.
    let again = alix.catch_up_to_live(None).await.unwrap();
    assert!(again.completed);
    assert_eq!(
        again.messages, 0,
        "a second catch-up must not replay already-stored messages"
    );
    assert_eq!(
        again.conversations, 0,
        "a second catch-up must not rejoin the already-known group"
    );
}

/// Cancellation safety: a deadline so short the run can be cut off mid-flight
/// must leave no partial state. `tokio::time::timeout` DROPS the future on
/// expiry, unwinding whatever `process_one`/welcome-join was in flight; a full
/// run afterward must still converge the store from durable cursors, and a
/// later call must find nothing owed. Convergence-after-cut is the assertion,
/// so it holds whether or not the 1ms deadline actually fired.
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn bidi_catch_up_to_live_bounded_run_is_cancel_safe() {
    unsafe { std::env::set_var("XMTP_BIDI_STREAMS_ENABLED", "1") };

    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    // Several messages so a 1ms deadline is likely to land mid-processing.
    for i in 0..5 {
        bo_group
            .send(
                format!("owed {i}").into_bytes(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
    }

    // May finish or be cut short — both are contractually valid. On the deadline
    // the summary carries the partial total persisted before the cut (possibly
    // zero if nothing landed in time) and `completed` is false; the full run
    // below proves the cut left no partial/corrupt state regardless.
    let bounded = alix
        .catch_up_to_live(Some(FfiCatchUpOptions {
            timeout_ms: Some(1),
        }))
        .await
        .unwrap();
    if !bounded.completed {
        assert!(
            bounded.messages <= 5,
            "partial count cannot exceed what was owed"
        );
    }

    // Whether or not the bounded run was cut off, a full run converges the store
    // from durable cursors — proving the cut left no partial/corrupt state.
    let full = alix.catch_up_to_live(None).await.unwrap();
    assert!(full.completed);
    let convos = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .await
        .unwrap();
    assert_eq!(convos.len(), 1, "the group converges regardless of the cut");

    // Convergence means the whole history landed intact — all five owed messages,
    // in order, with nothing dropped or duplicated by the cut-off run.
    let payloads = stored_app_payloads(convos[0].conversation().as_ref()).await;
    let expected: Vec<Vec<u8>> = (0..5).map(|i| format!("owed {i}").into_bytes()).collect();
    assert_eq!(
        payloads, expected,
        "all five owed messages converge to the store, in order"
    );

    // And catch-up is now drained: nothing owed on either axis.
    let again = alix.catch_up_to_live(None).await.unwrap();
    assert!(again.completed);
    assert_eq!(again.messages, 0);
    assert_eq!(again.conversations, 0);
}

/// With the gate off, `suspend`/`resume` are safe no-ops and `catch_up_to_live`
/// runs the legacy sync — still joining the pending group and reporting counts.
/// (Its own process under nextest, so the gate is genuinely unset.)
#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn catch_up_to_live_falls_back_when_bidi_disabled() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    bo_group
        .send(b"legacy path".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Nothing is streaming, so these are no-ops regardless of the gate.
    suspend_streams().await.unwrap();
    resume_streams().await.unwrap();

    let summary = alix.catch_up_to_live(None).await.unwrap();
    assert!(summary.completed);
    assert_eq!(
        summary.conversations, 1,
        "legacy catch-up must still join the one pending group"
    );

    let convos = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .await
        .unwrap();
    assert_eq!(convos.len(), 1);

    // The legacy sync path delivers the real payload too.
    let payloads = stored_app_payloads(convos[0].conversation().as_ref()).await;
    assert_eq!(
        payloads,
        vec![b"legacy path".to_vec()],
        "legacy catch-up must store the missed message"
    );
}
