//! Behavioral cover for `bind_task_hub` (and the `#[err_span]` wrap that uses it):
//! per-task breadcrumb isolation, and inner spans staying children of the FFI
//! transaction rather than re-rooting as transactions of their own.
#![cfg(not(target_arch = "wasm32"))]

use sentry_core::protocol::EnvelopeItem;
use sentry_core::test::TestTransport;
use sentry_core::{Breadcrumb, Client, ClientOptions, Hub, Level};
use std::sync::Arc;
use tracing_subscriber::prelude::*;

/// Binds a capturing client to *this thread's* hub. `Hub::current()` is
/// thread-local and libtest gives each test its own thread, so tests running in
/// parallel never see each other's envelopes.
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

    let mut transactions: Vec<(String, usize)> = Vec::new();
    for envelope in transport.fetch_and_clear_envelopes() {
        for item in envelope.items() {
            if let EnvelopeItem::Transaction(tx) = item {
                transactions.push((tx.name.clone().unwrap_or_default(), tx.spans.len()));
            }
        }
    }

    // Forking from `Hub::main()` instead yields two parentless transactions
    // ("ffi_op" with 0 spans and "mls.inner_op" with 0 spans).
    assert_eq!(
        transactions.len(),
        1,
        "inner span re-rooted as its own transaction: {transactions:?}"
    );
    assert_eq!(transactions[0].0, "ffi_op");
    assert_eq!(
        transactions[0].1, 1,
        "expected mls.inner_op as a child span: {transactions:?}"
    );
}
