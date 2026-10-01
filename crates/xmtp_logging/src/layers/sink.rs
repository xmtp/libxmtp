//! An event layer that sends records to a replaceable queue target.

use parking_lot::RwLock;
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tracing::{
    Event,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

use crate::Level;

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
    /// Pending drops captured when this record leaves the queue.
    /// A successful callback clears only the captured count.
    pub dropped_records: u64,
}

/// Error returned by a log sink.
pub type SinkError = Box<dyn Error + Send + Sync>;

/// A tracing event receiver. Application callbacks run in the SDK drain.
/// Implementations must enqueue without waiting for application code.
pub trait LogSinkTarget: Send + Sync + 'static {
    /// Skip record construction when this target has no receiver.
    fn enabled(&self) -> bool {
        true
    }

    fn on_record(&self, record: LogRecord) -> Result<(), SinkError>;
}

thread_local! {
    static SINK_CALL_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct SinkCallGuard;

impl SinkCallGuard {
    fn enter() -> Option<Self> {
        SINK_CALL_ACTIVE.with(|active| {
            if active.replace(true) {
                None
            } else {
                Some(Self)
            }
        })
    }
}

impl Drop for SinkCallGuard {
    fn drop(&mut self) {
        SINK_CALL_ACTIVE.with(|active| active.set(false));
    }
}

const SINK_ERROR_TARGET: &str = "xmtp_common::log_sink";

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
        // The sink layer excludes its own error target. Native layers retain it.
        tracing::error!(target: SINK_ERROR_TARGET, "log sink {detail}");
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
        drop(old);
    }

    pub(crate) fn error_count(&self) -> u64 {
        self.0.errors.load(Ordering::Relaxed)
    }
}

impl<S: tracing::Subscriber> Layer<S> for SinkSlot {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() == SINK_ERROR_TARGET {
            return;
        }
        let Some(target) = self.0.target.read().clone() else {
            return;
        };
        let Some(_guard) = SinkCallGuard::enter() else {
            return;
        };
        if !target.enabled() {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::{sync::mpsc::channel, thread, time::Duration};
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

    struct EmitsTwo(Arc<AtomicUsize>);

    impl LogSinkTarget for EmitsTwo {
        fn on_record(&self, _record: LogRecord) -> Result<(), SinkError> {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                tracing::info!(target: "xmtp_mls", "first nested sink event");
                tracing::info!(target: "xmtp_mls", "second nested sink event");
            }
            Ok(())
        }
    }

    // A scoped dispatch suppresses recursion itself. The fresh child tests the
    // production global dispatch path with exactly one subscriber.
    #[test]
    fn sink_target_logging_does_not_reenter() {
        const CHILD: &str = "XMTP_SINK_REENTRY_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let calls = Arc::new(AtomicUsize::new(0));
            let slot = SinkSlot::default();
            slot.set_sink(Some(Arc::new(EmitsTwo(calls.clone()))));
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(slot))
                .expect("fresh child must have no subscriber");
            tracing::info!(target: "xmtp_mls", "outer sink event");
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "layers::sink::tests::sink_target_logging_does_not_reenter",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "global sink child failed: {status}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("global sink child exceeded five seconds");
            }
            thread::sleep(Duration::from_millis(10));
        }
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
        let slot = SinkSlot::default();
        let subscriber = tracing_subscriber::registry().with(slot.clone());
        let dropped = Arc::new(AtomicBool::new(false));
        slot.set_sink(Some(Arc::new(LogsOnDrop(dropped.clone()))));
        let replacement = Arc::new(Collect::default());
        let (done_tx, done_rx) = channel();
        let worker = thread::spawn({
            let replacement = replacement.clone();
            move || {
                tracing::subscriber::with_default(subscriber, || {
                    slot.set_sink(Some(replacement));
                });
                done_tx.send(()).unwrap();
            }
        });
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(replacement.0.lock()[0].message, "old sink dropped");
    }

    struct Failing(Arc<parking_lot::Mutex<Vec<String>>>);

    impl LogSinkTarget for Failing {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record.message.clone());
            if record.message.starts_with("original") {
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
        let subscriber = tracing_subscriber::registry().with(slot.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "xmtp_mls", "original one");
            tracing::info!(target: "xmtp_mls", "original two");
        });
        assert_eq!(&*calls.lock(), &["original one", "original two"]);
        assert_eq!(slot.error_count(), 2);
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
