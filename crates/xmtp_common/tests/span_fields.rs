#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};
use tracing_subscriber::prelude::*;

#[derive(Default, Clone)]
struct FieldCapture(Arc<Mutex<Vec<String>>>);

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
        let mut names: Vec<String> = attrs
            .metadata()
            .fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        self.0.lock().unwrap().append(&mut names);
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

#[test]
fn span_macros_emit_sentry_fields() {
    let cap = FieldCapture::default();
    let subscriber = tracing_subscriber::registry().with(cap.clone());
    tracing::subscriber::with_default(subscriber, || {
        let _ = sample_op();
        futures::executor::block_on(sample_ffi()).unwrap();
    });
    let fields = cap.0.lock().unwrap().clone();
    for expected in ["operation", "sentry.op", "sentry.name"] {
        assert!(
            fields.iter().any(|f| f == expected),
            "missing {expected}: {fields:?}"
        );
    }
}
