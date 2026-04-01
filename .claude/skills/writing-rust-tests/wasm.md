# WASM Compatibility

## Skipping Tests on WASM

```rust
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]  // sync_worker requires native tokio runtime
async fn test_device_sync() {
    tester!(alix1, sync_worker);
    tester!(bo, disable_workers);
    let (dm, msg) = alix1.test_talk_in_dm_with(&bo).await?;
    // ...
}
```

Use this for tests that depend on: filesystem, tokio multi-thread, device sync, toxiproxy, or native-only APIs.

## Conditional Compilation Macros

All from `xmtp_common`:

```rust
// Compile items for one target only
if_native! { use std::fs; }
if_wasm! { use web_sys::Storage; }

// Block form (multiple statements)
if_native! { @
    let file = std::fs::read("path")?;
    process(file);
}

// Expression-level branching
let val = wasm_or_native_expr! {
    wasm => web_time::Instant::now(),
    native => std::time::Instant::now(),
};

// Block-level branching
wasm_or_native! {
    wasm => { /* wasm code */ },
    native => { /* native code */ }
}
```

## Feature Gate Macros

```rust
if_d14n! { /* compiled only with d14n feature */ }
if_v3! { /* compiled only without d14n feature */ }
if_test! { /* compiled in #[cfg(test)] or feature = "test-utils" */ }
if_only_test! { /* compiled only in #[cfg(test)] */ }
if_not_test! { /* compiled only outside test */ }
```

## Cross-Platform Time

Always use `xmtp_common::time` instead of `std::time` for WASM compat:

```rust
use xmtp_common::time::{Duration, Instant, SystemTime, now_ns, now_ms, sleep, timeout};

// These dispatch to web_time on WASM, std::time on native
sleep(Duration::from_millis(100)).await;
timeout(Duration::from_secs(5), some_future).await?;
```

## Tester WASM Differences

The `Tester` struct has different `Send` bounds on WASM vs native (stream handles are `Send` on native only). The `tester!` macro handles this transparently.

**Source:** `crates/xmtp_common/src/macros.rs`, `crates/xmtp_common/src/time.rs`
