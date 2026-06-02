//! OpenTelemetry traces + metrics export (native only, opt-in via the `otel`
//! feature).
//!
//! [`init`] builds OTLP exporters from the standard `OTEL_EXPORTER_OTLP_*`
//! environment variables and returns:
//! - a [`tracing_opentelemetry`] layer to add to the tracing subscriber, so any
//!   existing `#[tracing::instrument]` span is exported as a distributed trace;
//! - a [`TelemetryGuard`] that owns the SDK providers and flushes/shuts them down
//!   on drop.
//!
//! Metric instruments (e.g. per-RPC duration) read the **global** meter provider
//! that [`init`] installs, so call sites can record metrics via
//! [`opentelemetry::global::meter`] without threading a handle through the API.
//!
//! When the endpoint cannot be reached the exporters retry in the background;
//! nothing here blocks the hot path. If you do not call [`init`], the global
//! providers are the OTel no-op defaults and all instrument calls are cheap
//! no-ops.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// The OTel instrumentation scope / tracer name used for libxmtp spans.
pub const SCOPE: &str = "libxmtp";

/// Owns the OTel SDK providers. Drop (or call [`TelemetryGuard::shutdown`]) to
/// flush pending spans/metrics before exit.
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
}

impl TelemetryGuard {
    /// Flush and shut down the exporters. Idempotent-safe to call once; the
    /// `Drop` impl calls this if you don't.
    pub fn shutdown(&self) {
        // Best-effort flush; log rather than panic on exporter shutdown error.
        if let Err(e) = self.tracer_provider.shutdown() {
            tracing::debug!("otel tracer shutdown: {e}");
        }
        if let Err(e) = self.meter_provider.shutdown() {
            tracing::debug!("otel meter shutdown: {e}");
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Build the OTel resource (service.name + version + caller-supplied attrs)
/// attached to all telemetry.
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

/// Initialize OTLP traces + metrics export.
///
/// Honors the standard `OTEL_EXPORTER_OTLP_ENDPOINT` (and signal-specific
/// `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` / `..._METRICS_ENDPOINT`) env vars; the
/// underlying exporter defaults to `http://localhost:4317` (gRPC) when unset.
///
/// Returns the tracing layer to register on the subscriber and a guard that must
/// be kept alive for the process lifetime. Returns `Err` only if an exporter
/// fails to build (e.g. a malformed endpoint).
pub fn init<S>(
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
    let resource = resource(resource_attrs);

    // ---- Traces ----
    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();
    let tracer = tracer_provider.tracer(SCOPE);
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    // ---- Metrics ----
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .build()?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((
        layer,
        TelemetryGuard {
            tracer_provider,
            meter_provider,
        },
    ))
}
