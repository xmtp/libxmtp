# Async and runtime

Shared crates compile for native and wasm. Wasm has no threads and no `Send`
futures. Every rule here follows from that.

## Time

```rust
use xmtp_common::time::{self, Duration, Expired, Instant};

let started = Instant::now();                       // web_time on wasm, std on native
let deadline_ns = time::now_ns() + NS_IN_MIN;       // i64 nanoseconds; now_secs() -> i64
time::sleep(Duration::from_millis(50)).await;
match time::timeout(Duration::from_secs(5), fut).await {
    Ok(value) => value,
    Err(Expired) => return Err(err.into()),         // Expired implements ErrorCode
}
```

A `Duration` in a constant or signature may stay `std::time::Duration`; it is a
value type. `Instant`, `SystemTime`, and every timer come from
`xmtp_common::time`. Nanosecond constants `NS_IN_SEC`, `NS_IN_MIN`,
`NS_IN_HOUR`, `NS_IN_DAY`, `NS_IN_30_DAYS` sit at the `xmtp_common` root.

Periodic workers use a jittered interval so instances do not fire together.
The first tick is immediate; skip it for sleep-then-run.

```rust
// crates/xmtp_mls/src/worker/device_sync/worker.rs
let mut ticks = time::jittered_interval_stream(base, jitter);
let _ = ticks.next().await;
while ticks.next().await.is_some() { /* work */ }

// apps/backend/src/auth/jwks.rs: a worker that computes its own sleeps
time::sleep(period + time::rand_offset(period / JITTER_DIVISOR)).await;
```

`#[xmtp_common::timeout(d)]` is for tests only. It panics on expiry.

## Tasks and cancellation

```rust
use xmtp_common::{AbortHandle, StreamHandle, StreamHandleError, spawn};

// No readiness signal
let handle = spawn(None, async move { worker.run().await });

// With one: the task owns tx and sends () once its stream is subscribed
let (tx, rx) = tokio::sync::oneshot::channel();
let mut handle = spawn(Some(rx), xmtp_common::bind_task_hub(async move {
    let stream = subscribe().await;
    let _ = tx.send(());
    drive(stream).await
}));
handle.wait_for_ready().await;
```

Keep the handle. `handle.end()` aborts. `handle.end_and_wait().await` aborts
and joins; `Err(Cancelled)` is the normal result. `handle.abort_handle()`
returns a `Box<dyn AbortHandle>` to store in a struct
(`crates/xmtp_api_backend/src/queries/bidi.rs`). Wrap a long-lived spawned
future in `xmtp_common::bind_task_hub` so Sentry breadcrumbs follow the task.

Use `xmtp_common::task::yield_now()` in polling loops. Never `tokio::spawn` or
`tokio::task` in a shared crate.

## Retry

```rust
use xmtp_common::{ExponentialBackoff, Retry, Strategy, retry_async};

// Default: 5 retries, 50 ms base, x3, 25 ms jitter, 120 s total cap
let state = retry_async!(Retry::default(), (async { self.fetch_state().await }))?;

// Tuned backoff. Every number is a named constant in xmtp_configuration.
let backoff = ExponentialBackoff::builder()
    .duration(Duration::from_millis(SYNC_BACKOFF_WAIT_MS.into()))
    .total_wait_max(Duration::from_secs(SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS.into()))
    .max_jitter(Duration::from_millis(SYNC_JITTER_MS.into()))
    .build();
let retry = Retry::builder().retries(3).with_strategy(backoff).build();

// Fixed delay: implement Strategy (apps/backend/src/auth/jwks.rs)
impl Strategy for FixedRetry {
    fn backoff(&self, _attempts: usize, _since: Instant) -> Option<Duration> {
        Some(JWKS_STARTUP_RETRY_DELAY)
    }
}
```

The async block sits in parentheses; the macro takes one token tree. It retries
only while `err.is_retryable()` and logs each attempt. There is no `retry_sync!`.

## `Send` bounds and async traits

```rust
use xmtp_common::{BoxDynError, BoxDynFuture, MaybeSend, MaybeSync};

#[xmtp_common::async_trait]                         // async_trait(?Send) on wasm
pub trait AuthCallback: MaybeSend + MaybeSync {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError>;
}
type OpenFuture = BoxDynFuture<'static, Result<Opened, NetworkError>>;
```

`MaybeSend` is `Send` on native and a no-op on wasm. Put the attribute on the
trait and on every `impl`. `BoxDynError` is `Box<dyn Error + Send + Sync>` on
native and `Box<dyn Error>` on wasm. `BoxDynStream` follows the same rule.

## Platform splits

Use the macros, not raw `#[cfg]`. The item form takes items; the `@` form takes
statements.

```rust
xmtp_common::if_native! { pub fn with_streams(self) -> Self { .. } }
xmtp_common::if_wasm! { pub fn with_streams(self) -> Self { .. } }
xmtp_common::wasm_or_native! {
    native => { tokio::time::sleep(d).await },
    wasm => { gloo_timers::future::sleep(d).await },
}
let dir = xmtp_common::wasm_or_native_expr! { native => temp_dir(), wasm => "test_db".into() };
xmtp_common::if_test! { pub use test::*; }          // cfg(any(test, feature = "test-utils"))
xmtp_common::if_only_test! { mod fixtures; }        // cfg(test)
```

## Locks

`parking_lot::Mutex` for short synchronous state. Never hold a guard across
`.await`. A non-poisoning lock does not make panicking code safe.

## HTTP

```rust
let client = xmtp_common::http::client_builder().timeout(FETCH_TIMEOUT).build()?;
```

`.clippy.toml` forbids `reqwest::Client::new`, `Client::builder`, and
`ClientBuilder::new`. The helpers pin webpki roots on Android, where a default
client aborts the process on its first TLS connection.

## Rate limiting

`xmtp_common::rate_limit::Bucket::new(rate, burst)`: `take() -> bool`,
`refund()`, `wait() -> Duration`. Used for stream admission
(`apps/backend/src/stream/session.rs`) and client update budgets.
