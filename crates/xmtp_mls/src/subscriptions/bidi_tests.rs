//! Live integration tests for the XIP-83 bidirectional `Subscribe` connection.
//!
//! The actor's own unit tests (in `xmtp_api_d14n`) drive a mock wire and prove
//! its logic — auto-pong, probe correlation, teardown, backpressure. They prove
//! nothing about whether the node actually speaks the dialect. These open a real
//! [`BidiConnection`] against whichever backend the test feature switch selects
//! (local docker node by default; dev with `--features dev`) and assert the wire
//! contract end to end: the `Started` handshake, catch-up of pre-subscription
//! history strictly before the `TopicsLive` marker, live streaming after it,
//! `history_only` bounded catch-up with no live delivery, bounded-sync half-close
//! (the server closes the stream after the wave), and ping/pong probes.
//!
//! Assertions are derived entirely from the stream (not a side query): each
//! group-message frame carries an `is_commit` flag, so we count application
//! messages independently of the MLS commits the membership ops produce.
//!
//! v3-only and native-only: the bidi transport is implemented for the v3 client
//! and needs full-duplex HTTP/2 (the wasm gRPC-Web transport cannot carry it),
//! and `TestClient` is only bidi-capable under the v3 config.

use crate::builder::ClientBuilder;
use crate::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use std::collections::BTreeSet;
use std::time::Duration;
use xmtp_api_d14n::{BidiConnection, BidiError, BidiEvent};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_proto::mls_v1::subscribe_request::v1::{Mutate, mutate::Subscription};
use xmtp_proto::types::Topic;

/// `(cursor, is_commit)` of a bidi group-message frame. The bidi event carries
/// the raw proto `GroupMessage`, whose `V1.id` is the topic cursor and whose
/// `is_commit` flag separates MLS commits from application messages.
fn gm(m: &xmtp_proto::mls_v1::GroupMessage) -> (u64, bool) {
    use xmtp_proto::mls_v1::group_message::Version;
    match &m.version {
        Some(Version::V1(v1)) => (v1.id, v1.is_commit),
        None => panic!("group message frame without a version"),
    }
}

/// Concise one-line summary of a frame, for clear panic messages.
fn summarize(ev: &BidiEvent) -> String {
    match ev {
        BidiEvent::Started {
            keepalive_interval_ms,
            capabilities,
        } => format!("Started(keepalive={keepalive_interval_ms}ms, caps={capabilities:?})"),
        BidiEvent::CatchUpComplete { mutate_id } => {
            format!("CatchUpComplete(mutate_id={mutate_id})")
        }
        BidiEvent::TopicsLive { topics } => format!("TopicsLive(n={})", topics.len()),
        BidiEvent::GroupMessages(m) => {
            format!(
                "GroupMessages(ids={:?})",
                m.iter().map(|g| gm(g).0).collect::<Vec<_>>()
            )
        }
        BidiEvent::WelcomeMessages(w) => format!("WelcomeMessages(n={})", w.len()),
    }
}

/// Next frame, failing fast (rather than hanging to the test timeout) if the
/// server goes quiet or the connection ends when a frame was expected.
async fn next_within(conn: &mut BidiConnection, secs: u64) -> BidiEvent {
    tokio::time::timeout(Duration::from_secs(secs), conn.next())
        .await
        .expect("timed out waiting for a bidi frame")
        .expect("bidi connection closed unexpectedly")
}

/// Handshake + live welcome delivery + probe — the minimal proof the node speaks
/// the dialect at all.
#[xmtp_common::timeout(Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn bidi_connection_delivers_live_welcome_over_the_wire() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    // caro subscribes to its own welcome topic from the beginning of time.
    let welcome_topic = Topic::new_welcome_message(caro.installation_public_key());
    let initial = Mutate {
        adds: vec![Subscription {
            topic: welcome_topic.cloned_vec(),
            id_cursor: 0,
        }],
        removes: vec![],
        history_only: false,
        mutate_id: 1,
    };

    let mut conn = BidiConnection::open(&caro.context.api().api_client, initial).await?;

    let BidiEvent::Started {
        keepalive_interval_ms,
        ..
    } = conn.next().await.expect("connection closed before Started")
    else {
        panic!("first frame must be Started");
    };
    tracing::info!("bidi started; server keepalive = {keepalive_interval_ms}ms");

    let group = alix.create_group(None, None)?;
    group.add_members(&[caro.inbox_id()]).await?;

    let welcomes = loop {
        match conn.next().await {
            Some(BidiEvent::WelcomeMessages(w)) if !w.is_empty() => break w,
            Some(other) => tracing::info!("pre-welcome bidi event: {}", summarize(&other)),
            None => panic!("connection closed before the welcome arrived"),
        }
    };
    assert!(
        !welcomes.is_empty(),
        "expected at least one welcome message"
    );

    conn.probe().await?;
}

