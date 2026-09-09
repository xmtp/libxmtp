//! Native OTLP span and optional log export.
use crate::TelemetryConfig;
use opentelemetry::{KeyValue, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    logs::SdkLoggerProvider,
    trace::{Sampler, SdkTracerProvider, SpanData, SpanExporter},
};
use std::time::Duration;

/// Instrumentation scope for libxmtp spans.
pub const SCOPE: &str = "libxmtp";
pub(crate) mod switch;

const OTLP_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// Owns the providers. Keep this guard until all operation spans close.
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    logger_provider: Option<SdkLoggerProvider>,
    stopped: std::sync::atomic::AtomicBool,
}

impl TelemetryGuard {
    pub(crate) fn tracer(&self) -> opentelemetry_sdk::trace::Tracer {
        self.tracer_provider.tracer(SCOPE)
    }

    /// Flush queued telemetry without stopping export. The wait is bounded.
    pub fn force_flush(&self) {
        if !self.stopped.load(std::sync::atomic::Ordering::Acquire) {
            self.flush_providers(false);
        }
    }

    /// Flush and stop export. Repeated calls do nothing. The wait is bounded,
    /// including when the caller has no Tokio runtime.
    pub fn shutdown(&self) {
        if !self.stopped.swap(true, std::sync::atomic::Ordering::AcqRel) {
            self.flush_providers(true);
        }
    }

    /// Clone providers into the worker so a timeout does not borrow the guard.
    fn flush_providers(&self, shutdown: bool) {
        let tracer = self.tracer_provider.clone();
        let logger = self.logger_provider.clone();
        if !wait_for_flush(OTLP_FLUSH_TIMEOUT, move || {
            if let Err(error) = tracer.force_flush() {
                tracing::warn!(%error, "OTLP trace flush failed");
            }
            if let Some(logger) = &logger
                && let Err(error) = logger.force_flush()
            {
                tracing::warn!(%error, "OTLP log flush failed");
            }
            if shutdown {
                if let Err(error) = tracer.shutdown_with_timeout(OTLP_FLUSH_TIMEOUT) {
                    tracing::warn!(%error, "OTLP trace shutdown failed");
                }
                if let Some(logger) = logger
                    && let Err(error) = logger.shutdown_with_timeout(OTLP_FLUSH_TIMEOUT)
                {
                    tracing::warn!(%error, "OTLP log shutdown failed");
                }
            }
        }) {
            tracing::warn!("OTLP flush did not complete before the deadline");
        }
    }
}

/// Run blocking SDK calls on a worker with an independent Tokio timer. This keeps
/// the synchronous binding APIs safe to call both inside and outside a runtime.
/// A timed-out SDK call cannot be cancelled; it finishes on the detached worker.
fn wait_for_flush(timeout: Duration, flush: impl FnOnce() + Send + 'static) -> bool {
    const COMPLETION_CAPACITY: usize = 1;
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(COMPLETION_CAPACITY);
    let worker = std::thread::Builder::new()
        .name("xmtp-otel-flush".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
            else {
                return;
            };
            let completed = runtime.block_on(async {
                matches!(
                    tokio::time::timeout(timeout, tokio::task::spawn_blocking(flush)).await,
                    Ok(Ok(()))
                )
            });
            // Runtime drop would join blocking workers and defeat the deadline.
            runtime.shutdown_background();
            let _ = done_tx.send(completed);
        });
    worker.is_ok() && done_rx.recv_timeout(timeout).unwrap_or(false)
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Caller attributes cannot replace service identity.
fn resource(config: &TelemetryConfig) -> Resource {
    let service_name = config
        .service_name
        .clone()
        .unwrap_or_else(|| std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "libxmtp".into()));
    Resource::builder()
        .with_attributes(
            config
                .resource_attributes
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), v.clone())),
        )
        .with_service_name(service_name)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Trace layer, optional log bridge (a no-op layer when disabled), and provider guard.
pub type TelemetryLayers<S> = (
    tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
    Box<dyn tracing_subscriber::Layer<S> + Send + Sync>,
    TelemetryGuard,
);

#[derive(Debug)]
struct CountingExporter<E>(E);
impl<E: SpanExporter> SpanExporter for CountingExporter<E> {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let result = self.0.export(batch).await;
        #[cfg(feature = "metrics")]
        if result.is_err() {
            const FAILED_BATCH: u64 = 1;
            metrics::counter!("xmtp_telemetry_export_failures_total").increment(FAILED_BATCH);
        }
        result
    }
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }
    fn force_flush(&self) -> OTelSdkResult {
        self.0.force_flush()
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

/// Build OTLP layers on a Tokio runtime. Endpoint defaults follow OTLP environment
/// variables. The caller must install the returned layers and retain the guard.
pub fn init<S>(
    config: TelemetryConfig,
) -> Result<TelemetryLayers<S>, opentelemetry_otlp::ExporterBuildError>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use opentelemetry_otlp::WithExportConfig as _;
    use tracing_subscriber::Layer as _;
    let resource = resource(&config);
    let mut builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();
    if let Some(endpoint) = &config.endpoint {
        builder = builder.with_endpoint(endpoint.clone());
    }
    let span_exporter = builder.build()?;
    let logger_provider = if config.logs {
        let mut builder = opentelemetry_otlp::LogExporter::builder().with_tonic();
        if let Some(endpoint) = &config.endpoint {
            builder = builder.with_endpoint(endpoint.clone());
        }
        Some(
            SdkLoggerProvider::builder()
                .with_batch_exporter(builder.build()?)
                .with_resource(resource.clone())
                .build(),
        )
    } else {
        None
    };
    let appender = logger_provider
        .as_ref()
        .map(OpenTelemetryTracingBridge::new)
        .boxed();
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(CountingExporter(span_exporter))
        .with_sampler(sampler(config.sample_ratio))
        .with_resource(resource)
        .build();
    let tracer = tracer_provider.tracer(SCOPE);
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    crate::propagation::install();
    Ok((
        tracing_opentelemetry::layer().with_tracer(tracer),
        appender,
        TelemetryGuard {
            tracer_provider,
            logger_provider,
            stopped: std::sync::atomic::AtomicBool::new(false),
        },
    ))
}

