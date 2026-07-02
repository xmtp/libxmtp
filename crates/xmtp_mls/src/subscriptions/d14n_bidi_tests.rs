//! Live integration tests for the XIP-83 bidirectional `Subscribe` connection on
//! the d14n (xmtpv4 `QueryApi`) backend, round-tripping to the local xmtpd.
//!
//! Counterpart to `bidi_tests.rs` (v3). These open a real [`D14nBidiConnection`]
//! against the feature-switched d14n test client and assert the same wire
//! contract — but exercise the d14n delivery path end to end: `OriginatorEnvelope`
//! batches decoded into unified `xmtp_proto::types` messages through the
//! per-envelope extractors (no transport-level reordering — see the binding's
//! module docs for why ordering belongs to the consumer).
//!
//! Assertions are derived from the stream: the unified `GroupMessage` carries an
//! openmls `ProtocolMessage`, so application messages are counted independently
//! of the MLS commits a membership change produces. d14n cursors are per-originator
//! vectors, so uniqueness is keyed on the full `(sequence_id, originator_id)` and
//! we assert phase + counts rather than a single global order.
//!
//! d14n-only and native-only.

use crate::builder::ClientBuilder;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::utils::test::{FullXmtpClient, TestXmtpMlsContext};
use openmls::framing::ContentType;
use std::collections::BTreeSet;
use std::time::Duration;
use xmtp_api_d14n::{D14nBidiConnection, D14nBidiEvent};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_proto::types::{GroupMessage, Topic};
use xmtp_proto::xmtp::xmtpv4::message_api::subscribe_request::v1::{Mutate, mutate::Subscription};

/// Stable key for a delivered group message: its full vector-cursor position.
fn cursor_key(m: &GroupMessage) -> (u64, u32) {
    (m.cursor.sequence_id, m.cursor.originator_id)
}

/// Whether a delivered group message is an application message (vs an MLS commit
/// from a membership change).
fn is_app(m: &GroupMessage) -> bool {
    matches!(m.message.content_type(), ContentType::Application)
}

/// Concise one-line summary of a frame, for clear panic messages.
fn summarize(ev: &D14nBidiEvent) -> String {
    match ev {
        D14nBidiEvent::Started {
            keepalive_interval_ms,
            capabilities,
        } => format!("Started(keepalive={keepalive_interval_ms}ms, caps={capabilities:?})"),
        D14nBidiEvent::CatchUpComplete { mutate_id } => {
            format!("CatchUpComplete(mutate_id={mutate_id})")
        }
        D14nBidiEvent::TopicsLive { topics } => format!("TopicsLive(n={})", topics.len()),
        D14nBidiEvent::GroupMessages(m) => format!(
            "GroupMessages(cursors={:?})",
            m.iter().map(cursor_key).collect::<Vec<_>>()
        ),
        D14nBidiEvent::WelcomeMessages(w) => format!("WelcomeMessages(n={})", w.len()),
    }
}

/// A d14n group-subscription Mutate from cursor 0 (the beginning of the topic).
fn subscribe_group(topic: &Topic, history_only: bool, mutate_id: u64) -> Mutate {
    Mutate {
        adds: vec![Subscription {
            topic: topic.cloned_vec(),
            last_seen: None,
        }],
        removes: vec![],
        history_only,
        mutate_id,
    }
}

/// Next frame, failing fast (rather than hanging to the test timeout) if the
/// server goes quiet or the connection ends when a frame was expected.
async fn next_within(conn: &mut D14nBidiConnection, secs: u64) -> D14nBidiEvent {
    tokio::time::timeout(Duration::from_secs(secs), conn.next())
        .await
        .expect("timed out waiting for a d14n bidi frame")
        .expect("d14n bidi connection closed unexpectedly")
}

