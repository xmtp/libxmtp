//! Behavioral cover for `bind_task_hub` (and the `#[err_span]` wrap that uses it):
//! per-task breadcrumb isolation, inner spans staying children of the FFI
//! transaction rather than re-rooting as transactions of their own, and the
//! FFI error event carrying the trail its own call accumulated.
#![cfg(not(target_arch = "wasm32"))]

use sentry_core::protocol::EnvelopeItem;
use sentry_core::test::TestTransport;
use sentry_core::{Breadcrumb, Client, ClientOptions, Hub, Level};
use std::future::Future;
use std::sync::Arc;
use tracing_subscriber::prelude::*;

/// Binds a capturing client to *this thread's* hub. A thread's hub is forked
/// client-less off `Hub::main()` on first touch, so the client each test binds
/// afterwards belongs to that test alone — including the `bind_task_hub` forks
/// it then makes, which copy the thread hub's top scope. Tests therefore have to
/// stay on their own thread (`futures::executor::block_on`, never a worker pool).
fn bind_test_client() -> Arc<TestTransport> {
    let (client, transport) = test_client();
    Hub::current().bind_client(Some(client));
    transport
}

fn test_client() -> (Arc<Client>, Arc<TestTransport>) {
    let transport = TestTransport::new();
    let options = ClientOptions::new()
        .dsn("https://public@example.com/1")
        .transport(transport.clone())
        .traces_sample_rate(1.0);
    (Arc::new(Client::from(options)), transport)
}

/// Suspends once so `futures::future::join` interleaves the two tasks; a hub
/// that only survived a single poll would not be caught otherwise.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}

fn crumb(message: String) {
    sentry_core::add_breadcrumb(Breadcrumb {
        message: Some(message),
        ..Default::default()
    });
}

/// Mirrors what `#[err_span]` expands an async FFI body into.
async fn bound_task(tag: &'static str) {
    xmtp_common::bind_task_hub(async move {
        crumb(format!("{tag}-1"));
        yield_once().await;
        crumb(format!("{tag}-2"));
        yield_once().await;
        sentry_core::capture_message(&format!("{tag}-error"), Level::Error);
    })
    .await
}

#[xmtp_common::test(unwrap_try = true)]
fn concurrent_tasks_keep_their_own_breadcrumb_trails() {
    let _slot = lock_main_hub();
    let transport = bind_test_client();
    futures::executor::block_on(futures::future::join(bound_task("A"), bound_task("B")));

    let events = transport.fetch_and_clear_events();
    assert_eq!(events.len(), 2, "expected one event per task: {events:?}");
    for tag in ["A", "B"] {
        let event = events
            .iter()
            .find(|e| e.message.as_deref() == Some(&format!("{tag}-error")))
            .unwrap_or_else(|| panic!("no {tag}-error event: {events:?}"));
        let crumbs: Vec<&str> = event
            .breadcrumbs
            .iter()
            .filter_map(|b| b.message.as_deref())
            .collect();
        // Exact, not just "no crossing": with the hub binding removed both tasks
        // share one scope and every event carries all four crumbs — and an
        // implementation that dropped crumbs entirely would leave this empty.
        assert_eq!(
            crumbs,
            [format!("{tag}-1"), format!("{tag}-2")],
            "{tag} event carries the wrong breadcrumb trail"
        );
    }
}

#[xmtp_common::mls_span]
async fn inner_op() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::err_span]
async fn ffi_op() -> Result<(), std::io::Error> {
    inner_op().await
}

