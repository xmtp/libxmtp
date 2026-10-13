//! Report HMAC epoch changes observed while this client is open.

use crate::context::XmtpSharedContext;
use crate::worker::{BoxedWorker, DynMetrics, Worker, WorkerFactory, WorkerKind, WorkerResult};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use xmtp_common::time::{self, Duration};
use xmtp_events::{ClientEvent, EventWriter, HmacKeysUpdated};

pub struct HmacEpochWorker<Context> {
    context: Context,
    last_epoch: Arc<AtomicI64>,
    #[cfg(test)]
    clock: Option<TestClock>,
}

#[cfg(test)]
struct TestClock {
    now: Arc<AtomicI64>,
    ticks: futures::channel::mpsc::UnboundedReceiver<i64>,
    waits: futures::channel::mpsc::UnboundedSender<Duration>,
}

impl<Context> HmacEpochWorker<Context> {
    fn now(&self) -> i64 {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return clock.now.load(Ordering::Acquire);
        }
        time::now_secs()
    }

    async fn wait(&mut self, duration: Duration) {
        #[cfg(test)]
        if let Some(clock) = &mut self.clock {
            use futures::StreamExt;
            clock
                .waits
                .unbounded_send(duration)
                .expect("test wait observer must stay open");
            let now = clock.ticks.next().await.expect("test clock must stay open");
            clock.now.store(now, Ordering::Release);
            return;
        }
        time::sleep(duration).await;
    }

    fn wait_until_next_epoch(&self, now: i64) -> Duration {
        let next =
            (self.last_epoch.load(Ordering::Acquire) + 1) * xmtp_push_types::HMAC_EPOCH_SECONDS;
        Duration::from_secs((next - now).max(1) as u64)
    }

    fn observe_epoch(&mut self, current: i64) -> bool {
        loop {
            let previous = self.last_epoch.load(Ordering::Acquire);
            if current <= previous {
                return false;
            }
            if self
                .last_epoch
                .compare_exchange(previous, current, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl<Context: XmtpSharedContext> HmacEpochWorker<Context> {
    // implements: EVENT-026
    fn report_epoch(&mut self, observed: i64) {
        if self.observe_epoch(observed) {
            self.context.task_channels().mark_notification_changed();
            self.context
                .events()
                .emit(Some(ClientEvent::HmacKeysUpdated(HmacKeysUpdated)), None);
        }
    }
}

struct Factory<Context> {
    context: Context,
    last_epoch: Arc<AtomicI64>,
}

impl<Context> Factory<Context> {
    fn new(context: Context, startup_epoch: i64) -> Self {
        Self {
            context,
            last_epoch: Arc::new(AtomicI64::new(startup_epoch)),
        }
    }
}

impl<Context: XmtpSharedContext + 'static> WorkerFactory for Factory<Context> {
    fn kind(&self) -> WorkerKind {
        WorkerKind::HmacEpoch
    }

    fn create(&self, _metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        (
            Box::new(HmacEpochWorker {
                context: self.context.clone(),
                last_epoch: self.last_epoch.clone(),
                #[cfg(test)]
                clock: None,
            }),
            None,
        )
    }
}

#[xmtp_common::async_trait]
impl<Context: XmtpSharedContext + 'static> Worker for HmacEpochWorker<Context> {
    fn kind(&self) -> WorkerKind {
        WorkerKind::HmacEpoch
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static,
    {
        Factory::new(context, crate::utils::time::hmac_epoch())
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        loop {
            let now = self.now();
            self.wait(self.wait_until_next_epoch(now)).await;
            self.report_epoch(self.now() / xmtp_push_types::HMAC_EPOCH_SECONDS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_events::{EventFilter, EventKind};

    #[xmtp_common::test(unwrap_try = true)]
    async fn reports_only_new_epochs_after_startup() {
        let mut worker = HmacEpochWorker {
            context: (),
            last_epoch: Arc::new(AtomicI64::new(10)),
            clock: None,
        };
        assert!(!worker.observe_epoch(10));
        assert!(worker.observe_epoch(11));
        assert!(!worker.observe_epoch(11));
        assert!(worker.observe_epoch(12));
    }

    // verifies: EVENT-001
    #[xmtp_common::test(unwrap_try = true)]
    async fn epoch_timer_turn_emits_once_and_invalidates_notifications() {
        tester!(alix, disable_workers);
        let current = crate::utils::time::hmac_epoch();
        let mut worker = HmacEpochWorker {
            context: alix.context.clone(),
            last_epoch: Arc::new(AtomicI64::new(current)),
            clock: None,
        };
        let events = alix
            .context
            .events()
            .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
        let boundary = (current + 1) * xmtp_push_types::HMAC_EPOCH_SECONDS;
        assert_eq!(
            worker.wait_until_next_epoch(boundary - 1),
            Duration::from_secs(1)
        );
        let previous_revision = alix.context.task_channels().notification_revision();
        worker.report_epoch(current);
        assert!(events.drain().is_empty());
        worker.report_epoch(current + 1);
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::HmacKeysUpdated(_)),
                ..
            }]
        ));
        assert_eq!(
            alix.context.task_channels().notification_revision(),
            previous_revision + 1
        );
        worker.report_epoch(current + 1);
        assert!(events.drain().is_empty());
    }
    // verifies: EVENT-026
    #[xmtp_common::test(unwrap_try = true)]
    async fn running_epoch_worker_skips_closed_epochs_and_uses_no_network() {
        tester!(alix, disable_workers);
        let events = alix
            .context
            .events()
            .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
        let stats = alix.api_stats();
        let identity = alix.identity_api_stats();
        let counts = || {
            [
                stats.publish.get_count(),
                stats.create_upload.get_count(),
                stats.query.get_count(),
                stats.query_newest.get_count(),
                stats.subscribe.get_count(),
                stats.subscribe_static.get_count(),
                identity.get_inbox_ids.get_count(),
                identity.verify_smart_contract_wallet_signatures.get_count(),
            ]
        };
        let before = counts();
        let revision = alix.context.task_channels().notification_revision();
        let width = xmtp_push_types::HMAC_EPOCH_SECONDS;
        let now = Arc::new(AtomicI64::new(10 * width));
        let (ticks, receiver) = futures::channel::mpsc::unbounded();
        let (waits, mut scheduled) = futures::channel::mpsc::unbounded();
        let factory = Factory::new(alix.context.clone(), 10);
        let mut worker = HmacEpochWorker {
            context: factory.context.clone(),
            last_epoch: factory.last_epoch.clone(),
            clock: Some(TestClock {
                now: now.clone(),
                ticks: receiver,
                waits,
            }),
        };
        let mut running = Box::pin(worker.run_tasks());
        assert!(futures::poll!(&mut running).is_pending());
        assert!(events.drain().is_empty());
        assert_eq!(scheduled.try_recv()?, Duration::from_secs(width as u64));
        let mut previous = 10;
        for (epoch, expected) in [(10, 0), (11, 1), (11, 0), (14, 1), (13, 0), (14, 0)] {
            ticks.unbounded_send(epoch * width)?;
            assert!(futures::poll!(&mut running).is_pending());
            let emitted = events.drain();
            assert_eq!(emitted.len(), expected);
            previous = previous.max(epoch);
            assert_eq!(
                scheduled.try_recv()?,
                Duration::from_secs(((previous + 1 - epoch) * width) as u64)
            );
            assert!(
                emitted
                    .iter()
                    .all(|event| matches!(event.client, Some(ClientEvent::HmacKeysUpdated(_))))
            );
        }
        drop(running);
        assert_eq!(
            alix.context.task_channels().notification_revision(),
            revision + 2
        );
        now.store(18 * width, Ordering::Release);
        let (ticks, receiver) = futures::channel::mpsc::unbounded();
        let (waits, mut scheduled) = futures::channel::mpsc::unbounded();
        let factory = Factory::new(alix.context.clone(), 18);
        let mut reopened = HmacEpochWorker {
            context: factory.context.clone(),
            last_epoch: factory.last_epoch.clone(),
            clock: Some(TestClock {
                now,
                ticks: receiver,
                waits,
            }),
        };
        let mut running = Box::pin(reopened.run_tasks());
        assert!(futures::poll!(&mut running).is_pending());
        assert_eq!(scheduled.try_recv()?, Duration::from_secs(width as u64));
        ticks.unbounded_send(18 * width)?;
        assert!(futures::poll!(&mut running).is_pending());
        assert!(events.drain().is_empty());
        ticks.unbounded_send(19 * width)?;
        assert!(futures::poll!(&mut running).is_pending());
        assert_eq!(events.drain().len(), 1);
        assert_eq!(
            alix.context.task_channels().notification_revision(),
            revision + 3
        );
        assert_eq!(counts(), before);
    }
}
