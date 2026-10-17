# Assertions and waiting

## Result assertions

```rust
use xmtp_common::{assert_err, assert_ok};

let val = assert_ok!(result);                          // unwraps; panics with Debug on Err
assert_ok!(result, expected);                          // assert_eq!(result, Ok(expected.into())); needs PartialEq + Debug
assert_err!(result, MyError::NotFound(..));            // a pattern; variants with fields need (..) or { .. }
assert_err!(result, MyError::NotFound(..), "missing item {}", id);
```

**Source:** `crates/xmtp_common/src/test/macros.rs`

## Wait helpers

Poll with `yield_now()` between attempts, 20 s timeout. Flat re-exports from
`xmtp_common`; there is no `xmtp_common::test::` path.

```rust
use xmtp_common::{wait_for_eq, wait_for_ge, wait_for_ok, wait_for_some};

wait_for_eq(|| async { group.member_count().await }, 3).await?;   // Result<(), Expired>
wait_for_ge(|| async { messages.len() }, 5).await?;               // Result<(), Expired>
let v = wait_for_ok(|| async { client.sync().await }).await?;      // Result<T, Expired>; the last error is discarded
let msg = wait_for_some(|| async { stream.next().await }).await;   // Option<T>; None on timeout, no ?
```

`xmtp_mls` extras in `crates/xmtp_mls/src/utils/test/mod.rs`:

```rust
wait_for_min_intents(&alix.context.db(), 2).await?;   // 5 s
let delivery = Delivery::new(None);                    // notify_one(); wait_for_delivery() -> Result<(), Expired>, 60 s
```

Never write a bare `sleep` loop.

## Worker metrics

```rust
alix.worker().register_interest(SyncMetric::PayloadSent, 1).wait().await?;
```

## Stream assertions

`assert_msg!(stream, "text")` and `assert_msg_exists!` are exported from
`crates/xmtp_mls/src/subscriptions/mod.rs`.

## Custom helpers

Mark a helper that panics with `#[track_caller]` so the failure points at the
test line. `xmtp_common::DebugDisplay` gives `slice.format_list()` and
`format_enumerated()` for readable collection output in messages.
