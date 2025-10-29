use super::{Cursor, GroupList, ProcessMessageFuture, State};
use crate::groups::MlsGroup;
use pin_project_lite::pin_project;
use rstest::*;
use std::{
    borrow::Cow,
    collections::VecDeque,
    ops::Range,
    sync::{
        Arc,
        mpsc::{Receiver, Sender, channel},
    },
};
use xmtp_common::time::now_ns;

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

pub(super) struct StatsInner {
    pub(super) reconnect_start: Option<u64>,
    pub(super) stats_tx: Sender<StreamStat>,
    pub(super) stats: Arc<StreamStats>,
}

impl StatsInner {
    pub(super) fn start_reconnect(&mut self) {
        self.reconnect_start = Some(now_ns() as u64);
    }
    pub(super) fn finish_reconnect(&mut self) {
        if let Some(start) = self.reconnect_start.take() {
            self.stats_tx.send(StreamStat::Reconnection {
                duration: start..(now_ns() as u64),
            });
        }
    }
}

impl StatsInner {
    pub(super) fn new() -> Self {
        let (stats_tx, stats_rx) = channel();
        Self {
            stats_tx,
            reconnect_start: None,
            stats: Arc::new(StreamStats { rx: stats_rx }),
        }
    }

    // Give a reference to the stats handle
    pub(super) fn stats(&self) -> Arc<StreamStats> {
        self.stats.clone()
    }
}

pub struct StreamStats {
    pub(super) rx: Receiver<StreamStat>,
}

pub enum StreamStat {
    // the duration is a range of two timestamps in nanos
    Reconnection { duration: Range<u64> },
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
