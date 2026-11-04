use futures::StreamExt;
use std::time::Duration;
use xmtp_mls_engine::groups::{
    send_message_opts::SendMessageOpts,
    test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps},
};

/// The premise the whole watchdog design rests on: a real, live v3 subscription that goes
/// idle produces *nothing* (the v3 path has no server-side keepalive), so it appears as a
/// perpetual `Pending`. Wrapped in a `WatchdogStream`, that idle stream must trip with
/// `StreamStale` and then terminate — while live messages still flow through untouched.
#[xmtp_common::timeout(Duration::from_secs(40))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn watchdog_trips_on_idle_real_stream() {
    use crate::subscriptions::{SubscribeError, stream_all_messages, watchdog::WatchdogStream};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let group = alix
        .create_group_with_members(&[bo.context.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;

    // Wrap a real, live subscription with a short idle timeout.
    let raw = stream_all_messages(&bo.context, None, None).await?;
    let watchdog = WatchdogStream::new(raw, Some(Duration::from_secs(3)));
    futures::pin_mut!(watchdog);

    // A live application message flows through and resets the idle timer. Tolerate any
    // leading protocol messages (e.g. the membership commit) ahead of our payload.
    group
        .send_message(b"first", SendMessageOpts::default())
        .await?;
    xmtp_common::time::timeout(Duration::from_secs(15), async {
        loop {
            match watchdog.next().await {
                Some(Ok(msg))
                    if String::from_utf8_lossy(msg.decrypted_message_bytes.as_slice())
                        == "first" =>
                {
                    break;
                }
                Some(Ok(_)) => continue,
                other => panic!("expected live messages, got {other:?}"),
            }
        }
    })
    .await
    .expect("live message did not flow through the watchdog");

    // With the group now idle and no server keepalive on the v3 path, the real stream
    // produces nothing, so the watchdog must trip. A stray protocol message would merely
    // reset the timer; we still expect an eventual trip.
    let tripped = xmtp_common::time::timeout(Duration::from_secs(15), async {
        loop {
            match watchdog.next().await {
                Some(Err(SubscribeError::StreamStale)) => break,
                Some(Ok(_)) => continue,
                other => panic!("expected StreamStale, got {other:?}"),
            }
        }
    })
    .await;
    assert!(
        tripped.is_ok(),
        "watchdog did not trip on a real idle stream within the timeout"
    );

    // Having tripped, the stream is finished.
    assert!(
        watchdog.next().await.is_none(),
        "watchdog stream must terminate after tripping"
    );
}
