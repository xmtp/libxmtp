//! A replaceable event sink and an optional bounded delivery queue.

use std::{
    cell::Cell,
    collections::BTreeMap,
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use parking_lot::{Mutex, RwLock};
use tracing::{
    Event,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

use crate::Level;

/// The queue size used by a Node log sink.
pub const BOUNDED_SINK_CAPACITY: usize = 4_096;

/// One event sent to a log sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
    pub timestamp_ns: i64,
    /// Records discarded since the last record delivered by a bounded sink.
    pub dropped_records: u64,
}

/// Error returned by a log sink.
pub type SinkError = Box<dyn Error + Send + Sync>;

/// A destination for log records. A direct sink is called on the logging thread.
pub trait LogSinkTarget: Send + Sync + 'static {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError>;
}

thread_local! {
    static IN_SINK: Cell<bool> = const { Cell::new(false) };
}

struct SinkCallGuard;

impl SinkCallGuard {
    fn enter() -> Option<Self> {
        IN_SINK.with(|active| {
            if active.get() {
                None
            } else {
                active.set(true);
                Some(Self)
            }
        })
    }
}

impl Drop for SinkCallGuard {
    fn drop(&mut self) {
        IN_SINK.with(|active| active.set(false));
    }
}

static LAST_ERROR_REPORT_SECOND: AtomicU64 = AtomicU64::new(0);

fn report_error(errors: &AtomicU64, detail: &'static str) {
    errors.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    if LAST_ERROR_REPORT_SECOND
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
            (now.saturating_sub(previous) >= 60).then_some(now)
        })
        .is_ok()
    {
        // The re-entry guard is held by the caller. This event reaches the
        // native layers, but cannot enter the sink again.
        tracing::error!(target: "xmtp_common", "log sink {detail}");
    }
}

fn deliver(target: &dyn LogSinkTarget, record: LogRecord, errors: &AtomicU64) {
    match catch_unwind(AssertUnwindSafe(|| target.on_record(record))) {
        Ok(Ok(())) => {}
        Ok(Err(_)) => report_error(errors, "returned an error"),
        Err(_) => report_error(errors, "panicked"),
    }
}

#[derive(Default)]
struct SinkState {
    target: RwLock<Option<Arc<dyn LogSinkTarget>>>,
    errors: AtomicU64,
}

/// Always-present layer slot. Replacing a sink never calls it under a lock.
#[derive(Clone, Default)]
pub(crate) struct SinkSlot(Arc<SinkState>);

impl SinkSlot {
    pub(crate) fn set_sink(&self, target: Option<Arc<dyn LogSinkTarget>>) {
        *self.0.target.write() = target;
    }

    pub(crate) fn error_count(&self) -> u64 {
        self.0.errors.load(Ordering::Relaxed)
    }
}

impl<S: tracing::Subscriber> Layer<S> for SinkSlot {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let Some(_guard) = SinkCallGuard::enter() else {
            return;
        };
        let Some(target) = self.0.target.read().clone() else {
            return;
        };
        let record = LogRecord::from_event(event);
        deliver(target.as_ref(), record, &self.0.errors);
    }
}

#[derive(Default)]
struct RecordVisitor {
    message: String,
    fields: BTreeMap<String, String>,
}

impl RecordVisitor {
    fn push(&mut self, name: &str, value: String) {
        if name == "message" {
            self.message = value;
        } else {
            self.fields.insert(name.to_owned(), value);
        }
    }
}

impl Visit for RecordVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        let rendered = if field.name() == "message" {
            rendered
                .strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))
                .unwrap_or(&rendered)
                .to_owned()
        } else {
            rendered
        };
        self.push(field.name(), rendered);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field.name(), value.to_owned());
    }
}

impl LogRecord {
    fn from_event(event: &Event<'_>) -> Self {
        let metadata = event.metadata();
        let mut visitor = RecordVisitor::default();
        event.record(&mut visitor);
        let level = match *metadata.level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::TRACE => Level::Trace,
        };
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX))
            .unwrap_or_default();
        Self {
            level,
            target: metadata.target().to_owned(),
            message: visitor.message,
            fields: visitor.fields,
            timestamp_ns,
            dropped_records: 0,
        }
    }
}

/// Send records through one bounded queue to one drain thread.
///
/// The drain thread owns no queue or slot lock while it calls the destination.
/// Dropping the adapter closes the queue without waiting for the destination.
pub struct BoundedSink {
    sender: SyncSender<LogRecord>,
    dropped: Arc<AtomicU64>,
    pending_drops: Arc<Mutex<u64>>,
    errors: Arc<AtomicU64>,
}

