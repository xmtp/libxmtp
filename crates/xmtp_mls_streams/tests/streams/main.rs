//! Stream suites that build clients. They reach the stream operations from
//! outside the crate, through the facade, so the streams crate takes them
//! along when it moves out.

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(not(target_arch = "wasm32"))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}

/// A macro for asserting that a stream yields a specific decrypted message.
///
/// # Example
/// ```rust
/// assert_msg!(stream, b"first");
/// ```
macro_rules! assert_msg {
    ($stream:expr, $expected:expr) => {
        let next = $stream
            .next()
            .await
            .unwrap()
            .inspect_err(|e| tracing::error!("{}", e.to_string()))
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(next.decrypted_message_bytes.as_slice()),
            String::from_utf8_lossy($expected.as_bytes())
        );
    };
}

/// A macro for asserting that a stream yields a specific decrypted message.
///
/// # Example
/// ```rust
/// assert_msg!(stream, b"first");
/// ```
#[allow(unused_macros)]
macro_rules! assert_msg_exists {
    ($stream:expr) => {
        assert!(
            !$stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .decrypted_message_bytes
                .is_empty()
        );
    };
}

// Live backend tests require native full-duplex HTTP/2.
#[cfg(not(target_arch = "wasm32"))]
mod bidi_tests;
mod catch_up;
// Randomized delivery fuzz over the live node (same gating as `bidi_tests`).
#[cfg(not(target_arch = "wasm32"))]
mod bidi_fuzz_tests;
// Live integration tests for the router (v3 wire; same gating rationale as
// `bidi_tests` above).
#[cfg(not(target_arch = "wasm32"))]
mod delivery_integration_tests;
mod message_deletion_tests;
mod message_reader;
mod process_welcome;
#[cfg(not(target_arch = "wasm32"))]
mod router_callbacks_tests;
mod stream_all;
mod stream_conversations;
mod stream_stats;
