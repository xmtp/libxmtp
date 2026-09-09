//! W3C trace context propagation. Extraction returns a Context suitable for set_parent.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use http::HeaderMap;
    use opentelemetry::{
        Context, global,
        propagation::{Extractor, Injector},
        trace::TraceContextExt,
    };
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    /// Install the W3C trace-context propagator.
    pub fn install() {
        global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );
    }

    struct HeaderInjector<'a>(&'a mut HeaderMap);
    impl Injector for HeaderInjector<'_> {
        fn set(&mut self, key: &str, value: String) {
            if let (Ok(key), Ok(value)) = (
                http::header::HeaderName::try_from(key),
                http::HeaderValue::try_from(value),
            ) {
                self.0.insert(key, value);
            }
        }
    }

    struct HeaderExtractor<'a>(&'a HeaderMap);
    impl Extractor for HeaderExtractor<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key)?.to_str().ok()
        }
        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(|key| key.as_str()).collect()
        }
    }

    /// Inject the span context. Without an OTel layer, no headers are written.
    pub fn inject(span: &tracing::Span, headers: &mut HeaderMap) {
        global::get_text_map_propagator(|p| {
            p.inject_context(&span.context(), &mut HeaderInjector(headers))
        });
    }

    /// Extract a valid remote parent. Invalid and all-zero trace IDs are absent.
    pub fn extract(headers: &HeaderMap) -> Option<Context> {
        let cx = global::get_text_map_propagator(|p| {
            p.extract_with_context(&Context::new(), &HeaderExtractor(headers))
        });
        cx.span().span_context().is_valid().then_some(cx)
    }

    /// Set the remote parent. Return false when no OTel layer accepts it.
    pub fn set_parent(span: &tracing::Span, cx: Context) -> bool {
        span.set_parent(cx).is_ok()
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use http::HeaderMap;
    use opentelemetry::Context;

    /// Browser telemetry propagation is disabled.
    pub fn install() {}
    /// Leave browser request headers unchanged.
    pub fn inject(_span: &tracing::Span, _headers: &mut HeaderMap) {}
    /// Browser telemetry propagation is disabled.
    pub fn extract(_headers: &HeaderMap) -> Option<Context> {
        None
    }
    /// Browser telemetry propagation is disabled.
    pub fn set_parent(_span: &tracing::Span, _cx: Context) -> bool {
        false
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use http::HeaderMap;
    use opentelemetry::{
        Context,
        trace::{TraceContextExt, TracerProvider},
    };
    use tracing_subscriber::prelude::*;

    #[test]
    fn no_layer_writes_nothing() {
        install();
        tracing::subscriber::with_default(tracing_subscriber::registry(), || {
            let span = tracing::info_span!("no_exporter");
            let mut headers = HeaderMap::new();
            inject(&span, &mut headers);
            assert!(headers.is_empty());
            assert!(!set_parent(&span, Context::new()));
        });
    }

    #[test]
    fn trace_context_round_trips() {
        install();
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("test"));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            let mut incoming = HeaderMap::new();
            incoming.insert(
                "traceparent",
                "00-12345678901234567890123456789012-1234567890123456-01"
                    .parse()
                    .unwrap(),
            );
            incoming.insert("tracestate", "vendor=value".parse().unwrap());
            let parent = extract(&incoming).unwrap();
            let span = tracing::info_span!("child");
            assert!(set_parent(&span, parent.clone()));
            let mut outgoing = HeaderMap::new();
            inject(&span, &mut outgoing);
            let child = extract(&outgoing).unwrap();
            assert_eq!(
                child.span().span_context().trace_id(),
                parent.span().span_context().trace_id()
            );
            assert_eq!(outgoing["tracestate"], "vendor=value");
            incoming.insert(
                "traceparent",
                "00-00000000000000000000000000000000-1234567890123456-01"
                    .parse()
                    .unwrap(),
            );
            assert!(extract(&incoming).is_none());
        });
    }
}
