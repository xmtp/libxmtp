//! Test diagnostics from the shared receiver status.

use crate::subscriptions::{
    Result,
    incoming::{IncomingConnection, IncomingProcessing, IncomingRegistration},
    stream_all::StreamAllMessages,
};
use futures::{Stream, StreamExt};
use std::{
    ops::Range,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::Notify;
use xmtp_common::{StreamHandle, time::now_ns};
use xmtp_db::group_message::StoredGroupMessage;

pub trait StreamWithStats: Stream<Item = Result<StoredGroupMessage>> {
    fn stats(&self) -> Arc<StreamStats>;
    fn spin(self) -> Arc<Notify>;
}

pub struct StreamStats {
    pending: parking_lot::Mutex<Vec<StreamStat>>,
}

impl StreamStats {
    pub async fn new_stats(&self) -> Vec<StreamStat> {
        std::mem::take(&mut *self.pending.lock())
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum StreamState {
    Unknown,
    Waiting,
    Processing,
    Adding,
}

#[derive(Debug)]
pub enum StreamStat {
    Reconnection {
        duration: Range<u64>,
        num_groups: u64,
    },
    ChangeState {
        state: StreamState,
    },
}

pub struct StreamStatsWrapper {
    inner: StreamAllMessages,
    stats: Arc<StreamStats>,
    watch: Box<dyn StreamHandle<StreamOutput = ()>>,
}

impl StreamStatsWrapper {
    pub fn new(inner: StreamAllMessages) -> Self {
        let stats = Arc::new(StreamStats {
            pending: parking_lot::Mutex::new(Vec::new()),
        });
        let control = inner.control.clone();
        let events = stats.clone();
        let watch = xmtp_common::spawn(None, async move {
            let mut previous = StreamState::Unknown;
            let mut reconnect = None;
            loop {
                let status = control.catch_up_snapshot();
                let state = if status.connection == IncomingConnection::Closed {
                    break;
                } else if status
                    .topics
                    .iter()
                    .any(|topic| topic.registration == IncomingRegistration::Pending)
                    || matches!(
                        status.connection,
                        IncomingConnection::Connecting | IncomingConnection::Reconnecting
                    )
                {
                    StreamState::Adding
                } else if status.processing == IncomingProcessing::Pending {
                    StreamState::Processing
                } else {
                    StreamState::Waiting
                };
                if state != previous {
                    let mut pending = events.pending.lock();
                    if state == StreamState::Adding {
                        reconnect = Some(now_ns() as u64);
                    }
                    if previous == StreamState::Adding
                        && let Some(start) = reconnect.take()
                    {
                        pending.push(StreamStat::Reconnection {
                            duration: start..now_ns() as u64,
                            num_groups: status.topics.len() as u64,
                        });
                    }
                    pending.push(StreamStat::ChangeState { state });
                    previous = state;
                }
                control.changed().await;
            }
        });
        Self {
            inner,
            stats,
            watch: Box::new(watch),
        }
    }
}

impl Stream for StreamStatsWrapper {
    type Item = Result<StoredGroupMessage>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl StreamWithStats for StreamStatsWrapper {
    fn stats(&self) -> Arc<StreamStats> {
        self.stats.clone()
    }
    fn spin(mut self) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        let wake = notify.clone();
        xmtp_common::spawn(None, async move {
            while self.next().await.is_some() {
                wake.notify_one();
            }
        });
        notify
    }
}

impl Drop for StreamStatsWrapper {
    fn drop(&mut self) {
        self.watch.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_common::wait_for_some;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_stream_stats() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let mut stream = alix
            .stream_all_messages_owned_with_stats(None, None)
            .await?;
        let stats = stream.stats();
        let observed = parking_lot::Mutex::new(Vec::new());
        let mut drain =
            xmtp_common::spawn(None, async move { while stream.next().await.is_some() {} });

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
        drain.end_and_wait().await?;
        assert!(
            completed.is_some(),
            "subscription growth did not report its states: {:?}",
            observed.lock()
        );
    }
}