/// Prelude shared by the group-subscription tests: two clients, a group with
/// both members, `history` pre-subscription app messages, then a subscription
/// opened from cursor 0 with `Started` already consumed. The clients are
/// returned so the test keeps them (and their backing streams) alive.
async fn open_group_history_sub(
    history: usize,
    history_only: bool,
    mutate_id: u64,
) -> (
    FullXmtpClient,
    FullXmtpClient,
    MlsGroup<TestXmtpMlsContext>,
    Topic,
    D14nBidiConnection,
) {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let group = alix.create_group(None, None).expect("create group");
    group
        .add_members(&[bo.inbox_id()])
        .await
        .expect("add member");

    for i in 0..history {
        group
            .send_message(
                format!("history-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await
            .expect("send history message");
    }

    let topic = Topic::new_group_message(group.group_id);
    let mut conn = D14nBidiConnection::open(
        bo.context.api().api_client.inner(),
        subscribe_group(&topic, history_only, mutate_id),
    )
    .await
    .expect("open d14n bidi connection");
    assert!(
        matches!(
            next_within(&mut conn, 10).await,
            D14nBidiEvent::Started { .. }
        ),
        "first frame must be Started"
    );
    (alix, bo, group, topic, conn)
}

/// Handshake + live welcome delivery + probe — the minimal proof xmtpd speaks the
/// d14n dialect and the welcome extraction works.
#[xmtp_common::timeout(Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn d14n_bidi_delivers_live_welcome_over_the_wire() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let welcome_topic = Topic::new_welcome_message(caro.installation_public_key());
    let initial = Mutate {
        adds: vec![Subscription {
            topic: welcome_topic.cloned_vec(),
            last_seen: None,
        }],
        removes: vec![],
        history_only: false,
        mutate_id: 1,
    };
    let mut conn = D14nBidiConnection::open(caro.context.api().api_client.inner(), initial).await?;

    let D14nBidiEvent::Started {
        keepalive_interval_ms,
        ..
    } = conn.next().await.expect("connection closed before Started")
    else {
        panic!("first frame must be Started");
    };
    tracing::info!("d14n bidi started; server keepalive = {keepalive_interval_ms}ms");

    let group = alix.create_group(None, None)?;
    group.add_members(&[caro.inbox_id()]).await?;

    // The loop's break condition is the delivery proof: a non-empty
    // WelcomeMessages frame arrived.
    loop {
        match conn.next().await {
            Some(D14nBidiEvent::WelcomeMessages(w)) if !w.is_empty() => break,
            Some(other) => tracing::info!("pre-welcome d14n event: {}", summarize(&other)),
            None => panic!("connection closed before the welcome arrived"),
        }
    }

    conn.probe().await?;
}

/// The happy path, in one test: everything published before the subscription is
/// caught up before the live marker; messages published while the stream starts
/// arrive exactly once; messages published after the marker stream live. Counts
/// application messages (excluding commits) and keys uniqueness on the full
/// vector cursor.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn d14n_bidi_catch_up_precedes_live_marker_then_streams_live() {
    const HISTORY: usize = 5;
    const CONCURRENT: usize = 3;
    const LIVE: usize = 4;
    const TOTAL_APP: usize = HISTORY + CONCURRENT + LIVE;
    const MUTATE_ID: u64 = 77;

    let (_alix, _bo, group, topic, mut conn) =
        open_group_history_sub(HISTORY, false, MUTATE_ID).await;

    // --- while the stream is starting, publish more (race the catch-up edge) ---
    for i in 0..CONCURRENT {
        group
            .send_message(
                format!("concurrent-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
    }

    let mut seen: BTreeSet<(u64, u32)> = BTreeSet::new();
    let mut app_count = 0usize;
    let mut catchup_app = 0usize;
    let mut catchup_complete: Option<u64> = None;

    // Phase 1: drain catch-up until the topic crosses to live.
    loop {
        match next_within(&mut conn, 10).await {
            D14nBidiEvent::GroupMessages(m) => {
                for g in &m {
                    assert!(seen.insert(cursor_key(g)), "duplicate cursor in catch-up");
                    if is_app(g) {
                        app_count += 1;
                        catchup_app += 1;
                    }
                }
            }
            D14nBidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
            D14nBidiEvent::TopicsLive { topics } => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                break;
            }
            other => panic!("unexpected frame during catch-up: {}", summarize(&other)),
        }
    }
    assert!(
        catchup_app >= HISTORY,
        "catch-up must contain at least the {HISTORY} pre-subscription messages, got {catchup_app}"
    );

    // --- live: published strictly after the marker; must stream live ---
    for i in 0..LIVE {
        group
            .send_message(format!("live-{i}").as_bytes(), SendMessageOpts::default())
            .await?;
    }

    // Phase 2: drain until every application message we sent has been delivered
    // exactly once.
    while app_count < TOTAL_APP {
        match next_within(&mut conn, 10).await {
            D14nBidiEvent::GroupMessages(m) => {
                for g in &m {
                    assert!(seen.insert(cursor_key(g)), "cursor delivered twice");
                    if is_app(g) {
                        app_count += 1;
                    }
                }
            }
            D14nBidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
            other => panic!("unexpected frame on live stream: {}", summarize(&other)),
        }
    }

    assert_eq!(
        app_count, TOTAL_APP,
        "exactly the application messages we sent"
    );
    assert_eq!(
        catchup_complete,
        Some(MUTATE_ID),
        "CatchUpComplete must echo our mutate_id"
    );
    conn.probe().await?;
}

/// `history_only` catches the subscription up to the live edge — history plus the
/// markers — but does NOT register the topic for live delivery: a later publish
/// must not stream.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn d14n_bidi_history_only_catches_up_then_delivers_nothing_live() {
    const HISTORY: usize = 4;
    const MUTATE_ID: u64 = 99;

    let (_alix, _bo, group, topic, mut conn) =
        open_group_history_sub(HISTORY, true, MUTATE_ID).await;

    let mut catchup_app = 0usize;
    let mut live_marker = false;
    let mut catchup_complete: Option<u64> = None;
    while !(live_marker && catchup_complete.is_some()) {
        match next_within(&mut conn, 10).await {
            D14nBidiEvent::GroupMessages(m) => {
                for g in &m {
                    if is_app(g) {
                        catchup_app += 1;
                    }
                }
            }
            D14nBidiEvent::TopicsLive { topics } => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                live_marker = true;
            }
            D14nBidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
            other => panic!(
                "unexpected frame during history-only catch-up: {}",
                summarize(&other)
            ),
        }
    }
    assert!(
        catchup_app >= HISTORY,
        "history-only catch-up must contain the {HISTORY} pre-subscription messages, got {catchup_app}"
    );
    assert_eq!(
        catchup_complete,
        Some(MUTATE_ID),
        "CatchUpComplete must echo our mutate_id"
    );

    // history_only does not register for live delivery: a new publish must not
    // arrive over this connection.
    group
        .send_message(b"should-not-stream", SendMessageOpts::default())
        .await?;
    match tokio::time::timeout(Duration::from_secs(5), conn.next()).await {
        Err(_) => {} // idle: correct — history only, nothing streams live
        // Without a half-close the node MUST keep the stream open (XIP-83 server
        // req 9 closes only after the *client* half-closes), so an ended stream
        // is a teardown bug — and tolerating it would let a dead connection pass
        // this negative check vacuously.
        Ok(None) => panic!("stream ended without a half-close: history_only must stay open"),
        Ok(Some(D14nBidiEvent::GroupMessages(m))) => panic!(
            "history_only must not deliver live messages, got {:?}",
            m.iter().map(cursor_key).collect::<Vec<_>>()
        ),
        Ok(Some(other)) => {
            panic!(
                "history_only delivered an unexpected live frame: {}",
                summarize(&other)
            )
        }
    }
}

