# Test Utilities

## Data Generators

```rust
use xmtp_common::{rand_string, rand_vec, rand_hexstring, rand_account_address, rand_u64, rand_i64};
use xmtp_common::test::{tmp_path, rand_time, Generate};

let msg = rand_string::<20>();              // 20-char random alphanumeric
let bytes = rand_vec::<16>();               // 16 random bytes
let hex = rand_hexstring();                 // 0x-prefixed 40-char hex
let addr = rand_account_address();          // 42-char alphanumeric
let path = tmp_path();                      // Temp DB path (WASM-aware)
let n = rand_u64();                         // Random u64
let t = rand_time();                        // Random i64 in 0..1_000_000_000
```

### Generate Trait (for OpenMLS fakes)

```rust
use xmtp_common::test::Generate;

let app_msg = FakeMlsApplicationMessage::generate();
let commit = FakeMlsCommitMessage::generate();
```

**Source:** `crates/xmtp_common/src/test.rs`, `crates/xmtp_common/src/test/openmls.rs`

## Logging

Logger is initialized automatically by `#[xmtp_common::test]`. Control output with env vars:

```bash
RUST_LOG=xmtp_mls=debug cargo nextest run test_name     # Standard log filtering
CONTEXTUAL=1 cargo nextest run test_name                  # Tree-format async-aware logs
STRUCTURED=1 cargo nextest run test_name                  # JSON structured logs
SHOW_SPAN_FIELDS=1 cargo nextest run test_name            # Include tracing span fields
```

### TestLogReplace

The `tester!` macro automatically registers inbox IDs with human-readable names so logs show "alix" instead of hex addresses. Managed via `TestLogReplace`:

```rust
let mut replace = TestLogReplace::default();
replace.add("0x123abc...", "Alice");
// Cleaned up on Drop
```

### Traced Test (capturing logs for assertions)

```rust
use xmtp_common::{traced_test, assert_logged};

traced_test!(async {
    tracing::info!("expected message");
    assert_logged!("expected message", 1);  // Assert it appeared exactly once
});
```

**Source:** `crates/xmtp_common/src/test.rs`, `crates/xmtp_common/src/test/traced_test.rs`

## Retry

```rust
use xmtp_common::{retry_async, Retry};

// Retries with exponential backoff if error.is_retryable() returns true
retry_async!(Retry::default(), (async {
    fallible_network_call().await
}))
```

Default: 3 retries, 50ms initial delay, 3x multiplier, 120s max total wait.

Custom:

```rust
use xmtp_common::{Retry, ExponentialBackoff};
use xmtp_common::time::Duration;

let retry = Retry::builder()
    .retries(5)
    .strategy(ExponentialBackoff::builder()
        .duration(Duration::from_millis(25))
        .multiplier(2)
        .build())
    .build();

retry_async!(retry, (async { op().await }))
```

**Source:** `crates/xmtp_common/src/retry.rs`

## Toxiproxy

For network fault injection tests:

```rust
use xmtp_common::test::toxiproxy_test;

// Serializes test access and resets proxy state
toxiproxy_test(|| async {
    tester!(alix, proxy);
    alix.for_each_proxy(|p| async { p.set_enabled(false).await }).await;
    // Test behavior under network failure
}).await;
```

**Source:** `crates/xmtp_common/src/test.rs`

## Display Helpers

```rust
use xmtp_common::fmt::{truncate_hex, debug_hex};
use xmtp_common::Snippet;

truncate_hex("0x5bf078bd83995fe83092d93c5655f059"); // "0x5bf0...f059"
debug_hex(bytes);                                     // hex-encoded + truncated
some_bytes.snippet();                                  // first 6 chars + ".."
```

**Source:** `crates/xmtp_common/src/fmt.rs`, `crates/xmtp_common/src/snippet.rs`
