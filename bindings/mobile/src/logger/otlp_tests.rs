//! Separate-process tests for the mobile logger's global telemetry slot.
use xmtp_logging::test_logging::OtlpCollector;
use xmtpv3::logger::{
    FfiOtlpConfig, FfiSentryConfig, disable_otlp_telemetry, disable_sentry_telemetry,
    enable_otlp_telemetry, enable_sentry_telemetry, flush_telemetry,
};

#[xmtp_common::test(unwrap_try = true, disable_logging = true, flavor = "multi_thread")]
async fn mobile_otlp_exports_identity_and_frees_slot() {
    const SAMPLE_ALL: f64 = 1.0;
    const SAMPLE_NONE: f32 = 0.0;
    const BREADCRUMBS: u32 = 10;
    let collector = OtlpCollector::start().await?;
    let config = FfiOtlpConfig {
        endpoint: collector.endpoint(),
        service_name: Some("mobile-test".into()),
        sample_ratio: SAMPLE_ALL,
        resource_attributes: [
            ("region".into(), "west".into()),
            ("service.name".into(), "wrong".into()),
        ]
        .into(),
    };
    let sentry = FfiSentryConfig {
        dsn: "https://public@example.ingest.sentry.io/1".into(),
        environment: None,
        release: None,
        traces_sample_rate: SAMPLE_NONE,
        max_breadcrumbs: BREADCRUMBS,
        user_stable_id: None,
        tags: vec![],
    };
    enable_sentry_telemetry(sentry.clone())?;
    let error = enable_otlp_telemetry(config.clone())
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "[GenericError::Generic] telemetry: sentry telemetry active; disable it before enabling OTLP"
    );
    disable_sentry_telemetry()?;
    enable_otlp_telemetry(config).await?;
    {
        let _span = tracing::info_span!(target: "xmtpv3", "mobile_operation", operation = "test.mobile", otel.name = "test.mobile").entered();
        tracing::info!(target: "xmtpv3", "span event");
    }
    tokio::task::spawn_blocking(flush_telemetry).await?;
    let batches = collector.take_spans();
    let batch = batches
        .iter()
        .find(|batch| {
            batch
                .scope_spans
                .iter()
                .any(|scope| scope.spans.iter().any(|span| span.name == "test.mobile"))
        })
        .expect("mobile operation reached OTLP collector");
    let attributes = &batch.resource.as_ref().unwrap().attributes;
    assert_eq!(
        xmtp_logging::test_logging::string_attribute(attributes, "service.name"),
        Some("mobile-test")
    );
    assert_eq!(
        xmtp_logging::test_logging::string_attribute(attributes, "region"),
        Some("west")
    );
    assert!(collector.take_logs().is_empty());
    tokio::task::spawn_blocking(disable_otlp_telemetry).await??;
    {
        let _span = tracing::info_span!(target: "xmtpv3", "disabled", operation = "test.disabled")
            .entered();
    }
    tokio::task::spawn_blocking(flush_telemetry).await?;
    assert!(collector.take_spans().is_empty());
    enable_sentry_telemetry(sentry)?;
    disable_otlp_telemetry()?;
    disable_sentry_telemetry()?;
}
