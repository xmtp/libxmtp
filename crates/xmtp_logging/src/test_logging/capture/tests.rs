use super::LogCapture;
use crate::Level;

#[test]
fn backend_events_use_the_shared_filter_and_keep_structured_fields() {
    let capture = LogCapture::new(Level::Info);
    tracing::dispatcher::with_default(&capture.dispatch(), || {
        tracing::info!(target: "xmtp_backend::server", request_size_bytes = 17, request_id = "sample", "request finished");
        tracing::debug!(target: "xmtp_backend::server", "hidden debug event");
    });
    let output = capture.output();
    assert_eq!(output.lines().count(), 1);
    assert!(output.contains("\"request_size_bytes\":17"));
    assert!(output.contains("\"request_id\":\"sample\""));
    assert!(!output.contains("hidden debug event"));

    let warnings = LogCapture::new(Level::Warn);
    tracing::dispatcher::with_default(&warnings.dispatch(), || {
        tracing::info!(target: "xmtp_backend::server", "hidden request event");
    });
    assert!(warnings.output().is_empty());
}