#[xmtp_common::test(unwrap_try = true)]
fn err_span_hub_keeps_inner_spans_under_the_ffi_transaction() {
    let _slot = lock_main_hub();
    let transport = bind_test_client();
    let layer = sentry_tracing::layer()
        .span_filter(|meta| meta.fields().field("sentry.op").is_some())
        .event_filter(|meta| match *meta.level() {
            tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
            _ => sentry_tracing::EventFilter::Breadcrumb,
        });
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        futures::executor::block_on(ffi_op()).unwrap();
    });

    let envelopes = transport.fetch_and_clear_envelopes();
    let transactions: Vec<_> = envelopes
        .iter()
        .flat_map(|envelope| envelope.items())
        .filter_map(|item| match item {
            EnvelopeItem::Transaction(tx) => Some(tx),
            _ => None,
        })
        .collect();

    // Forking from `Hub::main()` instead yields two parentless transactions
    // ("ffi_op" with 0 spans and "mls.inner_op" with 0 spans).
    assert_eq!(
        transactions.len(),
        1,
        "inner span re-rooted as its own transaction: {transactions:?}"
    );
    assert_eq!(transactions[0].name.as_deref(), Some("ffi_op"));
    // `#[mls_span]`'s sentry.op/sentry.name land on the child's `op`/`description`,
    // so this pins its identity rather than just "some span arrived".
    let children: Vec<_> = transactions[0]
        .spans
        .iter()
        .map(|s| (s.op.as_deref(), s.description.as_deref()))
        .collect();
    assert_eq!(
        children,
        [(Some("mls"), Some("mls.inner_op"))],
        "expected mls.inner_op as the one child span: {transactions:?}"
    );
}

/// Serializes the tests in this binary and restores the process hub's client on
/// drop, so a test that binds one there (or panics part-way through) cannot leave
/// it behind for the rest of the binary. Serialization matters because
/// `bind_task_hub` tracks the process hub's client: under `cargo test` the tests
/// share a process, and one binding a client to `Hub::main()` would otherwise
/// reach into another's task hubs.
struct MainHubSlot {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<Arc<Client>>,
}

impl Drop for MainHubSlot {
    fn drop(&mut self) {
        Hub::main().bind_client(self.previous.take());
    }
}

fn lock_main_hub() -> MainHubSlot {
    static MAIN_HUB: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = MAIN_HUB.lock().unwrap_or_else(|e| e.into_inner());
    MainHubSlot {
        _lock,
        previous: Hub::main().client(),
    }
}

/// The late-enable case: a long-lived worker future is bound to its task hub
/// *before* any client exists anywhere, so the fork is a client-less snapshot and
/// `Hub::main()` has nothing to hand it either. Enabling afterwards happens on a
/// different thread and binds the client to that thread's hub *and* to
/// `Hub::main()` — what `build_sentry_layer` does. Nothing propagates that to the
/// already-forked task hub except `bind_task_hub`'s per-poll refresh: without it
/// `Hub::capture_event` returns early on a client-less hub and the event is
/// silently dropped.
#[xmtp_common::test(unwrap_try = true)]
fn task_hub_bound_before_enable_picks_up_a_late_client() {
    let restore = lock_main_hub();
    assert!(
        restore.previous.is_none() && Hub::current().client().is_none(),
        "precondition: this test starts from a client-less process and thread hub"
    );

    // Bound now, while no client exists: the fork cannot inherit one, and the
    // process hub has nothing to hand it either.
    let task = xmtp_common::bind_task_hub(async {
        crumb("late-1".to_string());
        yield_once().await;
        crumb("late-2".to_string());
        yield_once().await;
        sentry_core::capture_message("late-error", Level::Error);
    });

    // "Enable" from another thread, the way `build_sentry_layer` now does it:
    // `sentry::init` binds the calling thread's hub, and we propagate to the
    // process hub so hubs forked later — and this already-forked one — can find it.
    let transport = std::thread::spawn(|| {
        let (client, transport) = test_client();
        Hub::current().bind_client(Some(client));
        Hub::main().bind_client(Hub::current().client());
        transport
    })
    .join()
    .expect("enable thread");

    futures::executor::block_on(task);

    let events = transport.fetch_and_clear_events();
    let event = events
        .iter()
        .find(|e| e.message.as_deref() == Some("late-error"))
        .unwrap_or_else(|| {
            panic!("late-enabled client never reached the pre-bound task hub: {events:?}")
        });
    let crumbs: Vec<&str> = event
        .breadcrumbs
        .iter()
        .filter_map(|b| b.message.as_deref())
        .collect();
    assert_eq!(
        crumbs,
        ["late-1", "late-2"],
        "task hub adopted the client but lost its own trail"
    );
    drop(restore);
}