impl BoundedSink {
    pub fn new(target: Arc<dyn LogSinkTarget>) -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel::<LogRecord>(BOUNDED_SINK_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let pending_drops = Arc::new(Mutex::new(0));
        let errors = Arc::new(AtomicU64::new(0));
        let worker_drops = pending_drops.clone();
        let worker_errors = errors.clone();
        thread::Builder::new()
            .name("xmtp-log-sink".to_owned())
            .spawn(move || {
                while let Ok(mut record) = receiver.recv() {
                    let missed = std::mem::take(&mut *worker_drops.lock());
                    record.dropped_records = record.dropped_records.saturating_add(missed);
                    if let Some(_guard) = SinkCallGuard::enter() {
                        deliver(target.as_ref(), record, &worker_errors);
                    }
                }
            })?;
        Ok(Self {
            sender,
            dropped,
            pending_drops,
            errors,
        })
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn error_count(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }
}

impl LogSinkTarget for BoundedSink {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
        match self.sender.try_send(record) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                let mut pending = self.pending_drops.lock();
                *pending = pending.saturating_add(1);
                self.dropped.fetch_add(1, Ordering::Relaxed);
                // The first subsequent record *delivered* carries this count,
                // including one that was already waiting in the queue.
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "log sink drain thread stopped",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize},
        mpsc::{Receiver, Sender, channel},
    };
    use std::time::Duration;
    use tracing_subscriber::prelude::*;

    #[derive(Default)]
    struct NativeCount(Arc<AtomicUsize>);

    impl<S: tracing::Subscriber> Layer<S> for NativeCount {
        fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[derive(Default)]
    struct Collect(Arc<parking_lot::Mutex<Vec<LogRecord>>>);

    impl LogSinkTarget for Collect {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }

    fn record(message: &str) -> LogRecord {
        LogRecord {
            level: Level::Info,
            target: "xmtp_mls".into(),
            message: message.into(),
            fields: BTreeMap::new(),
            timestamp_ns: 1,
            dropped_records: 0,
        }
    }

    #[test]
    fn sink_receives_records_in_order() {
        let sink = Arc::new(Collect::default());
        let slot = SinkSlot::default();
        slot.set_sink(Some(sink.clone()));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(slot), || {
            tracing::info!(target: "xmtp_mls", attempt = 1, "first");
            tracing::warn!(target: "xmtp_mls", "second");
            tracing::error!(target: "xmtp_db", "third");
        });
        let records = sink.0.lock();
        assert_eq!(
            records
                .iter()
                .map(|record| record.message.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(records[0].fields.get("attempt"), Some(&"1".to_owned()));
        assert_eq!(records[1].level, Level::Warn);
        assert_eq!(records[2].target, "xmtp_db");
        assert!(records.iter().all(|record| record.timestamp_ns > 0));
    }

    #[test]
    fn sink_replace_and_clear() {
        let first = Arc::new(Collect::default());
        let second = Arc::new(Collect::default());
        let native = NativeCount::default();
        let native_count = native.0.clone();
        let slot = SinkSlot::default();
        let subscriber = tracing_subscriber::registry()
            .with(slot.clone())
            .with(native);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "xmtp_mls", "before");
            slot.set_sink(Some(first.clone()));
            tracing::info!(target: "xmtp_mls", "first");
            slot.set_sink(Some(second.clone()));
            tracing::info!(target: "xmtp_mls", "second");
            slot.set_sink(None);
            tracing::info!(target: "xmtp_mls", "after");
        });
        assert_eq!(native_count.load(Ordering::Relaxed), 4);
        assert_eq!(first.0.lock()[0].message, "first");
        assert_eq!(first.0.lock().len(), 1);
        assert_eq!(second.0.lock()[0].message, "second");
        assert_eq!(second.0.lock().len(), 1);
    }

    struct BlockingTarget {
        entered: Sender<()>,
        release: parking_lot::Mutex<Receiver<()>>,
        output: Sender<LogRecord>,
        first: AtomicBool,
    }

    impl LogSinkTarget for BlockingTarget {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            if self.first.swap(false, Ordering::SeqCst) {
                self.entered.send(()).unwrap();
                self.release.lock().recv().unwrap();
            }
            self.output.send(record).unwrap();
            Ok(())
        }
    }

    #[test]
    fn bounded_sink_counts_drops() {
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (output_tx, output_rx) = channel();
        let target = Arc::new(BlockingTarget {
            entered: entered_tx,
            release: parking_lot::Mutex::new(release_rx),
            output: output_tx,
            first: AtomicBool::new(true),
        });
        let bounded = BoundedSink::new(target).unwrap();
        bounded.on_record(record("first")).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for _ in 0..BOUNDED_SINK_CAPACITY {
            bounded.on_record(record("waiting")).unwrap();
        }
        bounded.on_record(record("dropped")).unwrap();
        assert_eq!(bounded.dropped_count(), 1);
        release_tx.send(()).unwrap();
        assert_eq!(
            output_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .dropped_records,
            0
        );
        assert_eq!(
            output_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .dropped_records,
            1
        );
        assert_eq!(bounded.error_count(), 0);
    }

    struct Failing(Arc<AtomicUsize>);

    impl LogSinkTarget for Failing {
        fn on_record(&self, _record: LogRecord) -> Result<(), SinkError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(Box::new(std::io::Error::other("sink failed")))
        }
    }

    #[test]
    fn sink_error_never_reaches_sink() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = SinkSlot::default();
        slot.set_sink(Some(Arc::new(Failing(calls.clone()))));
        let native = NativeCount::default();
        let native_count = native.0.clone();
        let subscriber = tracing_subscriber::registry()
            .with(slot.clone())
            .with(native);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "xmtp_mls", "one call");
        });
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(slot.error_count(), 1);
        assert!(native_count.load(Ordering::Relaxed) >= 1);
    }

    struct Panicking;

    impl LogSinkTarget for Panicking {
        fn on_record(&self, _record: LogRecord) -> Result<(), SinkError> {
            panic!("sink panic");
        }
    }

    #[test]
    fn sink_panic_is_contained() {
        let slot = SinkSlot::default();
        slot.set_sink(Some(Arc::new(Panicking)));
        let result = catch_unwind(AssertUnwindSafe(|| {
            tracing::subscriber::with_default(
                tracing_subscriber::registry().with(slot.clone()),
                || {
                    tracing::info!(target: "xmtp_mls", "one call");
                },
            );
        }));
        assert!(result.is_ok());
        assert_eq!(slot.error_count(), 1);
    }
}
