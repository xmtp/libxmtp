use super::{Cursor, GroupList, MessagesApiSubscription, ProcessMessageFuture, State};
use crate::{context::XmtpSharedContext, groups::MlsGroup};
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use rstest::*;
use std::{
    borrow::Cow,
    collections::VecDeque,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use xmtp_common::time::now_ns;
use xmtp_proto::prelude::XmtpMlsStreams;

pin_project! {
    pub struct StreamGroupMessages<'a, Context: Clone, Subscription, Factory = ProcessMessageFuture<Context>> {
        #[pin] pub(super) inner: Subscription,
        #[pin] pub(super) state: State<'a, Subscription>,
        pub(super) factory: Factory,
        pub(super) context: Cow<'a, Context>,
        pub(super) groups: GroupList,
        pub(super) add_queue: VecDeque<MlsGroup<Context>>,
        pub(super) returned: Vec<Cursor>,
        pub(super) got: Vec<Cursor>,
        pub(super) stats: StatsInner
    }
}

impl<C> StreamGroupMessages<'static, C, MessagesApiSubscription<'static, C::ApiClient>>
where
    C: XmtpSharedContext + 'static,
    C::ApiClient: XmtpMlsStreams + Send + Sync + 'static,
    C::Db: Send + 'static,
{
    pub fn stats(&self) -> Arc<StreamStats> {
        self.stats.stats()
    }
}

pub(super) struct StatsInner {
    pub(super) reconnect_start: Option<u64>,
    // Enables tracking - true if a stats handle has been handed out.
    pub(super) enabled: AtomicBool,
    pub(super) stats_tx: UnboundedSender<StreamStat>,
    pub(super) stats: Arc<StreamStats>,
    pub(super) state: StreamState,
}

impl StatsInner {
    pub(super) fn start_reconnect(&mut self) {
        self.reconnect_start = Some(now_ns() as u64);
    }
    pub(super) fn finish_reconnect(&mut self, num_groups: usize) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        if let Some(start) = self.reconnect_start.take() {
            let _ = self.stats_tx.send(StreamStat::Reconnection {
                duration: start..(now_ns() as u64),
                num_groups: num_groups as u64,
            });
        }
    }

    pub(super) fn set_state(&mut self, state: StreamState) {
        if self.state != state {
            self.state = state;
            let _ = self
                .stats_tx
                .send(StreamStat::ChangeState { state: self.state });
        }
    }
}

impl StatsInner {
    pub(super) fn new() -> Self {
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
    pub(super) fn stats(&self) -> Arc<StreamStats> {
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

pub mod cases {
    use xmtp_proto::types::GroupId;

    use super::*;

    // creates groups 1, 2, 3, 4
    #[fixture]
    pub fn group_list() -> Vec<GroupId> {
        vec![vec![1], vec![2], vec![3], vec![4]]
            .into_iter()
            .map(|mut i| {
                i.resize(31, 0);
                GroupId::from(i)
            })
            .collect::<Vec<GroupId>>()
    }
}
