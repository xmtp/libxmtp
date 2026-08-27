#![cfg(all(feature = "sentry", not(target_arch = "wasm32")))]

use std::sync::Arc;

use xmtp_logging::{Level, SentryConfig, TelemetryConfig, XmtpLogging};

/// One test for the whole lifecycle: the subscriber, the process hub and the
/// user slot are all process-global, so the assertions have to run in a fixed
/// order. The handle drop is deliberately last — nothing after it can log.
#[test]
fn sentry_slot_lifecycle() {
    let handle = XmtpLogging::builder()
        .level(Level::Debug)
        .install()
        .expect("first install in this process");

    // Stand in for a Rust host that ran its own `sentry::init` before handing
    // control to libxmtp: its client sits on the process hub, and our enable ->
    // disable cycle has to give that hub back exactly as it found it.
    let host = Arc::new(sentry::Client::from(sentry::ClientOptions::new()));
    sentry::Hub::main().bind_client(Some(host.clone()));

    // disable_sentry before any enable_sentry is a no-op: it must not corrupt
    // slot state (asserted below by a subsequent enable_sentry succeeding).
    handle
        .disable_sentry()
        .expect("disable before enable is a no-op");
    assert!(
        main_client().is_some_and(|c| Arc::ptr_eq(&c, &host)),
        "the no-op disable took the host's client off the process hub"
    );

    let cfg = SentryConfig {
        // Valid-shape DSN; nothing sends without real traffic + flush.
        dsn: "https://public@example.ingest.sentry.io/1".into(),
        ..Default::default()
    };
    // This thread touched the hub first, so it owns the process hub and its
    // `Hub::current()` *is* `Hub::main()`. Enabling here must still publish.
    assert!(
        Arc::ptr_eq(&sentry::Hub::current(), &sentry::Hub::main()),
        "the test thread is expected to own the process hub"
    );
    handle.enable_sentry(cfg.clone()).expect("enable");
    assert!(
        main_client().is_some_and(|c| !Arc::ptr_eq(&c, &host)),
        "enable_sentry never published its own client to the process hub"
    );

    // `sentry::init` binds to `Hub::current()`, which off the process-hub thread
    // is an island the host keeps using. Enabling from a worker thread must leave
    // that thread's hub exactly as the host had it — only `Hub::main()` is ours.
    // Also a second enable: it replaces the layer instead of erroring, and must
    // not re-stash our own client as if it were the host's.
    let thread_host = Arc::new(sentry::Client::from(sentry::ClientOptions::new()));
    let cfg_worker = cfg.clone();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let thread_hub = sentry::Hub::current();
            assert!(
                !Arc::ptr_eq(&thread_hub, &sentry::Hub::main()),
                "a spawned thread unexpectedly owns the process hub"
            );
            thread_hub.bind_client(Some(thread_host.clone()));
            handle.enable_sentry(cfg_worker).expect("re-enable");
            assert!(
                thread_hub
                    .client()
                    .is_some_and(|c| Arc::ptr_eq(&c, &thread_host)),
                "enable_sentry left our client bound to the enabling thread's hub"
            );
        });
    });
    assert!(
        main_client().is_some_and(|c| !Arc::ptr_eq(&c, &host) && !Arc::ptr_eq(&c, &thread_host)),
        "the process hub does not carry the client the worker thread enabled"
    );

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
        main_client().is_some_and(|c| Arc::ptr_eq(&c, &host)),
        "disable_sentry left the host without its own sentry client"
    );
    handle.flush();

    let bad = SentryConfig {
        dsn: "not a dsn".into(),
        ..Default::default()
    };
    assert!(handle.enable_sentry(bad).is_err());
    assert!(
        main_client().is_some_and(|c| Arc::ptr_eq(&c, &host)),
        "a failed enable disturbed the process hub"
    );

    // Last step in the file: a host that drops the handle without disabling first
    // still gets its hub back. The handle is gone afterwards, so nothing below
    // may log.
    let drop_host = Arc::new(sentry::Client::from(sentry::ClientOptions::new()));
    sentry::Hub::main().bind_client(Some(drop_host.clone()));
    handle.enable_sentry(cfg).expect("enable before drop");
    assert!(
        main_client().is_some_and(|c| !Arc::ptr_eq(&c, &drop_host)),
        "enable_sentry never published its own client to the process hub"
    );
    drop(handle);
    assert!(
        main_client().is_some_and(|c| Arc::ptr_eq(&c, &drop_host)),
        "dropping the handle left the process hub on the client it just closed"
    );
}

fn main_client() -> Option<Arc<sentry::Client>> {
    sentry::Hub::main().client()
}
