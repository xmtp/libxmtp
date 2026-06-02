//! OpenTelemetry trace export (native only, opt-in via the `otel` feature).
//!
//! [`init`] builds an OTLP span exporter from the standard `OTEL_EXPORTER_OTLP_*`
//! environment variables and returns:
//! - a [`tracing_opentelemetry`] layer to add to the tracing subscriber, so any
//!   existing `#[tracing::instrument]` span is exported as a distributed trace;
//! - a [`TelemetryGuard`] that owns the tracer provider and flushes/shuts it down
//!   on drop.
//!
//! **Metrics are intentionally not emitted here.** Duration / count / error
//! metrics are derived downstream from the exported spans by an OpenTelemetry
//! Collector's `spanmetrics` connector (rate/errors/duration from span
//! name + duration + status + attributes). libxmtp's job is to emit
//! well-attributed spans (`operation`, `worker`, … fields on the instrumented
//! chokepoints); the Collector turns those into metrics. This keeps libxmtp
//! free of metric instruments and keeps metric definitions (buckets,
//! dimensions) in the Collector config. An in-process metrics layer can be
//! re-added later if exact, sampling-independent counts are needed in-app.
//!
//! When the endpoint cannot be reached the exporter retries in the background;
//! nothing here blocks the hot path. If you do not call [`init`], the global
//! tracer provider is the OTel no-op default and spans are not exported.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// The OTel instrumentation scope / tracer name used for libxmtp spans.
pub const SCOPE: &str = "libxmtp";

/// Owns the OTel tracer provider. Drop (or call [`TelemetryGuard::shutdown`]) to
/// flush pending spans before exit.
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
}

impl TelemetryGuard {
    /// Flush and shut down the span exporter. Idempotent-safe to call once; the
    /// `Drop` impl calls this if you don't.
    pub fn shutdown(&self) {
        // Best-effort flush; log rather than panic on exporter shutdown error.
        if let Err(e) = self.tracer_provider.shutdown() {
            tracing::debug!("otel tracer shutdown: {e}");
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Build the OTel resource (service.name + version + caller-supplied attrs)
/// attached to all exported spans.
fn resource(extra: Vec<(String, String)>) -> Resource {
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "libxmtp".to_string());
    let mut builder = Resource::builder()
        .with_service_name(service_name)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
    for (k, v) in extra {
        builder = builder.with_attribute(KeyValue::new(k, v));
    }
    builder.build()
}

/// Initialize OTLP trace export.
///
/// `endpoint` sets the OTLP gRPC endpoint directly (e.g.
/// `http://collector:4317`). When `None`, the exporter falls back to the
/// standard `OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`
/// environment variables, then to `http://localhost:4317`. `resource_attrs` are
/// merged into the OTel resource (e.g. `service.instance.id`,
/// `deployment.environment`) and attached to every exported span — the standard
/// way to attribute telemetry to its source.
///
/// Returns the tracing layer to register on the subscriber and a guard that must
/// be kept alive for the process lifetime (and shut down before exit to flush —
/// see [`TelemetryGuard::shutdown`]). Returns `Err` only if the exporter fails to
/// build (e.g. a malformed endpoint).
pub fn init<S>(
    endpoint: Option<String>,
    resource_attrs: Vec<(String, String)>,
) -> Result<
    (
        tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
        TelemetryGuard,
    ),
    opentelemetry_otlp::ExporterBuildError,
>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use opentelemetry_otlp::WithExportConfig as _;

    let resource = resource(resource_attrs);

    let mut builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();
    if let Some(endpoint) = endpoint {
        // Pass the endpoint straight to the exporter (no env-var round-trip).
        builder = builder.with_endpoint(endpoint);
    }
    let span_exporter = builder.build()?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource)
        .build();
    let tracer = tracer_provider.tracer(SCOPE);
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((layer, TelemetryGuard { tracer_provider }))
}
