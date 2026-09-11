# Assertions & Waiting

## Result Assertions

```rust
use xmtp_common::{assert_ok, assert_err};

// assert_ok! — unwraps and returns the value on Ok, panics with debug on Err
let val = assert_ok!(some_result());

// assert_ok! with expected value
assert_ok!(some_result(), expected_value);
assert_ok!(some_result(), expected_value, "custom message: {}", ctx);

// assert_err! — asserts Err matching a pattern
assert_err!(some_result(), MyError::NotFound);
assert_err!(some_result(), MyError::NotFound, "should fail for missing item");
```

## Wait Helpers (eventually-consistent assertions)

All poll with `yield_now()` between attempts, 20-second timeout. From `xmtp_common::test`:

```rust
use xmtp_common::test::{wait_for_eq, wait_for_ok, wait_for_some, wait_for_ge};

// Poll until value equals expected
wait_for_eq(|| async { group.member_count().await }, 3).await?;

// Poll until result is Ok
wait_for_ok(|| async { client.sync().await }).await?;

// Poll until returns Some
let msg = wait_for_some(|| async { stream.next().await }).await;

// Poll until value >= threshold
wait_for_ge(|| async { messages.len() }, 5).await?;
```

## Worker Metric Assertions

For device sync tests, assert on worker completion:

```rust
alix.worker()
    .register_interest(SyncMetric::PayloadSent, 1)
    .wait().await?;
```

## Custom Assertion Patterns

Stream message assertions (defined locally in test modules):

```rust
// From subscriptions/stream_all/tests.rs
macro_rules! assert_msg {
    ($stream:expr, $expected:expr) => {
        let next = $stream.next().await.unwrap().unwrap();
        assert_eq!(
            String::from_utf8_lossy(next.decrypted_message_bytes.as_slice()),
            $expected
        );
    };
}
```

Track caller for custom assertion helpers:

```rust
#[track_caller]
fn assert_depends_on(env: &XmtpEnvelope, dependant: usize, commit: usize) {
    // ... custom assertion with good error location
}
```

**Source:** `crates/xmtp_common/src/test/macros.rs`, `crates/xmtp_common/src/test.rs`
