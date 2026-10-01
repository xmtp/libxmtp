use super::*;
use std::collections::BTreeMap;

fn record(index: usize) -> LogRecord {
    LogRecord {
        level: crate::Level::Info,
        target: "xmtp_sdk".into(),
        message: index.to_string(),
        fields: BTreeMap::new(),
        timestamp_ns: 0,
        dropped_records: 0,
    }
}

fn next<T: ?Sized>(queue: &SinkQueue<T>) -> SinkDispatch<T> {
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    match queue.poll_next(&mut cx) {
        Poll::Ready(call) => call,
        Poll::Pending => panic!("no queued record"),
    }
}

fn pending<T: ?Sized>(queue: &SinkQueue<T>) {
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    assert!(queue.poll_next(&mut cx).is_pending());
}

// verifies: LOG-002, LOG-003, LOG-004, LOG-005
#[test]
fn capacity_includes_transport_until_handoff() {
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new("sink")));
    queue.push(record(0));
    let first = next(&queue);
    for i in 1..=SINK_QUEUE_CAPACITY {
        queue.push(record(i));
    }
    assert_eq!(queue.dropped_count(), 1);
    pending(&queue);
    assert!(queue.handoff());
    queue.push(record(SINK_QUEUE_CAPACITY + 1));
    queue.push(record(SINK_QUEUE_CAPACITY + 2));
    assert_eq!(queue.dropped_count(), 2);
    queue.complete(first, true);
    for i in 1..SINK_QUEUE_CAPACITY {
        let call = next(&queue);
        assert_eq!(call.record.message, i.to_string());
        assert_eq!(call.record.dropped_records, if i == 1 { 2 } else { 0 });
        assert!(queue.handoff());
        queue.complete(call, true);
    }
    let last = next(&queue);
    assert_eq!(last.record.message, (SINK_QUEUE_CAPACITY + 1).to_string());
    queue.complete(last, true);
    pending(&queue);
}

// verifies: LOG-013
#[test]
fn failure_keeps_captured_drops_and_success_keeps_later_drops() {
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new("sink")));
    for i in 0..SINK_QUEUE_CAPACITY + 3 {
        queue.push(record(i));
    }
    let first = next(&queue);
    assert_eq!(first.record.dropped_records, 3);
    // These drops occur after dispatch but before handoff.
    queue.push(record(5000));
    queue.push(record(5001));
    assert!(queue.handoff());
    queue.complete(first, false);
    let failed_report = next(&queue);
    assert_eq!(failed_report.record.dropped_records, 5);
    // Fill the slots freed by the two admitted records, then overflow once.
    queue.push(record(5002));
    queue.push(record(5003));
    assert!(queue.handoff());
    queue.complete(failed_report, true);
    let later = next(&queue);
    assert_eq!(later.record.dropped_records, 1);
}

// verifies: LOG-002, LOG-007, LOG-011, LOG-012
#[test]
fn hundred_replacements_wait_for_only_one_old_callback() {
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new(0)));
    queue.push(record(0));
    let old = next(&queue);
    assert!(queue.handoff());
    for generation in 1..=100 {
        queue.replace(None);
        queue.replace(Some(Arc::new(generation)));
        for i in 0..SINK_QUEUE_CAPACITY + 1 {
            queue.push(record(i));
        }
        pending(&queue);
    }
    queue.complete(old, true);
    let current = next(&queue);
    assert_eq!(*current.target, 100);
    assert_eq!(current.record.message, "0");
    assert_eq!(current.record.dropped_records, 1);
}

// verifies: LOG-003, LOG-007, LOG-011, LOG-012
#[test]
fn replacement_rejects_delayed_handoff_and_keeps_transport_credit() {
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new("old")));
    for i in 0..SINK_QUEUE_CAPACITY + 7 {
        queue.push(record(i));
    }
    let old = next(&queue);
    assert_eq!(old.record.dropped_records, 7);
    queue.replace(Some(Arc::new("new")));
    for i in 0..SINK_QUEUE_CAPACITY {
        queue.push(record(i));
    }
    assert!(!queue.handoff());
    queue.complete(old, false);
    assert_eq!(queue.error_count(), 0);
    let new = next(&queue);
    assert_eq!(*new.target, "new");
    assert_eq!(new.record.dropped_records, 1);
}

