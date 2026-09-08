//! Live backend subscription tests for handshake, target completion, and delivery.

use crate::builder::ClientBuilder;
use crate::context::XmtpSharedContext;
use crate::groups::send_message_opts::SendMessageOpts;
use std::collections::BTreeSet;
use std::time::Duration;
use xmtp_api_d14n::{BackendBinding, BidiConnection, BidiEvent, TransportBinding};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_proto::types::Topic;

fn gm(message: &xmtp_proto::backend_v1::ServerEnvelope) -> (u64, bool) {
    let message = xmtp_api_d14n::envelope::decode_group_message(message.clone())
        .expect("valid backend group message");
    (message.cursor.0, message.is_commit())
}

/// Concise one-line summary of a frame, for clear panic messages.
fn summarize(event: &BidiEvent) -> String {
    format!("{event:?}")
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
    let initial = BackendBinding::build_mutate([(welcome_topic, 0)], [], 1);

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
            Some(BidiEvent::WelcomeMessages { messages: w, .. }) if !w.is_empty() => break w,
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
async fn bidi_reaches_applied_target_then_streams_live() {
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
    let initial = BackendBinding::build_mutate([(topic.clone(), 0)], [], MUTATE_ID);
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

    let mut target = None;
    loop {
        match next_within(&mut conn, 10).await {
            BidiEvent::Applied { id, targets } => {
                assert_eq!(id, MUTATE_ID, "Applied must echo our update id");
                catchup_complete = Some(id);
                target = targets
                    .into_iter()
                    .find(|(candidate, _)| candidate == &topic)
                    .map(|(_, target)| target);
                assert!(target.is_some(), "the existing topic must have a target");
            }
            BidiEvent::GroupMessages { messages } => {
                for message in &messages {
                    let (id, is_commit) = gm(message);
                    assert!(id > catchup_max, "per-topic delivery must increase");
                    assert!(seen.insert(id), "duplicate cursor {id} in catch-up");
                    catchup_max = id;
                    if !is_commit {
                        app_count += 1;
                        catchup_app += 1;
                    }
                }
            }
            other => panic!("unexpected frame during catch-up: {}", summarize(&other)),
        }
        if target.is_some_and(|target| catchup_max >= target) {
            break;
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
            BidiEvent::GroupMessages { messages: m, .. } => {
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
            BidiEvent::Applied { .. } => panic!("update acked twice"),
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
        "Applied must echo our update id"
    );
    conn.probe().await?;
}
