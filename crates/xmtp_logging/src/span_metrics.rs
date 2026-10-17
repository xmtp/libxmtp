//! One duration sample per operation span, including cancelled operations.
use std::time::Instant;
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// Records operation lifetime, independent of how often a future is polled.
pub struct SpanMetricsLayer;

struct Operation {
    name: String,
    start: Instant,
    failed: bool,
}

#[derive(Default)]
struct OperationVisitor(Option<String>);
impl Visit for OperationVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "operation" {
            self.0 = Some(value.to_owned());
        }
    }
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

impl<S> Layer<S> for SpanMetricsLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = OperationVisitor::default();
        attrs.record(&mut visitor);
        if let Some(name) = visitor.0
            && let Some(span) = ctx.span(id)
        {
            span.extensions_mut().insert(Operation {
                name,
                start: Instant::now(),
                failed: false,
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if *event.metadata().level() != Level::ERROR {
            return;
        }
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                let mut extensions = span.extensions_mut();
                if let Some(operation) = extensions.get_mut::<Operation>() {
                    operation.failed = true;
                    break;
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id)
            && let Some(operation) = span.extensions_mut().remove::<Operation>()
        {
            metrics::histogram!("xmtp_operation_duration_seconds",
                "operation" => operation.name,
                "status" => if operation.failed { "error" } else { "ok" }
            )
            .record(operation.start.elapsed().as_secs_f64());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    use tracing::{
        Instrument,
        dispatcher::{Dispatch, with_default},
    };
    use tracing_subscriber::prelude::*;

    const ONE_SAMPLE: usize = 1;

    /// Run all span polls and closes under thread-local recorder and dispatch.
    fn samples(run: impl FnOnce()) -> Vec<(String, String, usize)> {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            with_default(
                &Dispatch::new(tracing_subscriber::registry().with(SpanMetricsLayer)),
                run,
            );
        });
        snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, value)| {
                assert_eq!(key.key().name(), "xmtp_operation_duration_seconds");
                let labels: std::collections::BTreeMap<_, _> =
                    key.key().labels().map(|l| (l.key(), l.value())).collect();
                const LABEL_COUNT: usize = 2;
                assert_eq!(labels.len(), LABEL_COUNT);
                let DebugValue::Histogram(values) = value else {
                    panic!("expected histogram")
                };
                (
                    labels["operation"].to_owned(),
                    labels["status"].to_owned(),
                    values.len(),
                )
            })
            .collect()
    }

    #[test]
    fn async_span_records_once_across_awaits() {
        let result = samples(|| {
            futures::executor::block_on(
                async {
                    const AWAIT_COUNT: usize = 3;
                    for _ in 0..AWAIT_COUNT {
                        tokio::task::yield_now().await;
                    }
                }
                .instrument(tracing::info_span!("async", operation = "test.async")),
            );
        });
        assert_eq!(result, vec![("test.async".into(), "ok".into(), ONE_SAMPLE)]);
    }

    #[tracing::instrument(err, fields(operation = "test.error"))]
    async fn fails() -> Result<(), &'static str> {
        Err("failed")
    }

    #[test]
    fn instrument_error_marks_failure() {
        let result = samples(|| {
            assert!(futures::executor::block_on(fails()).is_err());
        });
        assert_eq!(
            result,
            vec![("test.error".into(), "error".into(), ONE_SAMPLE)]
        );
    }

    #[test]
    fn dropped_future_is_ok() {
        let result = samples(|| {
            let mut future = Box::pin(std::future::pending::<()>().instrument(
                tracing::info_span!("cancelled", operation = "test.cancelled"),
            ));
            assert!(
                futures::executor::block_on(futures::future::poll_immediate(&mut future)).is_none()
            );
            drop(future);
        });
        assert_eq!(
            result,
            vec![("test.cancelled".into(), "ok".into(), ONE_SAMPLE)]
        );
    }

    #[test]
    fn ignores_spans_without_operation() {
        assert!(
            samples(|| {
                let _span = tracing::info_span!("ignored").entered();
                tracing::error!("ignored");
            })
            .is_empty()
        );
    }

    #[test]
    fn error_marks_nearest_operation_only() {
        let mut result = samples(|| {
            let _outer = tracing::info_span!("outer", operation = "test.outer").entered();
            tracing::warn!("outer warning is not a failure");
            let _inner = tracing::info_span!("inner", operation = "test.inner").entered();
            let _detail = tracing::info_span!("detail").entered();
            tracing::warn!("not an error");
            tracing::error!("inner failed");
        });
        result.sort();
        assert_eq!(
            result,
            vec![
                ("test.inner".into(), "error".into(), ONE_SAMPLE),
                ("test.outer".into(), "ok".into(), ONE_SAMPLE)
            ]
        );
    }
}