/// Bounded sync: `history_only` + half-close. The client catches up, then calls
/// `finish()` to half-close the request half; xmtpd finishes the wave and closes
/// the stream, so the consumer drains `next()` to `None`. (That `mutate` then
/// reports `Closed` is the handle-side latch, pinned by the unit test
/// `mutate_and_probe_report_closed_after_finish` — asserting it here would be
/// vacuously true once `finish()` succeeded.)
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn d14n_bidi_history_only_half_close_drains_then_server_closes() {
    const HISTORY: usize = 4;
    const MUTATE_ID: u64 = 123;

    let (_alix, _bo, _group, topic, mut conn) =
        open_group_history_sub(HISTORY, true, MUTATE_ID).await;

    conn.finish().await?;

    let mut catchup_app = 0usize;
    let mut live_marker = false;
    let mut catchup_complete: Option<u64> = None;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), conn.next()).await {
            Err(_) => {
                panic!("bounded sync never closed: xmtpd kept the stream open after finish()")
            }
            Ok(None) => break, // server closed the bounded stream — the point of the test
            Ok(Some(D14nBidiEvent::GroupMessages(m))) => {
                for g in &m {
                    if is_app(g) {
                        catchup_app += 1;
                    }
                }
            }
            Ok(Some(D14nBidiEvent::TopicsLive { topics })) => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                live_marker = true;
            }
            Ok(Some(D14nBidiEvent::CatchUpComplete { mutate_id })) => {
                catchup_complete = Some(mutate_id)
            }
            Ok(Some(other)) => panic!(
                "unexpected frame during bounded sync: {}",
                summarize(&other)
            ),
        }
    }
    assert!(
        catchup_app >= HISTORY,
        "bounded sync must deliver the {HISTORY} pre-subscription messages, got {catchup_app}"
    );
    assert!(
        live_marker,
        "bounded sync must emit the live marker before closing"
    );
    assert_eq!(
        catchup_complete,
        Some(MUTATE_ID),
        "CatchUpComplete must echo our mutate_id"
    );
}
