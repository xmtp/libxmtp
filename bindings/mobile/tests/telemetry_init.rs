//! Global-initialization assertions that need a fresh process: unit tests in
//! the lib target share one process and race the `HANDLE` OnceLock.
#![cfg(not(target_arch = "wasm32"))]

use xmtpv3::logger;

#[test]
fn flush_and_disable_before_init_touch_nothing() {
    // Flush before any init must not lazily install the global subscriber.
    logger::flush_telemetry();
    // A user id staged with no handle/enable must not survive disable.
    logger::set_sentry_user(Some("staged".into()));
    let _ = logger::disable_sentry_telemetry();
    assert!(!xmtp_logging::sentry::user_stable_id_is_set());
    // Installing now must still succeed: flush/disable left the slot untouched
    // (a lazy install by either would have claimed the subscriber already).
    logger::init_logger();
}