/// The happy path, in one test: everything published before the subscription is
/// caught up strictly before the live marker; messages published while the
/// stream starts arrive exactly once (either side of the marker); messages
/// published after the marker stream live, newer than every catch-up cursor.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn bidi_catch_up_precedes_live_marker_then_streams_live() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    const HISTORY: usize = 5;
    const CONCURRENT: usize = 3;
    const LIVE: usize = 4;
    const TOTAL_APP: usize = HISTORY + CONCURRENT + LIVE;

    // --- history: published before the subscription exists ---
    for i in 0..HISTORY {
        group
            .send_message(
                format!("history-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
    }

    // --- open the subscription from the beginning of the topic ---
    let topic = Topic::new_group_message(group.group_id);
    const MUTATE_ID: u64 = 77;
    let initial = Mutate {
        adds: vec![Subscription {
            topic: topic.cloned_vec(),
            id_cursor: 0,
        }],
        removes: vec![],
        history_only: false,
        mutate_id: MUTATE_ID,
    };
    let mut conn = BidiConnection::open(&bo.context.api().api_client, initial).await?;
    assert!(
        matches!(next_within(&mut conn, 10).await, BidiEvent::Started { .. }),
        "first frame must be Started"
    );

    // --- while the stream is starting, publish more (race the catch-up edge) ---
    for i in 0..CONCURRENT {
        group
            .send_message(
                format!("concurrent-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
    }

    let mut seen: BTreeSet<u64> = BTreeSet::new();
    let mut app_count = 0usize;
    let mut catchup_app = 0usize;
    let mut catchup_max = 0u64;
    let mut catchup_complete: Option<u64> = None;

    // Phase 1: drain catch-up until the topic crosses to live. `CatchUpComplete`
    // straddles the marker (the node emits it just after `TopicsLive`), so track
    // it on both sides rather than assuming an order.
    loop {
        match next_within(&mut conn, 10).await {
            BidiEvent::GroupMessages(m) => {
                for g in &m {
                    let (id, is_commit) = gm(g);
                    assert!(seen.insert(id), "duplicate cursor {id} in catch-up");
                    catchup_max = catchup_max.max(id);
                    if !is_commit {
                        app_count += 1;
                        catchup_app += 1;
                    }
                }
            }
            BidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
            BidiEvent::TopicsLive { topics } => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                break;
            }
            other => panic!("unexpected frame during catch-up: {}", summarize(&other)),
        }
    }
    // Every message published before the subscription is delivered in catch-up,
    // before the live marker. (Late-concurrent sends may add more.)
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
    // exactly once. Everything here is newer than the whole catch-up wave (the
    // live edge is monotonic).
    while app_count < TOTAL_APP {
        match next_within(&mut conn, 10).await {
            BidiEvent::GroupMessages(m) => {
                for g in &m {
                    let (id, is_commit) = gm(g);
                    assert!(
                        id > catchup_max,
                        "live cursor {id} must exceed catch-up max {catchup_max}"
                    );
                    assert!(
                        seen.insert(id),
                        "cursor {id} delivered twice (catch-up/live overlap or dup)"
                    );
                    if !is_commit {
                        app_count += 1;
                    }
                }
            }
            BidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
            other => panic!("unexpected frame on live stream: {}", summarize(&other)),
        }
    }

    assert_eq!(
        app_count, TOTAL_APP,
        "exactly the application messages we sent, no more, no fewer"
    );
    assert_eq!(
        catchup_complete,
        Some(MUTATE_ID),
        "CatchUpComplete must echo our mutate_id"
    );
    conn.probe().await?;
}

/// `history_only` catches the subscription up to the live edge — history plus the
/// `TopicsLive` / `CatchUpComplete` markers ("you have everything as of now") —
/// but does NOT register the topic for live delivery: a later publish must not
/// stream.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn bidi_history_only_catches_up_then_delivers_nothing_live() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    const HISTORY: usize = 4;
    for i in 0..HISTORY {
        group
            .send_message(
                format!("history-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
    }

    let topic = Topic::new_group_message(group.group_id);
    const MUTATE_ID: u64 = 99;
    let initial = Mutate {
        adds: vec![Subscription {
            topic: topic.cloned_vec(),
            id_cursor: 0,
        }],
        removes: vec![],
        history_only: true,
        mutate_id: MUTATE_ID,
    };
    let mut conn = BidiConnection::open(&bo.context.api().api_client, initial).await?;
    assert!(
        matches!(next_within(&mut conn, 10).await, BidiEvent::Started { .. }),
        "first frame must be Started"
    );

    // Catch-up still emits the markers; drain until both have arrived.
    let mut catchup_app = 0usize;
    let mut live_marker = false;
    let mut catchup_complete: Option<u64> = None;
    while !(live_marker && catchup_complete.is_some()) {
        match next_within(&mut conn, 10).await {
            BidiEvent::GroupMessages(m) => {
                for g in &m {
                    if !gm(g).1 {
                        catchup_app += 1;
                    }
                }
            }
            BidiEvent::TopicsLive { topics } => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                live_marker = true;
            }
            BidiEvent::CatchUpComplete { mutate_id } => catchup_complete = Some(mutate_id),
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

    // The topic was NOT registered for live delivery: a new publish must not
    // arrive over this connection.
    group
        .send_message(b"should-not-stream", SendMessageOpts::default())
        .await?;
    match tokio::time::timeout(Duration::from_secs(5), conn.next()).await {
        Err(_) => {}   // idle: correct — history_only does not stream live
        Ok(None) => {} // server closed the bounded stream — also acceptable
        Ok(Some(BidiEvent::GroupMessages(m))) => panic!(
            "history_only must not deliver live messages, got ids {:?}",
            m.iter().map(|g| gm(g).0).collect::<Vec<_>>()
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
/// [`BidiConnection::finish`] to half-close the request half; the server finishes
/// the wave and closes the stream itself, so the consumer drains `next()` to
/// `None`. After the close, `mutate` reports `Closed`.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
async fn bidi_history_only_half_close_drains_then_server_closes() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    const HISTORY: usize = 4;
    for i in 0..HISTORY {
        group
            .send_message(
                format!("history-{i}").as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
    }

    let topic = Topic::new_group_message(group.group_id);
    const MUTATE_ID: u64 = 123;
    let initial = Mutate {
        adds: vec![Subscription {
            topic: topic.cloned_vec(),
            id_cursor: 0,
        }],
        removes: vec![],
        history_only: true,
        mutate_id: MUTATE_ID,
    };
    let mut conn = BidiConnection::open(&bo.context.api().api_client, initial).await?;
    assert!(
        matches!(next_within(&mut conn, 10).await, BidiEvent::Started { .. }),
        "first frame must be Started"
    );

    // Half-close the request half: we are done sending, so the server should
    // finish the bounded wave and close the stream.
    conn.finish().await?;

    // Drain the bounded wave, then the server must close (next() -> None).
    let mut catchup_app = 0usize;
    let mut live_marker = false;
    let mut catchup_complete: Option<u64> = None;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), conn.next()).await {
            Err(_) => {
                panic!("bounded sync never closed: server kept the stream open after finish()")
            }
            Ok(None) => break, // server closed the bounded stream — the point of the test
            Ok(Some(BidiEvent::GroupMessages(m))) => {
                for g in &m {
                    if !gm(g).1 {
                        catchup_app += 1;
                    }
                }
            }
            Ok(Some(BidiEvent::TopicsLive { topics })) => {
                assert!(topics.contains(&topic), "our topic must be in TopicsLive");
                live_marker = true;
            }
            Ok(Some(BidiEvent::CatchUpComplete { mutate_id })) => {
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

    // The connection is gone: further requests report Closed.
    assert!(
        matches!(conn.mutate(Mutate::default()).await, Err(BidiError::Closed)),
        "mutate after the bounded-sync close must report Closed"
    );
}
