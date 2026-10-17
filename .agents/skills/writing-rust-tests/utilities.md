# Test utilities

## Data generators

Flat re-exports from `xmtp_common`. `rand_string` and `rand_vec` come from
`xmtp_cryptography::rand`; the rest from `crates/xmtp_common/src/test.rs`.

```rust
use xmtp_common::{Generate, rand_account_address, rand_hexstring, rand_string, rand_time, rand_u64, rand_vec, tmp_path};

let msg = rand_string::<20>();                  // 20-char alphanumeric
let bytes = rand_vec::<16>();                   // 16 random bytes
let hex = rand_hexstring();                     // 0x-prefixed, 40 hex chars
let addr = rand_account_address();
let path = tmp_path();                          // temp DB path, wasm-aware
let n = rand_u64();                             // also rand_i64
let t = rand_time();                            // i64 in 0..1_000_000_000
let commit = FakeMlsCommitMessage::generate();  // Generate trait; OpenMLS fakes in test/openmls.rs
```

## Logging

`#[xmtp_common::test]` installs the logger once. Control it with env vars:

```bash
RUST_LOG=xmtp_mls=debug just test workspace test_name   # target filter
STRUCTURED=1 just test workspace test_name              # JSON lines
SHOW_SPAN_FIELDS=1 just test workspace test_name        # include span fields
XMTP_TEST_LOGGING=false just test workspace test_name   # off; CI=true does the same
```

`just wasm test` forces `RUST_LOG=off`. Client names in logs come from
`tester!(name)`, which registers the inbox id through `Client::set_name`.

### Capturing logs

Native only, `test-utils` only.

```rust
// xmtp_logging::test_logging::LogCapture: JSON events through the production filter
let capture = LogCapture::new(Level::Info);
tracing::dispatcher::with_default(&capture.dispatch(), || {
    tracing::info!(target: "xmtp_backend::server", request_id = "sample", "request finished");
});
assert!(capture.output().contains("\"request_id\":\"sample\""));
```

`traced_test!(async { .. })` with `assert_logged!("msg", 1)` builds its own
runtime and subscriber. Call it from a sync `#[test]`, not from an
`#[xmtp_common::test] async fn` (`crates/xmtp_mls/src/identity_updates.rs`).

## Retry

```rust
use xmtp_common::{ExponentialBackoff, Retry, retry_async};

retry_async!(Retry::default(), (async { fallible_call().await }))   // retries while is_retryable()
```

Default: 5 retries, 50 ms base, x3, 25 ms jitter, 120 s total cap. Custom:

```rust
let retry = Retry::builder()
    .retries(5)
    .with_strategy(ExponentialBackoff::builder().duration(Duration::from_millis(25)).multiplier(2).build())
    .build();
```

**Source:** `crates/xmtp_common/src/retry.rs`

## Toxiproxy

Native only, behind `xmtp_common/test-utils-network`. `xmtp_common::toxiproxy()`
is the client, built from `XMTP_TOXIPROXY_API` so it follows the worktree's
port. `toxiproxy_test` takes a process-wide lock and resets the proxies first.

```rust
// crates/xmtp_mls/src/groups/tests/test_network.rs
use xmtp_common::toxiproxy_test;

toxiproxy_test(async || {
    tester!(alix, proxy);
    tester!(bo);
    alix.for_each_proxy(async |p| { p.disable().await.unwrap(); }).await;
    // behaviour under network failure; p.enable() restores
}).await;
```

## Display helpers

```rust
xmtp_common::fmt::truncate_hex("0x5bf078bd83995fe83092d93c5655f059");   // "0x5bf0...f059"
xmtp_common::fmt::debug_hex(bytes);                                     // hex-encoded, then truncated
use xmtp_common::snippet::Snippet;
bytes.snippet();                                                        // "abcdef.."
```
