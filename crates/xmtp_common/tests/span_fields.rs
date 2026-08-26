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

#[xmtp_common::err_span]
async fn sample_ffi() -> Result<(), std::io::Error> {
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
fn span_macros_emit_sentry_fields() {
    let cap = FieldCapture::default();
    let subscriber = tracing_subscriber::registry().with(cap.clone());
    // thread-local default; wins over the global subscriber `xmtp_common::test` installs
    tracing::subscriber::with_default(subscriber, || {
        let _ = sample_op();
        futures::executor::block_on(sample_ffi()).unwrap();
    });
    let spans = cap.0.lock().unwrap().clone();

    let op = spans
        .get("sample_op")
        .unwrap_or_else(|| panic!("no sample_op span: {spans:?}"));
    for expected in ["operation", "sentry.op", "sentry.name"] {
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
}
