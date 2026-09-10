#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing_subscriber::prelude::*;

/// Records the declared field names of every span, keyed by span name, so each
/// macro's contract can be asserted independently.
#[derive(Default, Clone)]
struct FieldCapture(Arc<Mutex<HashMap<String, Vec<String>>>>);

impl<S> tracing_subscriber::Layer<S> for FieldCapture
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = attrs.metadata();
        let names: Vec<String> = meta.fields().iter().map(|f| f.name().to_string()).collect();
        self.0
            .lock()
            .unwrap()
            .insert(meta.name().to_string(), names);
    }
}

#[xmtp_common::mls_span]
fn sample_op() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::rpc_span]
fn sample_rpc() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::db_span]
fn sample_db() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::span(prefix = "stream")]
fn sample_custom() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::err_span]
async fn sample_ffi() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn span_macros_emit_sentry_fields() {
    let cap = FieldCapture::default();
    let collector = xmtp_logging::test_logging::OtlpCollector::start().await?;
    let (trace, logs, guard) = xmtp_logging::init(xmtp_logging::TelemetryConfig {
        endpoint: Some(collector.endpoint()),
        logs: false,
        ..Default::default()
    })?;
    let subscriber =
        tracing_subscriber::registry().with(vec![cap.clone().boxed(), trace.boxed(), logs]);
    // thread-local default; wins over the global subscriber `xmtp_common::test` installs
    tracing::subscriber::with_default(subscriber, || {
        let _ = sample_op();
        let _ = sample_rpc();
        let _ = sample_db();
        let _ = sample_custom();
        futures::executor::block_on(sample_ffi()).unwrap();
    });
    let spans = cap.0.lock().unwrap().clone();

    let op = spans
        .get("sample_op")
        .unwrap_or_else(|| panic!("no sample_op span: {spans:?}"));
    for expected in ["operation", "sentry.op", "sentry.name", "otel.name"] {
        assert!(
            op.contains(&expected.to_string()),
            "mls_span missing {expected}: {op:?}"
        );
    }

    let ffi = spans
        .get("sample_ffi")
        .unwrap_or_else(|| panic!("no sample_ffi span: {spans:?}"));
    for expected in ["sentry.op", "sentry.name"] {
        assert!(
            ffi.contains(&expected.to_string()),
            "err_span missing {expected}: {ffi:?}"
        );
    }
    assert!(
        !ffi.contains(&"operation".to_string()),
        "err_span must not emit `operation` (FFI calls are not a Collector metric dimension): {ffi:?}"
    );
    tokio::task::spawn_blocking(move || guard.shutdown()).await?;
    let exported: Vec<_> = collector
        .take_spans()
        .into_iter()
        .flat_map(|resource| resource.scope_spans)
        .flat_map(|scope| scope.spans)
        .collect();
    for operation in [
        "mls.sample_op",
        "rpc.sample_rpc",
        "db.sample_db",
        "stream.sample_custom",
    ] {
        let span = exported
            .iter()
            .find(|span| span.name == operation)
            .unwrap_or_else(|| panic!("missing exported operation {operation}: {exported:?}"));
        assert_eq!(
            xmtp_logging::test_logging::string_attribute(&span.attributes, "operation"),
            Some(operation)
        );
    }
}
