#![cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]

use http::HeaderMap;
use xmtp_logging::test_logging::OtlpCollector;
use xmtp_logging::{Level, TelemetryConfig, XmtpLogging, propagation};

// The production subscriber is global. Keep its lifecycle in one process.
#[tokio::test]
async fn runtime_telemetry_preserves_generated_and_remote_contexts() {
    const TRACE_ID: &str = "12345678901234567890123456789012";
    let collector = OtlpCollector::start().await.unwrap();
    let handle = std::sync::Arc::new(XmtpLogging::builder().level(Level::Info).install().unwrap());
    propagation::install();
    let config = TelemetryConfig {
        endpoint: Some(collector.endpoint()),
        logs: false,
        ..Default::default()
    };
    for _ in 0..2 {
        let disabled = tracing::info_span!(target: "xmtp_api", "disabled");
        let mut headers = HeaderMap::new();
        propagation::inject(&disabled, &mut headers);
        assert!(headers.is_empty());
        drop(disabled);
        handle.enable_telemetry(config.clone()).unwrap();
        let generated = tracing::info_span!(target: "xmtp_api", "generated");
        assert!(!generated.is_disabled());
        propagation::inject(&generated, &mut headers);
        assert!(propagation::extract(&headers).is_some());
        drop(generated);
        headers.insert(
            "traceparent",
            format!("00-{TRACE_ID}-1234567890123456-01")
                .parse()
                .unwrap(),
        );
        headers.insert("tracestate", "vendor=value".parse().unwrap());
        let remote = propagation::extract(&headers).unwrap();
        let child = tracing::info_span!(target: "xmtp_api", "remote_child");
        assert!(propagation::set_parent(&child, remote));
        let mut outgoing = HeaderMap::new();
        propagation::inject(&child, &mut outgoing);
        assert!(outgoing["traceparent"].to_str().unwrap().contains(TRACE_ID));
        assert_eq!(outgoing["tracestate"], "vendor=value");
        drop(child);
        let shutdown = handle.clone();
        tokio::task::spawn_blocking(move || shutdown.disable_telemetry())
            .await
            .unwrap()
            .unwrap();
        let spans = collector.take_spans();
        let spans: Vec<_> = spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .collect();
        assert_eq!(spans.len(), 2);
        let child = spans
            .iter()
            .find(|span| span.name == "remote_child")
            .unwrap();
        assert_eq!(hex::encode(&child.trace_id), TRACE_ID);
        assert_eq!(hex::encode(&child.parent_span_id), "1234567890123456");
    }
}