fn sampler(ratio: f64) -> Sampler {
    Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::{Span, Tracer};
    use opentelemetry_sdk::trace::InMemorySpanExporter;

    #[test]
    fn resource_identity_wins() {
        let config = TelemetryConfig {
            service_name: Some("client".into()),
            resource_attributes: vec![
                ("service.name".into(), "wrong".into()),
                ("service.version".into(), "wrong".into()),
                ("region".into(), "west".into()),
            ],
            ..Default::default()
        };
        let resource = resource(&config);
        assert_eq!(
            resource.get(&"service.name".into()).unwrap().as_str(),
            "client"
        );
        assert_eq!(
            resource.get(&"service.version".into()).unwrap().as_str(),
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(resource.get(&"region".into()).unwrap().as_str(), "west");
    }

    #[test]
    fn root_sampling_extremes_are_deterministic() {
        const NONE: f64 = 0.0;
        const ALL: f64 = 1.0;
        const SPANS: usize = 8;
        for (ratio, expected) in [(NONE, 0), (ALL, SPANS)] {
            let exporter = InMemorySpanExporter::default();
            let provider = SdkTracerProvider::builder()
                .with_sampler(sampler(ratio))
                .with_simple_exporter(exporter.clone())
                .build();
            let tracer = provider.tracer("test");
            for _ in 0..SPANS {
                tracer
                    .start_with_context("root", &opentelemetry::Context::new())
                    .end();
            }
            assert_eq!(exporter.get_finished_spans().unwrap().len(), expected);
        }
    }

    #[test]
    fn flush_wait_is_bounded() {
        const TEST_TIMEOUT: Duration = Duration::from_millis(20);
        const TEST_DEADLINE: Duration = Duration::from_secs(2);
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let start = std::time::Instant::now();
        assert!(!wait_for_flush(TEST_TIMEOUT, move || {
            release_rx.recv().unwrap();
            finished_tx.send(()).unwrap();
        }));
        assert!(start.elapsed() < TEST_DEADLINE);
        release_tx.send(()).unwrap();
        finished_rx.recv_timeout(TEST_DEADLINE).unwrap();
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn export_failure_counts_once_per_batch() {
        use metrics_util::debugging::{DebugValue, DebuggingRecorder};
        #[derive(Debug)]
        struct Exporter(bool);
        impl SpanExporter for Exporter {
            async fn export(&self, _: Vec<SpanData>) -> OTelSdkResult {
                if self.0 {
                    Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                        "test failure".into(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
        let recorder = DebuggingRecorder::new();
        let snapshots = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            for failed in [true, false, true] {
                let result =
                    futures::executor::block_on(CountingExporter(Exporter(failed)).export(vec![]));
                assert_eq!(result.is_err(), failed);
            }
        });
        let values = snapshots.snapshot().into_vec();
        const COUNTERS: usize = 1;
        const FAILED_BATCHES: u64 = 2;
        assert_eq!(values.len(), COUNTERS);
        let (key, _, _, value) = values.first().unwrap();
        assert_eq!(key.key().name(), "xmtp_telemetry_export_failures_total");
        assert!(key.key().labels().next().is_none());
        assert_eq!(*value, DebugValue::Counter(FAILED_BATCHES));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn otlp_logs_are_optional() {
        use tracing_subscriber::prelude::*;
        let collector = crate::test_logging::OtlpCollector::start().await.unwrap();
        for logs in [false, true] {
            let (trace, appender, guard) = init(TelemetryConfig {
                endpoint: Some(collector.endpoint()),
                logs,
                ..Default::default()
            })
            .unwrap();
            tracing::subscriber::with_default(
                tracing_subscriber::registry().with(vec![trace.boxed(), appender]),
                || {
                    let _span = tracing::info_span!(
                        "operation",
                        operation = "test.logs",
                        otel.name = "test.logs"
                    )
                    .entered();
                    tracing::info!("log event");
                },
            );
            tokio::task::spawn_blocking(move || guard.shutdown())
                .await
                .unwrap();
            assert!(!collector.take_spans().is_empty());
            assert_eq!(!collector.take_logs().is_empty(), logs);
        }
    }
}
