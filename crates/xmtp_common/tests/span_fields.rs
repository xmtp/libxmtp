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

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("private-span-error:{0}")]
struct SpanFailure(String);

#[xmtp_common::db_span(redact_error)]
fn redacted_db(mode: u8) -> Result<u8, SpanFailure> {
    if mode == 1 {
        return Err(SpanFailure("early".into()));
    }
    if mode == 2 {
        Err::<(), _>(SpanFailure("question".into()))?;
    }
    Ok(7)
}

#[xmtp_common::mls_span(redact_error)]
async fn redacted_mls(mode: u8) -> Result<u8, SpanFailure> {
    tokio::task::yield_now().await;
    if mode == 1 {
        return Err(SpanFailure("early".into()));
    }
    if mode == 2 {
        Err::<(), _>(SpanFailure("question".into()))?;
    }
    Ok(7)
}

#[xmtp_common::db_span]
fn default_error_span() -> Result<(), SpanFailure> {
    Err(SpanFailure("default".into()))
}

#[xmtp_common::test(unwrap_try = true)]
async fn redacted_span_errors_keep_status_and_results() {
    use xmtp_logging::test_logging::string_attribute;

    let cap = FieldCapture::default();
    let collector = xmtp_logging::test_logging::OtlpCollector::start().await?;
    let (trace, logs, guard) = xmtp_logging::init(xmtp_logging::TelemetryConfig {
        endpoint: Some(collector.endpoint()),
        logs: false,
        ..Default::default()
    })?;
    let subscriber =
        tracing_subscriber::registry().with(vec![cap.clone().boxed(), trace.boxed(), logs]);
    use tracing::instrument::WithSubscriber;
    async {
        assert_eq!(redacted_db(0), Ok(7));
        assert_eq!(redacted_db(1), Err(SpanFailure("early".into())));
        assert_eq!(redacted_db(2), Err(SpanFailure("question".into())));
        assert_eq!(redacted_mls(0).await, Ok(7));
        assert_eq!(redacted_mls(1).await, Err(SpanFailure("early".into())));
        assert_eq!(redacted_mls(2).await, Err(SpanFailure("question".into())));
        assert_eq!(default_error_span(), Err(SpanFailure("default".into())));
    }
    .with_subscriber(tracing::Dispatch::new(subscriber))
    .await;
    tokio::task::spawn_blocking(move || guard.shutdown()).await?;
    let exported: Vec<_> = collector
        .take_spans()
        .into_iter()
        .flat_map(|resource| resource.scope_spans)
        .flat_map(|scope| scope.spans)
        .collect();
    // OTLP StatusCode::Error has wire value 2.
    const ERROR_STATUS: i32 = 2;
    for (name, prefix) in [("redacted_db", "db"), ("redacted_mls", "mls")] {
        let operation = format!("{prefix}.{name}");
        let matching: Vec<_> = exported
            .iter()
            .filter(|span| span.name == operation)
            .collect();
        assert_eq!(matching.len(), 3);
        let mut errors = 0;
        let mut successes = 0;
        for span in matching {
            assert_eq!(
                string_attribute(&span.attributes, "operation"),
                Some(operation.as_str())
            );
            assert_eq!(
                string_attribute(&span.attributes, "sentry.op"),
                Some(prefix)
            );
            assert_eq!(
                string_attribute(&span.attributes, "sentry.name"),
                Some(operation.as_str())
            );
            if span
                .status
                .as_ref()
                .is_some_and(|status| status.code == ERROR_STATUS)
            {
                errors += 1;
                assert_eq!(span.events.len(), 1);
                assert_eq!(
                    string_attribute(&span.events[0].attributes, "level"),
                    Some("ERROR")
                );
                assert_eq!(
                    string_attribute(&span.events[0].attributes, "exception.message"),
                    Some("\"operation failed\"")
                );
            } else {
                successes += 1;
                assert!(span.events.is_empty());
            }
        }
        assert_eq!((errors, successes), (2, 1));
        let fields = cap.0.lock().unwrap();
        let fields = fields.get(name).expect("operation span recorded");
        assert_eq!(fields.len(), 4);
        for field in ["operation", "sentry.op", "sentry.name", "otel.name"] {
            assert!(fields.iter().any(|name| name == field));
        }
    }
    let default = exported
        .iter()
        .find(|span| span.name == "db.default_error_span")
        .expect("default span exported");
    assert_eq!(
        default.status.as_ref().expect("default error status").code,
        ERROR_STATUS
    );
    assert_eq!(
        string_attribute(&default.events[0].attributes, "exception.message"),
        Some("private-span-error:default")
    );
}
