#![cfg(all(feature = "sentry", not(target_arch = "wasm32")))]

use xmtp_logging::{Level, SentryConfig, TelemetryConfig, XmtpLogging};

#[test]
fn sentry_slot_lifecycle() {
    let handle = XmtpLogging::builder()
        .level(Level::Debug)
        .install()
        .expect("first install in this process");

    // Stand in for a Rust host that ran its own `sentry::init` before handing
    // control to libxmtp: its client sits on the process hub, and our enable ->
    // disable cycle has to give that hub back exactly as it found it.
    let host = std::sync::Arc::new(sentry::Client::from(sentry::ClientOptions::new()));
    sentry::Hub::main().bind_client(Some(host.clone()));

    // disable_sentry before any enable_sentry is a no-op: it must not corrupt
    // slot state (asserted below by a subsequent enable_sentry succeeding).
    handle
        .disable_sentry()
        .expect("disable before enable is a no-op");
    assert!(
        main_client().is_some_and(|c| std::sync::Arc::ptr_eq(&c, &host)),
        "the no-op disable took the host's client off the process hub"
    );

    let cfg = SentryConfig {
        // Valid-shape DSN; nothing sends without real traffic + flush.
        dsn: "https://public@example.ingest.sentry.io/1".into(),
        ..Default::default()
    };
    handle.enable_sentry(cfg.clone()).expect("enable");
    assert!(
        main_client().is_some_and(|c| !std::sync::Arc::ptr_eq(&c, &host)),
        "enable_sentry never published its own client to the process hub"
    );
    // Second enable replaces the layer, not errors. It must not re-stash: the
    // client it would stash is our own, which `disable_sentry` would then leave
    // behind as if it were the host's.
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
    assert!(
        main_client().is_some_and(|c| std::sync::Arc::ptr_eq(&c, &host)),
        "disable_sentry left the host without its own sentry client"
    );
    handle.flush();

    let bad = SentryConfig {
        dsn: "not a dsn".into(),
        ..Default::default()
    };
    assert!(handle.enable_sentry(bad).is_err());
    assert!(
        main_client().is_some_and(|c| std::sync::Arc::ptr_eq(&c, &host)),
        "a failed enable disturbed the process hub"
    );
}

fn main_client() -> Option<std::sync::Arc<sentry::Client>> {
    sentry::Hub::main().client()
}
