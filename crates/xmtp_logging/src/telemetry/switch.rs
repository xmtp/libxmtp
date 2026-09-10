//! Keep the OTel layer fixed so span context access can downcast through it.

use opentelemetry::{Context, trace::SpanBuilder};
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct SwitchTracer(Arc<RwLock<Option<opentelemetry_sdk::trace::Tracer>>>);

impl SwitchTracer {
    pub(crate) fn set(&self, tracer: Option<opentelemetry_sdk::trace::Tracer>) {
        *self.0.write() = tracer;
        tracing::callsite::rebuild_interest_cache();
    }

    pub(crate) fn enabled(&self) -> bool {
        self.0.read().is_some()
    }
}

impl opentelemetry::trace::Tracer for SwitchTracer {
    type Span = opentelemetry::global::BoxedSpan;

    fn build_with_context(&self, builder: SpanBuilder, parent: &Context) -> Self::Span {
        // Release the lock before SDK callbacks run. A concurrent disable can
        // leave an existing span here after the filter has closed.
        let tracer = self.0.read().clone();
        let tracer = match tracer {
            Some(tracer) => opentelemetry::global::BoxedTracer::new(Box::new(tracer)),
            None => opentelemetry::global::BoxedTracer::new(Box::new(
                opentelemetry::trace::noop::NoopTracer::new(),
            )),
        };
        tracer.build_with_context(builder, parent)
    }
}
