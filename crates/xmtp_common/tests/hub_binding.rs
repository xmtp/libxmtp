//! Behavioral cover for `bind_task_hub` (and the `#[err_span]` wrap that uses it):
//! per-task breadcrumb isolation, inner spans staying children of the FFI
//! transaction rather than re-rooting as transactions of their own, and the
//! FFI error event carrying the trail its own call accumulated.
#![cfg(not(target_arch = "wasm32"))]

use sentry_core::protocol::EnvelopeItem;
use sentry_core::test::TestTransport;
use sentry_core::{Breadcrumb, Client, ClientOptions, Hub, Level};
use std::sync::Arc;
use tracing_subscriber::prelude::*;

/// Binds a capturing client to *this thread's* hub. A thread's hub is forked
/// client-less off `Hub::main()` on first touch, so the client each test binds
/// afterwards belongs to that test alone — including the `bind_task_hub` forks
/// it then makes, which copy the thread hub's top scope. Tests therefore have to
/// stay on their own thread (`futures::executor::block_on`, never a worker pool).
fn bind_test_client() -> Arc<TestTransport> {
    let transport = TestTransport::new();
    let options = ClientOptions::new()
        .dsn("https://public@example.com/1")
        .transport(transport.clone())
        .traces_sample_rate(1.0);
    Hub::current().bind_client(Some(Arc::new(Client::from(options))));
    transport
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