/// Drives `fut` one poll with a no-op waker, so the test controls what the
/// process hub looks like between polls. Panics if the future finishes early.
fn poll_once<F: Future>(fut: &mut std::pin::Pin<&mut F>) {
    let waker = futures::task::noop_waker();
    let polled = fut
        .as_mut()
        .poll(&mut std::task::Context::from_waker(&waker));
    assert!(
        polled.is_pending(),
        "future finished before the test drove it"
    );
}

/// The re-enable case, which the host exposes over FFI: `enable_sentry` ->
/// `disable_sentry` -> `enable_sentry` puts a *second*, different client on the
/// process hub. Long-lived task hubs (workers, watchdog, streams) are still alive
/// across all three, so an adoption that latches on presence keeps captures going
/// to the closed first client forever — a silent, unrecoverable drop. Adoption
/// keyed on client *identity* moves them onto the new one, and the disabled gap
/// in the middle reports nowhere at all.
#[xmtp_common::test(unwrap_try = true)]
fn task_hub_follows_a_disable_then_re_enable() {
    let restore = lock_main_hub();
    let (client_a, transport_a) = test_client();
    Hub::main().bind_client(Some(client_a));

    let task = xmtp_common::bind_task_hub(async {
        sentry_core::capture_message("first", Level::Error);
        yield_once().await;
        sentry_core::capture_message("while-off", Level::Error);
        yield_once().await;
        sentry_core::capture_message("second", Level::Error);
        // Never completes: the test drives it poll by poll and asserts on the
        // transports, so it stays a live task hub throughout.
        std::future::pending::<()>().await;
    });
    futures::pin_mut!(task);

    poll_once(&mut task);

    // `disable_sentry`: the process hub gives its client back (here, to nothing).
    Hub::main().bind_client(None);
    poll_once(&mut task);

    // `enable_sentry` again — a brand new client, not the one A was.
    let (client_b, transport_b) = test_client();
    Hub::main().bind_client(Some(client_b));
    poll_once(&mut task);

    let messages = |t: &Arc<TestTransport>| -> Vec<String> {
        t.fetch_and_clear_events()
            .iter()
            .filter_map(|e| e.message.clone())
            .collect()
    };
    assert_eq!(
        messages(&transport_a),
        ["first"],
        "the first client kept receiving after it was replaced"
    );
    assert_eq!(
        messages(&transport_b),
        ["second"],
        "the re-enabled client never reached the still-running task hub"
    );
    drop(restore);
}

#[xmtp_common::err_span]
async fn ffi_failing_op() -> Result<(), std::io::Error> {
    crumb("E-1".to_string());
    yield_once().await;
    crumb("E-2".to_string());
    Err(std::io::Error::other("boom"))
}

/// The error event an `#[err_span]` fn emits is the one Sentry promotes to an
/// issue, so it has to fire while the task hub is still bound — otherwise the
/// trail the call accumulated is on a hub nobody looks at.
#[xmtp_common::test(unwrap_try = true)]
fn err_span_error_event_carries_the_task_hub_trail() {
    let _slot = lock_main_hub();
    let transport = bind_test_client();
    let layer = sentry_tracing::layer()
        .span_filter(|meta| meta.fields().field("sentry.op").is_some())
        .event_filter(|meta| match *meta.level() {
            tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
            _ => sentry_tracing::EventFilter::Ignore,
        });
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        futures::executor::block_on(ffi_failing_op()).unwrap_err();
    });

    let events = transport.fetch_and_clear_events();
    // Exactly one: the manual event replaces `instrument(err)`'s, never doubles it.
    assert_eq!(
        events.len(),
        1,
        "expected exactly one FFI error event: {events:?}"
    );
    // `#[err_span]` names the event after the fn, with no format-args wrapping.
    assert_eq!(
        events[0].message.as_deref(),
        Some("ffi_failing_op"),
        "unexpected FFI error event message: {events:?}"
    );
    let crumbs: Vec<&str> = events[0]
        .breadcrumbs
        .iter()
        .filter_map(|b| b.message.as_deref())
        .collect();
    assert_eq!(
        crumbs,
        ["E-1", "E-2"],
        "error event captured off the task hub, without its trail"
    );
    assert!(
        events[0].contexts.contains_key("trace"),
        "error event lost the FFI span's trace context: {events:?}"
    );
}
