use super::{MessagesApiSubscription, State, StreamGroupMessages};
use crate::{
    context::XmtpSharedContext,
    groups::MlsGroup,
    subscriptions::{
        Result, StreamAllMessages,
        stream_conversations::{StreamConversations, WelcomesApiSubscription},
    },
};
use futures::Stream;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::{
    ops::Range,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use xmtp_common::time::now_ns;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::prelude::XmtpMlsStreams;

pin_project! {
    pub struct StreamStatsWrapper<'a, Context: Clone, Conversations, Messages> {
        #[pin] inner: StreamAllMessages<'a, Context, Conversations, Messages>,
        #[pin] old_state: StreamState,
        stats: StatsInner
    }
}

pub trait StreamWithStats: Stream {
    type Item;

    fn stats(&self) -> Arc<StreamStats>;
}

impl<'a, Context: Clone, Conversations, Messages> StreamWithStats
    for StreamStatsWrapper<'a, Context, Conversations, Messages>
where
    Self: Stream,
{
    type Item = Result<StoredGroupMessage>;

    fn stats(&self) -> Arc<StreamStats> {
        self.stats.stats()
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
        tracing::info!("?");
        if let Some(start) = self.reconnect_start.take() {
            let _ = self.stats_tx.send(StreamStat::Reconnection {
                duration: start..(now_ns() as u64),
                num_groups: num_groups as u64,
            });
        }
    }

    fn set_state(&mut self, state: StreamState) {
        self.state = state;
        let _ = self
            .stats_tx
            .send(StreamStat::ChangeState { state: self.state });
    }

    fn stats(&self) -> Arc<StreamStats> {
        self.enabled.store(true, Ordering::SeqCst);
        self.stats.clone()
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
}

pub struct StreamStats {
    pub rx: Mutex<UnboundedReceiver<StreamStat>>,
}

impl StreamStats {
    pub fn new_stats(&self) -> Vec<StreamStat> {
        let mut stats = vec![];
        let mut stats_rx = self.rx.lock();
        while let Ok(stat) = stats_rx.try_recv() {
            stats.push(stat);
        }
        stats
    }
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

impl<'a, Context>
    StreamStatsWrapper<
        'a,
        Context,
        StreamConversations<'static, Context, WelcomesApiSubscription<'static, Context::ApiClient>>,
        StreamGroupMessages<'static, Context, MessagesApiSubscription<'static, Context::ApiClient>>,
    >
where
    Context: Clone + XmtpSharedContext + Send + Sync + 'static,
    Context::ApiClient: XmtpMlsStreams + Send + Sync + 'static,
{
    pub fn new(
        inner: StreamAllMessages<
            'a,
            Context,
            StreamConversations<
                'static,
                Context,
                WelcomesApiSubscription<'static, Context::ApiClient>,
            >,
            StreamGroupMessages<
                'static,
                Context,
                MessagesApiSubscription<'static, Context::ApiClient>,
            >,
        >,
    ) -> Self {
        Self {
            inner,
            old_state: StreamState::Unknown,
            stats: StatsInner::new(),
        }
    }
}

impl<'a, Context, Conversations> Stream
    for StreamStatsWrapper<
        'a,
        Context,
        Conversations,
        StreamGroupMessages<'a, Context, MessagesApiSubscription<'a, Context::ApiClient>>,
    >
where
    Context: XmtpSharedContext + 'a,
    Context::ApiClient: XmtpMlsStreams + 'a,
    Conversations: Stream<Item = Result<MlsGroup<Context>>>,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        let inner_poll = this.inner.as_mut().poll_next(cx);
        let inner_state: StreamState = (&this.inner.messages.state).into();

        if *this.old_state != inner_state {
            if *this.old_state != StreamState::Adding && inner_state == StreamState::Adding {
                this.stats.start_reconnect();
            }
            if *this.old_state == StreamState::Adding && inner_state != StreamState::Adding {
                this.stats
                    .finish_reconnect(this.inner.messages.groups.len());
            }

            this.stats.set_state(inner_state.into());
        }

        *this.old_state = inner_state;

        inner_poll
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_stream::StreamExt;

    use crate::{
        subscriptions::stream_messages::stream_stats::{StreamStat, StreamWithStats},
        tester,
    };

    #[xmtp_common::test(unwrap_try = true)]
    async fn stream_stats() {
        tester!(alix);
        tester!(bo);

        let mut stream = alix
            .stream_all_messages_owned_with_stats(None, None)
            .await?;
        let stream_stats = stream.stats();
        tokio::task::spawn(async move { while let Some(_) = stream.next().await {} });
        xmtp_common::time::sleep(Duration::from_millis(100)).await;

        bo.test_talk_in_dm_with(&alix).await?;

        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        let stats = stream_stats.new_stats();
        assert!(
            stats
                .iter()
                .any(|s| matches!(s, StreamStat::Reconnection { .. }))
        );
        assert!(
            stats
                .iter()
                .any(|s| matches!(s, StreamStat::ChangeState { .. }))
        );
    }
}
