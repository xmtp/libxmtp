use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

pub fn filter_directive(level: &str) -> EnvFilter {
    let level: LevelFilter = level
        .parse()
        .inspect_err(|_| tracing::error!("invalid level `{}`, defaulting to `INFO`", level))
        .unwrap_or(LevelFilter::INFO);

    let filter = format!(
        "xmtp_mls={level},xmtp_mls_common={level},xmtp_id={level},\
        xmtp_api={level},xmtp_api_grpc={level},xmtp_proto={level},\
        xmtp_common={level},xmtp_api_d14n={level},\
        xmtp_content_types={level},xmtp_cryptography={level},\
        xmtp_user_preferences={level},xmtpv3={level},xmtp_db={level},\
        bindings_wasm={level},bindings_node={level}"
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
