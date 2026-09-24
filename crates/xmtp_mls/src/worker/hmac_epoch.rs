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
}

impl<Context> HmacEpochWorker<Context> {
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

impl<Context: XmtpSharedContext + 'static> WorkerFactory for Factory<Context> {
    fn kind(&self) -> WorkerKind {
        WorkerKind::HmacEpoch
    }

    fn create(&self, _metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        (
            Box::new(HmacEpochWorker {
                context: self.context.clone(),
                last_epoch: self.last_epoch.clone(),
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
        Factory {
            context,
            last_epoch: Arc::new(AtomicI64::new(crate::utils::time::hmac_epoch())),
        }
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        loop {
            let now = time::now_secs();
            time::sleep(self.wait_until_next_epoch(now)).await;
            self.report_epoch(crate::utils::time::hmac_epoch());
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
        };
        assert!(!worker.observe_epoch(10));
        assert!(worker.observe_epoch(11));
        assert!(!worker.observe_epoch(11));
        assert!(worker.observe_epoch(12));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn epoch_timer_turn_emits_once_and_invalidates_notifications() {
        tester!(alix, disable_workers);
        let current = crate::utils::time::hmac_epoch();
        let mut worker = HmacEpochWorker {
            context: alix.context.clone(),
            last_epoch: Arc::new(AtomicI64::new(current)),
        };
        let events = alix
            .context
            .events()
            .subscribe(EventFilter::new([EventKind::HmacKeysUpdated]), Some(10));
        let boundary = (current + 1) * xmtp_push_types::HMAC_EPOCH_SECONDS;
        assert_eq!(
            worker.wait_until_next_epoch(boundary - 1),
            Duration::from_secs(1)
        );
        let previous_revision = alix.context.task_channels().notification_revision();
        worker.report_epoch(current);
        assert!(events.drain().is_empty());
        worker.report_epoch(current + 1);
        assert_eq!(events.drain().len(), 1);
        assert_eq!(
            alix.context.task_channels().notification_revision(),
            previous_revision + 1
        );
        worker.report_epoch(current + 1);
        assert!(events.drain().is_empty());
    }
}
