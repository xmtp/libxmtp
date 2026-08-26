#![cfg(all(feature = "sentry", not(target_arch = "wasm32")))]

use xmtp_logging::{Level, SentryConfig, TelemetryConfig, XmtpLogging};

#[test]
fn sentry_slot_lifecycle() {
    let handle = XmtpLogging::builder()
        .level(Level::Debug)
        .install()
        .expect("first install in this process");

    let cfg = SentryConfig {
        // Valid-shape DSN; nothing sends without real traffic + flush.
        dsn: "https://public@example.ingest.sentry.io/1".into(),
        ..Default::default()
    };
    handle.enable_sentry(cfg.clone()).expect("enable");
    // Second enable replaces the layer, not errors.
    handle.enable_sentry(cfg).expect("re-enable");
    // OTLP and Sentry share the slot: enabling OTLP must be refused while Sentry
    // holds it (rejected before any exporter is built, so no endpoint is needed).
    let refused = handle
        .enable_telemetry(TelemetryConfig::default())
        .expect_err("OTLP refused while sentry holds the slot");
    assert!(
        refused.to_string().contains("sentry telemetry active"),
        "unexpected error: {refused}"
    );
    handle.disable_sentry().expect("disable");
    handle.flush();

    let bad = SentryConfig {
        dsn: "not a dsn".into(),
        ..Default::default()
    };
    assert!(handle.enable_sentry(bad).is_err());
}