// verifies: LOG-004, LOG-008
#[test]
fn emission_from_plain_thread_completes_while_callback_waits() {
    let queue = Arc::new(SinkQueue::default());
    queue.replace(Some(Arc::new("sink")));
    queue.push(record(0));
    let active = next(&queue);
    assert!(queue.handoff());
    let emitter = queue.clone();
    std::thread::spawn(move || {
        emitter.push(record(1));
        emitter.replace(None);
    })
    .join()
    .unwrap();
    queue.complete(active, true);
    pending(&queue);
}

fn idle<T: ?Sized>(queue: &SinkQueue<T>) -> Poll<()> {
    let mut cx = Context::from_waker(futures::task::noop_waker_ref());
    queue.poll_idle(&mut cx)
}

#[test]
fn idle_waits_for_queue_transport_and_replaced_active_call() {
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new("old")));
    assert!(idle(&queue).is_ready());
    queue.push(record(0));
    assert!(idle(&queue).is_pending());
    let first = next(&queue);
    assert!(idle(&queue).is_pending());
    assert!(queue.handoff());
    queue.push(record(1));
    queue.replace(None);
    // Replacement clears queued records but does not finish a handed-off call.
    assert!(idle(&queue).is_pending());
    queue.complete(first, true);
    assert!(idle(&queue).is_ready());
}

#[derive(Default)]
struct Wakes(std::sync::atomic::AtomicUsize);

impl futures::task::ArcWake for Wakes {
    fn wake_by_ref(this: &Arc<Self>) {
        this.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn idle_waiter_does_not_replace_the_drain_waker() {
    use std::sync::atomic::Ordering;
    let queue = SinkQueue::default();
    queue.replace(Some(Arc::new("sink")));
    let drain = Arc::new(Wakes::default());
    let idle_waiter = Arc::new(Wakes::default());
    let drain_waker = futures::task::waker_ref(&drain);
    let idle_waker = futures::task::waker_ref(&idle_waiter);
    assert!(
        queue
            .poll_next(&mut Context::from_waker(&drain_waker))
            .is_pending()
    );
    assert!(
        queue
            .poll_idle(&mut Context::from_waker(&idle_waker))
            .is_ready()
    );
    queue.push(record(0));
    assert_eq!(drain.0.load(Ordering::SeqCst), 1);
    let first = next(&queue);
    assert!(
        queue
            .poll_idle(&mut Context::from_waker(&idle_waker))
            .is_pending()
    );
    queue.complete(first, true);
    assert_eq!(idle_waiter.0.load(Ordering::SeqCst), 1);
    assert!(idle(&queue).is_ready());
}

struct EmitsOnDrop {
    queue: std::sync::Weak<SinkQueue<EmitsOnDrop>>,
    emit: bool,
}

impl Drop for EmitsOnDrop {
    fn drop(&mut self) {
        if self.emit {
            self.queue.upgrade().unwrap().push(record(71));
        }
    }
}

#[test]
fn idle_includes_logs_from_a_retired_target_destructor() {
    let queue = Arc::new(SinkQueue::default());
    queue.replace(Some(Arc::new(EmitsOnDrop {
        queue: Arc::downgrade(&queue),
        emit: true,
    })));
    queue.push(record(0));
    let first = next(&queue);
    queue.replace(Some(Arc::new(EmitsOnDrop {
        queue: Arc::downgrade(&queue),
        emit: false,
    })));
    queue.complete(first, true);
    assert!(idle(&queue).is_pending());
    let second = next(&queue);
    assert_eq!(second.record.message, "71");
    queue.complete(second, true);
    assert!(idle(&queue).is_ready());
}
