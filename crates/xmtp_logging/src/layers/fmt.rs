use std::fmt::{Debug, Result as FmtResult, Write};
use tracing::field::Field;
use tracing_subscriber::Layer;
use tracing_subscriber::field::{MakeVisitor, VisitFmt, VisitOutput};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::{DefaultFields, DefaultVisitor, Writer};
use tracing_subscriber::registry::LookupSpan;

use crate::config::Level;
use crate::filter::filter_directive;

/// Field formatter for human-readable output: renders like [`DefaultFields`] but
/// drops `sentry.*` fields.
///
/// The span macros attach static `sentry.op`/`sentry.name` hints for
/// sentry-tracing (without them every Sentry span arrives as op = "default").
/// On a terminal or logcat line they are pure duplication of `operation`.
pub(crate) struct HideSentryFields;

impl<'a> MakeVisitor<Writer<'a>> for HideSentryFields {
    type Visitor = HideSentryVisitor<'a>;

    fn make_visitor(&self, target: Writer<'a>) -> Self::Visitor {
        HideSentryVisitor(DefaultFields::new().make_visitor(target))
    }
}

/// Delegates every `record_*` to [`DefaultVisitor`] except for `sentry.*` fields.
/// Each arm is forwarded explicitly: the `Visit` defaults funnel through
/// `record_debug`, which would bypass `DefaultVisitor`'s `str`/`error` handling.
pub(crate) struct HideSentryVisitor<'a>(DefaultVisitor<'a>);

fn is_sentry(field: &Field) -> bool {
    field.name().starts_with("sentry.")
}

macro_rules! forward_unless_sentry {
    ($(fn $name:ident($ty:ty);)*) => {
        $(
            fn $name(&mut self, field: &Field, value: $ty) {
                if !is_sentry(field) {
                    self.0.$name(field, value);
                }
            }
        )*
    };
}

impl tracing::field::Visit for HideSentryVisitor<'_> {
    forward_unless_sentry! {
        fn record_f64(f64);
        fn record_i64(i64);
        fn record_u64(u64);
        fn record_i128(i128);
        fn record_u128(u128);
        fn record_bool(bool);
        fn record_str(&str);
        fn record_bytes(&[u8]);
        fn record_error(&(dyn std::error::Error + 'static));
        fn record_debug(&dyn Debug);
    }
}

impl VisitOutput<FmtResult> for HideSentryVisitor<'_> {
    fn finish(self) -> FmtResult {
        self.0.finish()
    }
}

impl VisitFmt for HideSentryVisitor<'_> {
    fn writer(&mut self) -> &mut dyn Write {
        self.0.writer()
    }
}

/// A stdout fmt layer: JSON (flattened) when `json`, else compact.
///
/// Filtered via `filter_directive` (explicit per-crate directives at
/// `stdout_level`) so it overrides the global per-crate filter and narrows
/// stdout below `level` — a bare default directive would not (INFO leaks).
pub(crate) fn stdout_layer<S>(json: bool, stdout_level: Level) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let filter = filter_directive(stdout_level.as_str());
    if json {
        // Deliberately keeps `sentry.*`: JSON stdout is machine-consumed, so the
        // hints are filterable downstream rather than noise on a line.
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_level(true)
            .with_target(true)
            .with_filter(filter)
            .boxed()
    } else {
        fmt::layer()
            .fmt_fields(HideSentryFields)
            .with_filter(filter)
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::prelude::*;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Buf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> fmt::MakeWriter<'a> for Buf {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Every operation span carries `operation` plus the `sentry.*` hints; the
    /// human-readable line must show the first and none of the rest.
    #[test]
    fn plain_text_hides_sentry_fields() {
        let buf = Buf::default();
        let layer = fmt::layer()
            .with_ansi(false)
            .fmt_fields(HideSentryFields)
            .with_writer(buf.clone());
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            let span = tracing::info_span!(
                "op",
                operation = "mls.sync",
                sentry.op = "mls",
                sentry.name = "mls.sync"
            );
            let _entered = span.enter();
            tracing::info!("synced");
        });

        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            out.contains("operation=\"mls.sync\""),
            "operation field was dropped: {out}"
        );
        assert!(
            !out.contains("sentry."),
            "sentry.* hints leaked into human output: {out}"
        );
    }
}
