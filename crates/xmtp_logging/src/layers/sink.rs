//! A replaceable event sink and an optional bounded delivery queue.

#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    cell::Cell,
    collections::BTreeMap,
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::mpsc::{SyncSender, TrySendError, sync_channel},
    thread,
};

#[cfg(not(target_arch = "wasm32"))]
use parking_lot::Mutex;
use parking_lot::RwLock;
use tracing::{
    Event,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

use crate::Level;

/// The queue size used by a Node log sink.
#[cfg(not(target_arch = "wasm32"))]
pub const BOUNDED_SINK_CAPACITY: usize = 4_096;

#[cfg(target_arch = "wasm32")]
fn timestamp_ns() -> i64 {
    (js_sys::Date::now() * 1_000_000.0) as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn timestamp_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn timestamp_seconds() -> u64 {
    u64::try_from(timestamp_ns() / 1_000_000_000).unwrap_or_default()
}

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

/// The sink rejected a record because its delivery window is full.
#[derive(Debug, thiserror::Error)]
#[error("log sink is busy")]
pub struct SinkBusy;

/// A destination for log records. A direct sink is called on the logging thread.
/// A sink must return errors instead of panicking: release builds abort on panic.
pub trait LogSinkTarget: Send + Sync + 'static {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError>;

    /// Stop pending delivery when this sink is replaced or cleared.
    fn on_detach(&self) {}
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
    let now = timestamp_seconds();
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

fn deliver(target: &dyn LogSinkTarget, record: LogRecord, errors: &AtomicU64, dropped: &AtomicU64) {
    match catch_unwind(AssertUnwindSafe(|| target.on_record(record))) {
        Ok(Ok(())) => {}
        Ok(Err(error)) if error.is::<SinkBusy>() => {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
        Ok(Err(_)) => report_error(errors, "returned an error"),
        Err(_) => report_error(errors, "panicked"),
    }
}

#[derive(Default)]
struct SinkState {
    target: RwLock<Option<Arc<dyn LogSinkTarget>>>,
    errors: AtomicU64,
    dropped: AtomicU64,
}

/// Always-present layer slot. Replacing or dropping a sink never calls it under
/// the slot lock.
#[derive(Clone, Default)]
pub(crate) struct SinkSlot(Arc<SinkState>);

impl SinkSlot {
    pub(crate) fn set_sink(&self, target: Option<Arc<dyn LogSinkTarget>>) {
        let old = {
            let mut current = self.0.target.write();
            if current
                .as_ref()
                .zip(target.as_ref())
                .is_some_and(|(old, new)| Arc::ptr_eq(old, new))
            {
                return;
            }
            std::mem::replace(&mut *current, target)
        };
        if let Some(old) = old.as_ref() {
            old.on_detach();
        }
        drop(old);
    }

    pub(crate) fn error_count(&self) -> u64 {
        self.0.errors.load(Ordering::Relaxed)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn dropped_count(&self) -> u64 {
        self.0.dropped.load(Ordering::Relaxed)
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
        deliver(target.as_ref(), record, &self.0.errors, &self.0.dropped);
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
        self.push(field.name(), format!("{value:?}"));
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
        Self {
            level,
            target: metadata.target().to_owned(),
            message: visitor.message,
            fields: visitor.fields,
            timestamp_ns: timestamp_ns(),
            dropped_records: 0,
        }
    }
}

/// Send records through one bounded queue to one drain thread.
///
/// The drain thread owns no queue or slot lock while it calls the destination.
/// Detaching or dropping the adapter closes its queue without waiting for an
/// active callback. Queued records are counted as drops and are not delivered.
#[cfg(not(target_arch = "wasm32"))]
pub struct BoundedSink {
    sender: Mutex<Option<SyncSender<LogRecord>>>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
    dropped: Arc<AtomicU64>,
    pending_drops: Arc<Mutex<u64>>,
    errors: Arc<AtomicU64>,
}

#[cfg(not(target_arch = "wasm32"))]
impl BoundedSink {
    pub fn new(target: Arc<dyn LogSinkTarget>) -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel::<LogRecord>(BOUNDED_SINK_CAPACITY);
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let pending_drops = Arc::new(Mutex::new(0));
        let errors = Arc::new(AtomicU64::new(0));
        let worker_drops = pending_drops.clone();
        let worker_errors = errors.clone();
        let worker_stopped = stopped.clone();
        let worker_discarded = dropped.clone();
        thread::Builder::new()
            .name("xmtp-log-sink".to_owned())
            .spawn(move || {
                while let Ok(mut record) = receiver.recv() {
                    if worker_stopped.load(Ordering::Acquire) {
                        worker_discarded.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    let missed = std::mem::take(&mut *worker_drops.lock());
                    record.dropped_records = record.dropped_records.saturating_add(missed);
                    if worker_stopped.load(Ordering::Acquire) {
                        worker_discarded.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    if let Some(_guard) = SinkCallGuard::enter() {
                        deliver(target.as_ref(), record, &worker_errors, &worker_discarded);
                    }
                }
            })?;
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            stopped,
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

    /// Stop this adapter. The current callback can finish; queued records are
    /// discarded and the drain thread exits after that callback returns.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        drop(self.sender.lock().take());
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for BoundedSink {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LogSinkTarget for BoundedSink {
    fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
        let send_result = self
            .sender
            .lock()
            .as_ref()
            .map(|sender| sender.try_send(record));
        match send_result {
            None => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Some(Ok(())) => Ok(()),
            Some(Err(TrySendError::Full(_))) => {
                let mut pending = self.pending_drops.lock();
                *pending = pending.saturating_add(1);
                self.dropped.fetch_add(1, Ordering::Relaxed);
                // The first subsequent record *delivered* carries this count,
                // including one that was already waiting in the queue.
                Ok(())
            }
            Some(Err(TrySendError::Disconnected(_))) => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "log sink drain thread stopped",
            ))),
        }
    }

    fn on_detach(&self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize},
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
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
    struct NativeRecords(Arc<parking_lot::Mutex<Vec<LogRecord>>>);

    impl<S: tracing::Subscriber> Layer<S> for NativeRecords {
        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            self.0.lock().push(LogRecord::from_event(event));
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
    fn quoted_message_keeps_quotes() {
        let sink = Arc::new(Collect::default());
        let slot = SinkSlot::default();
        slot.set_sink(Some(sink.clone()));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(slot), || {
            tracing::info!(target: "xmtp_mls", "\"{}\"", "id");
        });
        assert_eq!(sink.0.lock()[0].message, "\"id\"");
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

    struct LogsOnDrop(Arc<AtomicBool>);

    impl Drop for LogsOnDrop {
        fn drop(&mut self) {
            tracing::info!(target: "xmtp_common", "old sink dropped");
            self.0.store(true, Ordering::SeqCst);
        }
    }

    impl LogSinkTarget for LogsOnDrop {
        fn on_record(&self, _record: LogRecord) -> Result<(), SinkError> {
            Ok(())
        }
    }

    #[test]
    fn replace_sink_whose_drop_logs() {
        let handle = Arc::new(
            crate::XmtpLogging::builder()
                .level(Level::Info)
                .with_native(true)
                .install()
                .unwrap(),
        );
        let dropped = Arc::new(AtomicBool::new(false));
        handle.set_sink(Some(Arc::new(LogsOnDrop(dropped.clone()))));
        let replacement = Arc::new(Collect::default());
        let (done_tx, done_rx) = channel();
        let worker = thread::spawn({
            let replacement = replacement.clone();
            move || {
                handle.set_sink(Some(replacement));
                done_tx.send(()).unwrap();
            }
        });
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(replacement.0.lock()[0].message, "old sink dropped");
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

    #[test]
    fn replaced_bounded_sink_stops_delivering() {
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (output_tx, output_rx) = channel();
        let old_target = Arc::new(BlockingTarget {
            entered: entered_tx,
            release: parking_lot::Mutex::new(release_rx),
            output: output_tx,
            first: AtomicBool::new(true),
        });
        let old = Arc::new(BoundedSink::new(old_target).unwrap());
        let slot = SinkSlot::default();
        slot.set_sink(Some(old.clone()));
        old.on_record(record("in flight")).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for _ in 0..BOUNDED_SINK_CAPACITY {
            old.on_record(record("queued")).unwrap();
        }
        old.on_record(record("overflow")).unwrap();
        assert_eq!(old.dropped_count(), 1);

        let replacement = Arc::new(Collect::default());
        slot.set_sink(Some(replacement.clone()));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(slot), || {
            tracing::info!(target: "xmtp_mls", "new sink");
        });
        assert_eq!(replacement.0.lock()[0].message, "new sink");

        release_tx.send(()).unwrap();
        assert_eq!(
            output_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .message,
            "in flight"
        );
        assert!(matches!(
            output_rx.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        ));
        assert_eq!(old.dropped_count(), BOUNDED_SINK_CAPACITY as u64 + 1);
    }

    struct Failing(Arc<parking_lot::Mutex<Vec<String>>>);

    impl LogSinkTarget for Failing {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record.message.clone());
            if record.message.starts_with("original") {
                tracing::warn!(target: "xmtp_common", "sink callback log");
                Err(Box::new(std::io::Error::other("sink failed")))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn sink_error_never_reaches_sink() {
        let calls = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let slot = SinkSlot::default();
        slot.set_sink(Some(Arc::new(Failing(calls.clone()))));
        let native = NativeRecords::default();
        let native_records = native.0.clone();
        let subscriber = tracing_subscriber::registry()
            .with(slot.clone())
            .with(native);
        tracing::subscriber::set_global_default(subscriber).unwrap();
        tracing::info!(target: "xmtp_mls", "original one");
        tracing::info!(target: "xmtp_mls", "original two");
        assert_eq!(&*calls.lock(), &["original one", "original two"]);
        assert_eq!(slot.error_count(), 2);
        assert_eq!(
            native_records
                .lock()
                .iter()
                .filter(|record| {
                    record.level == Level::Error
                        && record.target == "xmtp_common"
                        && record.message == "log sink returned an error"
                })
                .count(),
            1
        );
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
