use super::{
    MessagesApiSubscription, ProcessFutureFactory, ProcessMessageFuture, State, StreamGroupMessages,
};
use crate::{context::XmtpSharedContext, subscriptions::Result};
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::{
    ops::Range,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Context,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use xmtp_common::time::now_ns;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::prelude::XmtpMlsStreams;

pin_project! {
    pub struct StreamWithStats<'a, Context: Clone, Subscription, Factory = ProcessMessageFuture<Context>> {
        #[pin] inner: StreamGroupMessages<'a, Context, Subscription, Factory>,
        #[pin] old_state: StreamState,
        stats: StatsInner
    }
}

struct StatsInner {
    reconnect_start: Option<u64>,
    stats_tx: UnboundedSender<StreamStat>,
    enabled: AtomicBool,
    stats: Arc<StreamStats>,
    state: StreamState,
}

impl StatsInner {
    fn start_reconnect(&mut self) {
        self.reconnect_start = Some(now_ns() as u64);
    }
    fn finish_reconnect(&mut self, num_groups: usize) {
        if let Some(start) = self.reconnect_start.take() {
            let _ = self.stats_tx.send(StreamStat::Reconnection {
                duration: start..(now_ns() as u64),
                num_groups: num_groups as u64,
            });
        }
    }

    fn set_state(&mut self, state: StreamState) {
        if self.state != state {
            self.state = state;
            let _ = self
                .stats_tx
                .send(StreamStat::ChangeState { state: self.state });
        }
    }
}

impl StatsInner {
    fn new() -> Self {
        let (stats_tx, stats_rx) = unbounded_channel();
        Self {
            stats_tx,
            reconnect_start: None,
            stats: Arc::new(StreamStats {
                rx: Mutex::new(stats_rx),
            }),
            enabled: AtomicBool::new(false),
            state: StreamState::Unknown,
        }
    }

    // Give a reference to the stats handle
    fn stats(&self) -> Arc<StreamStats> {
        self.enabled.store(true, Ordering::SeqCst);
        self.stats.clone()
    }
}

pub struct StreamStats {
    pub rx: Mutex<UnboundedReceiver<StreamStat>>,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum StreamState {
    Unknown,
    Waiting,
    Processing,
    Adding,
}

impl<'a, Out> From<&State<'a, Out>> for StreamState {
    fn from(state: &State<'a, Out>) -> Self {
        match state {
            State::Adding { .. } => Self::Adding,
            State::Processing { .. } => Self::Processing,
            State::Waiting => Self::Waiting,
        }
    }
}

pub enum StreamStat {
    // the duration is a range of two timestamps in nanos
    Reconnection {
        duration: Range<u64>,
        num_groups: u64,
    },
    ChangeState {
        state: StreamState,
    },
}

impl<'a, C, Factory> StreamWithStats<'a, C, MessagesApiSubscription<'a, C::ApiClient>, Factory>
where
    C: XmtpSharedContext + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    pub fn stats(&self) -> Arc<StreamStats> {
        self.stats.stats()
    }
}

impl<'a, C, Factory> Stream
    for StreamWithStats<'a, C, MessagesApiSubscription<'a, C::ApiClient>, Factory>
where
    C: XmtpSharedContext + 'a,
    C::ApiClient: XmtpMlsStreams + 'a,
    Factory: ProcessFutureFactory<'a> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        let inner_poll = this.inner.poll_next_unpin(cx);
        let inner_state: StreamState = (&this.inner.state).into();

        if *this.old_state != inner_state {
            if matches!(inner_state, StreamState::Adding) {
                this.stats.start_reconnect();
            }
            if matches!(*this.old_state, StreamState::Adding) {
                this.stats.finish_reconnect(this.inner.groups.len());
            }

            this.stats.set_state(inner_state.into());
        }

        inner_poll
    }
}
