//! OpenTelemetry trace + log export (native only — OTLP/tonic is not wasm-compatible).
//!
//! [`init`] builds an OTLP span exporter and a [`tracing_opentelemetry`] layer, and
//! also wires up an OTLP log exporter so `tracing` events are forwarded as OTLP logs.
//! Both exporters share the same Resource, so logs correlate to spans automatically.
//! Metrics are derived downstream by an OpenTelemetry Collector's `spanmetrics` connector.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// The OTel instrumentation scope / tracer name used for libxmtp spans.
pub const SCOPE: &str = "libxmtp";

/// Owns the OTel tracer + logger providers. Call [`TelemetryGuard::force_flush`] to
/// push queued spans and logs without tearing anything down, or drop (or call
/// [`TelemetryGuard::shutdown`]) to flush-and-stop both exporters before exit.
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
}

impl TelemetryGuard {
    /// Push any queued spans **and** logs to their exporters **without** shutting them
    /// down. Both providers stay live after the call. Best-effort: logs rather than
    /// panics on error. Use this for periodic / pre-checkpoint flushes; use
    /// [`Self::shutdown`] only when you're done exporting.
    pub fn force_flush(&self) {
        if let Err(e) = self.tracer_provider.force_flush() {
            tracing::debug!("otel tracer force_flush: {e}");
        }
        if let Err(e) = self.logger_provider.force_flush() {
            tracing::debug!("otel logger force_flush: {e}");
        }
    }

    /// Flush and **shut down** both the span and log exporters. Terminal: both
    /// providers stop, so telemetry created afterwards is dropped. Idempotent-safe
    /// to call once; the `Drop` impl calls this if you don't.
    pub fn shutdown(&self) {
        // Best-effort flush; log rather than panic on exporter shutdown error.
        if let Err(e) = self.tracer_provider.shutdown() {
            tracing::debug!("otel tracer shutdown: {e}");
        }
        if let Err(e) = self.logger_provider.shutdown() {
            tracing::debug!("otel logger shutdown: {e}");
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

/// The layers and guard returned by [`init`]: the OpenTelemetry trace layer, the
/// OTLP-logs appender layer, and the guard owning both providers.
pub type TelemetryLayers<S> = (
    tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
    Box<dyn tracing_subscriber::Layer<S> + Send + Sync>,
    TelemetryGuard,
);

/// Initialize OTLP trace and log export.
///
/// `endpoint` sets the OTLP gRPC endpoint for both exporters (e.g.
/// `http://collector:4317`). When `None`, each exporter falls back to its
/// standard env-var (`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` /
/// `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`), then `OTEL_EXPORTER_OTLP_ENDPOINT`,
/// then `http://localhost:4317`. `resource_attrs` are merged into the shared OTel
/// resource and attached to every exported span and log — the log appender bridges
/// `tracing` events to OTLP logs carrying the active span's trace/span IDs.
///
/// Returns the tracing layer + log appender layer to register on the subscriber
/// and a guard that must be kept alive for the process lifetime (shut down before
/// exit to flush — see [`TelemetryGuard::shutdown`]). Returns `Err` only if
/// either exporter fails to build (e.g. a malformed endpoint).
pub fn init<S>(
    endpoint: Option<String>,
    resource_attrs: Vec<(String, String)>,
) -> Result<TelemetryLayers<S>, opentelemetry_otlp::ExporterBuildError>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use opentelemetry_otlp::WithExportConfig as _;
    use tracing_subscriber::Layer as _;

    let resource = resource(resource_attrs);

    let mut builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();
    if let Some(endpoint) = endpoint.clone() {
        // Pass the endpoint straight to the exporter (no env-var round-trip).
        builder = builder.with_endpoint(endpoint);
    }
    let span_exporter = builder.build()?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();

    // OTLP log exporter -> logger provider, sharing the SAME resource as the
    // tracer so exported logs carry identical service.name / deployment.environment
    // (the unified-tag match Datadog needs to correlate logs to traces). Build the
    // log exporter BEFORE registering the tracer provider globally, so a log-build
    // failure returns `Err` without leaving a leaked global exporter running.
    let mut log_builder = opentelemetry_otlp::LogExporter::builder().with_tonic();
    if let Some(endpoint) = endpoint {
        log_builder = log_builder.with_endpoint(endpoint);
    }
    let log_exporter = log_builder.build()?;
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(resource.clone())
        .build();
    let appender = OpenTelemetryTracingBridge::new(&logger_provider).boxed();

    // Both exporters built successfully — now register the tracer provider
    // globally and build the trace layer.
    let tracer = tracer_provider.tracer(SCOPE);
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((
        layer,
        appender,
        TelemetryGuard {
            tracer_provider,
            logger_provider,
        },
    ))
}
