use tracing::{Level, Metadata, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, filter::FilterFn};

/// Targets whose TRACE events print a serialized group context: OpenMLS's
/// key schedule, and its KDF labels, whose context is the group context. A
/// group context holds the AppData dictionary, and the dictionary holds the
/// group root, which never leaves the client outside a Welcome (GMOD-042).
const GROUP_CONTEXT_TRACE_TARGETS: [&str; 2] = ["openmls::schedule", "openmls::ciphersuite"];

/// Whether `metadata` is an OpenMLS TRACE event that prints a serialized
/// group context.
pub fn prints_group_context(metadata: &Metadata<'_>) -> bool {
    *metadata.level() == Level::TRACE
        && GROUP_CONTEXT_TRACE_TARGETS.iter().any(|prefix| {
            metadata
                .target()
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
        })
}

fn hides_group_context(metadata: &Metadata<'_>) -> bool {
    !prints_group_context(metadata)
}

/// A global filter that drops every event [`prints_group_context`] matches.
/// Every subscriber that can enable OpenMLS TRACE events installs it, so no
/// directive, `RUST_LOG` included, turns them back on.
pub fn group_context_guard() -> FilterFn<fn(&Metadata<'_>) -> bool> {
    FilterFn::new(hides_group_context as fn(&Metadata<'_>) -> bool)
}

pub fn filter_directive(level: &str) -> EnvFilter {
    let level: LevelFilter = level
        .parse()
        .inspect_err(|_| tracing::error!("invalid level `{}`, defaulting to `INFO`", level))
        .unwrap_or(LevelFilter::INFO);

    let filter = format!(
        "xchaos={level},xmtp_backend={level},xmtp_mls_validation={level},\
        xmtp_mls={level},xmtp_mls_common={level},xmtp_id={level},\
        xmtp_api={level},xmtp_api_grpc={level},xmtp_proto={level},\
        xmtp_common={level},xmtp_api_backend={level},\
        xmtp_content_types={level},xmtp_cryptography={level},\
        xmtp_user_preferences={level},xmtpv3={level},xmtp_db={level},xmtp_sdk={level},\
        opentelemetry_sdk=warn,opentelemetry_otlp=warn"
    );
    EnvFilter::builder()
        .parse(filter)
        .expect("Static filter must be correct")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_filter_correct() {
        filter_directive("OFF");
        filter_directive("ERROR");
        filter_directive("WARN");
        filter_directive("INFO");
        filter_directive("DEBUG");
        filter_directive("TRACE");
        filter_directive("INCORRECT_DOES_NOT_PANIC");
    }

    /// No filter, however verbose, passes an OpenMLS TRACE event that
    /// prints a group context. OpenMLS logs through the `log` crate; the
    /// `tracing-log` bridge gives each record its module path as the target.
    #[test]
    fn group_context_guard_drops_group_context_traces() {
        use std::sync::{Arc, Mutex};
        use tracing::dispatcher::{self, Dispatch};
        use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> fmt::MakeWriter<'a> for Buf {
            type Writer = Buf;
            fn make_writer(&'a self) -> Buf {
                self.clone()
            }
        }

        let buf = Buf::default();
        let subscriber = Registry::default().with(group_context_guard()).with(
            fmt::layer()
                .with_writer(buf.clone())
                .with_filter(EnvFilter::new("trace")),
        );
        dispatcher::with_default(&Dispatch::new(subscriber), || {
            tracing::trace!(target: "openmls::schedule", "schedule context");
            tracing::trace!(target: "openmls::ciphersuite::kdf_label", "kdf context");
            tracing::trace!(target: "openmls::schedulers", "other module");
            tracing::debug!(target: "openmls::schedule", "schedule debug");
            tracing::trace!(target: "openmls::group", "group trace");
        });

        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(!out.contains("schedule context"), "{out}");
        assert!(!out.contains("kdf context"), "{out}");
        for kept in ["other module", "schedule debug", "group trace"] {
            assert!(out.contains(kept), "{kept} was dropped:\n{out}");
        }
    }

    #[test]
    fn stdout_filter_at_warn_drops_xmtp_info() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};
        use tracing::dispatcher::{self, Dispatch};
        use tracing_subscriber::{Registry, fmt, prelude::*};

        #[derive(Clone, Default)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
            type Writer = Buf;
            fn make_writer(&'a self) -> Buf {
                self.clone()
            }
        }

        let buf = Buf::default();
        let layer = fmt::layer()
            .with_writer(buf.clone())
            .with_filter(super::filter_directive("warn"));
        let subscriber = Registry::default().with(layer);

        dispatcher::with_default(&Dispatch::new(subscriber), || {
            tracing::info!(target: "xmtp_api", "should be dropped at warn");
            tracing::warn!(target: "xmtp_api", "should be kept at warn");
        });

        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            !out.contains("should be dropped"),
            "INFO leaked to stdout at warn:\n{out}"
        );
        assert!(
            out.contains("should be kept"),
            "WARN was wrongly dropped:\n{out}"
        );
    }
}
