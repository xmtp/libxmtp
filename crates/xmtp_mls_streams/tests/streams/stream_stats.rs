use futures::StreamExt;
use xmtp_common::{StreamHandle, wait_for_some};
use xmtp_mls::subscriptions::stream_messages::stream_stats::*;
use xmtp_mls::tester;

#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_stats() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let mut stream = alix
        .stream_all_messages_owned_with_stats(None, None)
        .await?;
    let stats = stream.stats();
    let observed = parking_lot::Mutex::new(Vec::new());
    let mut drain = xmtp_common::spawn(None, async move { while stream.next().await.is_some() {} });

    bo.test_talk_in_dm_with(&alix).await?;
    for _ in 0..10 {
        bo.test_talk_in_new_group_with(&alix).await?;
    }

    let completed = wait_for_some(|| async {
        let next = stats.new_stats().await;
        let mut observed = observed.lock();
        observed.extend(next);
        let adding = observed.iter().any(|event| {
            matches!(
                event,
                StreamStat::ChangeState {
                    state: StreamState::Adding
                }
            )
        });
        let waiting = observed.iter().any(|event| {
            matches!(
                event,
                StreamStat::ChangeState {
                    state: StreamState::Waiting
                }
            )
        });
        let registered = observed.iter().any(|event| {
            matches!(
                event,
                StreamStat::Reconnection { duration, num_groups }
                    if duration.start <= duration.end && *num_groups >= 11
            )
        });
        (adding && waiting && registered).then_some(())
    })
    .await;
    match drain.end_and_wait().await {
        Ok(())
        | Err(
            xmtp_common::StreamHandleError::Cancelled
            | xmtp_common::StreamHandleError::StreamClosed,
        ) => {}
        Err(error) => panic!("stream drain did not stop cleanly: {error:?}"),
    }
    assert!(
        completed.is_some(),
        "subscription growth did not report its states: {:?}",
        observed.lock()
    );
}
