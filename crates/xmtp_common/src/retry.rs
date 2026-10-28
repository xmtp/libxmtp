//! A retry strategy that works with rusts native [`std::error::Error`] type.
//!
//! TODO: Could make the impl of `RetryableError` trait into a proc-macro to auto-derive Retryable
//! on annotated enum variants.
//! ```ignore
//! #[derive(Debug, Error)]
//! enum ErrorFoo {
//!     #[error("I am retryable")]
//!     #[retryable]
//!     Retryable,
//!     #[error("Nested errors are retryable")]
//!     #[retryable(inherit)]
//!     NestedRetryable(AnotherErrorWithRetryableVariants),
//!     #[error("Always fail")]
//!     NotRetryable
//! }
//! ```

use crate::time::Duration;
use crate::{MaybeSend, MaybeSync};
use rand::RngExt;
use std::error::Error;
use std::sync::Arc;

// Rust 1.86 added Trait upcasting, so we can add these infallible conversions
// which is useful when getting error messages
impl From<Box<dyn RetryableError>> for Box<dyn Error> {
    fn from(retryable: Box<dyn RetryableError>) -> Box<dyn Error> {
        retryable
    }
}

// NOTE: From<> implementation is not possible here b/c of rust orphan rules (relaxed for Box
// types)
/// Convert an `Arc<[RetryableError]>` to a Standard Library `Arc<Error>`
pub fn arc_retryable_to_error(retryable: Arc<dyn RetryableError>) -> Arc<dyn Error> {
    retryable
}

pub type BoxedRetry = Retry<Box<dyn Strategy>>;

tokio::task_local! {
    static RETRY_DEADLINE: crate::time::Instant;
}

/// Run `future` so that every rate-limit wait of [`retry_async!`] inside it
/// ends by `deadline`. A wait that would pass it returns the rejection at once,
/// so a caller that times out at `deadline` still sees the delay. A nested
/// scope keeps the earlier deadline.
pub async fn with_retry_deadline<F: std::future::Future>(
    deadline: crate::time::Instant,
    future: F,
) -> F::Output {
    let deadline = RETRY_DEADLINE
        .try_with(|outer| (*outer).min(deadline))
        .unwrap_or(deadline);
    RETRY_DEADLINE.scope(deadline, future).await
}

/// True when a rate-limit wait of `wait` ends before the scoped deadline of
/// [`with_retry_deadline`], or when no deadline is scoped.
#[doc(hidden)]
pub fn fits_retry_deadline(wait: Duration) -> bool {
    RETRY_DEADLINE
        .try_with(|deadline| deadline.saturating_duration_since(crate::time::Instant::now()) > wait)
        .unwrap_or(true)
}

pub struct NotSpecialized;

/// Specifies which errors are retryable.
/// All Errors are not retryable by-default.
pub trait RetryableError<SP = NotSpecialized>: std::error::Error + MaybeSend + MaybeSync {
    fn is_retryable(&self) -> bool;

    /// The least time to wait before a retry, when the error says so. A
    /// rate-limit rejection carries one; [`retry_async!`] never retries sooner.
    fn rate_limit_delay(&self) -> Option<Duration> {
        None
    }
}

impl<T> RetryableError for &'_ T
where
    T: RetryableError,
{
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
    }

    fn rate_limit_delay(&self) -> Option<Duration> {
        (**self).rate_limit_delay()
    }
}

impl<E: RetryableError> RetryableError for Box<E> {
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
    }

    fn rate_limit_delay(&self) -> Option<Duration> {
        (**self).rate_limit_delay()
    }
}

impl RetryableError for core::convert::Infallible {
    fn is_retryable(&self) -> bool {
        unreachable!()
    }
}

/// Options to specify how to retry a function
#[derive(Debug, Clone)]
pub struct Retry<S = ExponentialBackoff> {
    retries: usize,
    strategy: S,
}

impl Default for Retry {
    fn default() -> Retry {
        Retry {
            retries: 5,
            strategy: ExponentialBackoff::default(),
        }
    }
}

impl<S: Strategy> Retry<S> {
    /// Get the number of retries this is configured with.
    pub fn retries(&self) -> usize {
        self.retries
    }

    /// The backoff strategy this retry uses.
    pub fn strategy(&self) -> &S {
        &self.strategy
    }

    pub fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration> {
        self.strategy.backoff(attempts, time_spent)
    }

    /// The backoff, but at least `floor`. `None` gives up.
    pub fn backoff_at_least(
        &self,
        attempts: usize,
        time_spent: crate::time::Instant,
        floor: Duration,
    ) -> Option<Duration> {
        self.strategy.backoff_at_least(attempts, time_spent, floor)
    }
}

impl<S: Strategy + 'static> Retry<S> {
    pub fn boxed(self) -> Retry<Box<dyn Strategy>> {
        Retry {
            strategy: Box::new(self.strategy),
            retries: self.retries,
        }
    }
}

/// The strategy interface
pub trait Strategy: MaybeSend + MaybeSync {
    /// A time that this retry should backoff
    /// Returns None when we should no longer backoff,
    /// despite possibly being below attempts
    fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration>;

    /// The wait before the next attempt when the error asks for at least
    /// `floor`. Returns None when the strategy gives up instead of waiting.
    fn backoff_at_least(
        &self,
        attempts: usize,
        time_spent: crate::time::Instant,
        floor: Duration,
    ) -> Option<Duration> {
        self.backoff(attempts, time_spent)
            .map(|wait| wait.max(floor))
    }
}

impl Strategy for () {
    fn backoff(&self, _attempts: usize, _time_spent: crate::time::Instant) -> Option<Duration> {
        Some(Duration::ZERO)
    }
}

impl<S: ?Sized + Strategy> Strategy for Box<S> {
    fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration> {
        (**self).backoff(attempts, time_spent)
    }

    fn backoff_at_least(
        &self,
        attempts: usize,
        time_spent: crate::time::Instant,
        floor: Duration,
    ) -> Option<Duration> {
        (**self).backoff_at_least(attempts, time_spent, floor)
    }
}

#[derive(Clone, Debug)]
pub struct ExponentialBackoff {
    /// The amount to multiply the duration on each subsequent attempt
    multiplier: u32,
    /// Duration to be multiplied
    duration: Duration,
    /// jitter to add randomness
    max_jitter: Duration,
    /// upper limit on time to wait for all retries
    total_wait_max: Duration,
    /// upper limit on time to wait between retries
    individual_wait_max: Duration,
}

impl ExponentialBackoff {
    pub fn builder() -> ExponentialBackoffBuilder {
        ExponentialBackoffBuilder::default()
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            // total wait time == two minutes
            multiplier: 3,
            duration: Duration::from_millis(50),
            total_wait_max: Duration::from_secs(120),
            individual_wait_max: Duration::from_secs(30),
            max_jitter: Duration::from_millis(25),
        }
    }
}

#[derive(Default)]
pub struct ExponentialBackoffBuilder {
    duration: Option<Duration>,
    max_jitter: Option<Duration>,
    multiplier: Option<u32>,
    total_wait_max: Option<Duration>,
}

impl ExponentialBackoffBuilder {
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn max_jitter(mut self, max_jitter: Duration) -> Self {
        self.max_jitter = Some(max_jitter);
        self
    }

    pub fn multiplier(mut self, multiplier: u32) -> Self {
        self.multiplier = Some(multiplier);
        self
    }

    pub fn total_wait_max(mut self, total_wait_max: Duration) -> Self {
        self.total_wait_max = Some(total_wait_max);
        self
    }

    pub fn build(self) -> ExponentialBackoff {
        ExponentialBackoff {
            duration: self.duration.unwrap_or(Duration::from_millis(25)),
            max_jitter: self.max_jitter.unwrap_or(Duration::from_millis(25)),
            multiplier: self.multiplier.unwrap_or(3),
            total_wait_max: self.total_wait_max.unwrap_or(Duration::from_secs(120)),
            individual_wait_max: Duration::from_secs(30),
        }
    }
}

impl Strategy for ExponentialBackoff {
    fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration> {
        if time_spent.elapsed() > self.total_wait_max {
            return None;
        }
        let mut duration = self.duration;
        for _ in 0..(attempts.saturating_sub(1)) {
            duration *= self.multiplier;
            if duration > self.individual_wait_max {
                duration = self.individual_wait_max;
            }
        }
        let distr = rand::distr::Uniform::new_inclusive(Duration::ZERO, self.max_jitter).unwrap();
        let jitter = rand::rng().sample(distr);
        let wait = duration + jitter;
        Some(wait)
    }

    /// Wait `max(floor, backoff)` only when the wait ends within the total
    /// budget. A longer wait gives up at once, so the caller still sees the
    /// error and its delay.
    fn backoff_at_least(
        &self,
        attempts: usize,
        time_spent: crate::time::Instant,
        floor: Duration,
    ) -> Option<Duration> {
        let wait = self.backoff(attempts, time_spent)?.max(floor);
        (time_spent.elapsed().saturating_add(wait) <= self.total_wait_max).then_some(wait)
    }
}

/// Builder for [`Retry`]
#[derive(Default, Debug, Copy, Clone)]
pub struct RetryBuilder<S> {
    retries: Option<usize>,
    strategy: S,
}

impl RetryBuilder<ExponentialBackoff> {
    pub fn new() -> Self {
        Self {
            retries: Some(5),
            strategy: ExponentialBackoff::default(),
        }
    }
}

/// Builder for [`Retry`].
///
/// # Example
/// ```ignore
/// use xmtp_common::retry::RetryBuilder;
///
/// RetryBuilder::default()
///     .retries(5)
///     .with_strategy(xmtp_common::ExponentialBackoff::default())
///     .build();
/// ```
impl<S: Strategy> RetryBuilder<S> {
    pub fn build(self) -> Retry<S> {
        let mut retry = Retry {
            retries: 5usize,
            strategy: self.strategy,
        };

        if let Some(retries) = self.retries {
            retry.retries = retries;
        }

        retry
    }

    /// Specify the  of retries to allow
    pub fn retries(mut self, retries: usize) -> Self {
        self.retries = Some(retries);
        self
    }

    pub fn with_strategy<St: Strategy>(self, strategy: St) -> RetryBuilder<St> {
        RetryBuilder {
            retries: self.retries,
            strategy,
        }
    }
}

impl Retry {
    /// Get the builder for [`Retry`]
    pub fn builder() -> RetryBuilder<ExponentialBackoff> {
        RetryBuilder::new()
    }
}

/// Retry but for an async context
/// ```
/// use xmtp_common::{retry_async, retry::{RetryableError, Retry}};
/// use thiserror::Error;
/// use tokio::sync::mpsc;
///
/// #[derive(Debug, Error)]
/// enum MyError {
///     #[error("A retryable error")]
///     Retryable,
///     #[error("An error we don't want to retry")]
///     NotRetryable
/// }
///
/// impl RetryableError for MyError {
///     fn is_retryable(&self) -> bool {
///         match self {
///             Self::Retryable => true,
///             _=> false,
///         }
///     }
/// }
///
/// async fn fallable_fn(rx: &mut mpsc::Receiver<usize>) -> Result<(), MyError> {
///     if rx.recv().await.unwrap() == 2 {
///         return Ok(());
///     }
///     Err(MyError::Retryable)
/// }
///
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() -> Result<(), MyError> {
///
///     let (tx, mut rx) = mpsc::channel(3);
///
///     for i in 0..3 {
///         tx.send(i).await.unwrap();
///     }
///     retry_async!(Retry::default(), (async {
///         fallable_fn(&mut rx).await
///     }))
/// }
/// ```
#[macro_export]
macro_rules! retry_async {
    ($retry: expr, $code: tt) => {{
        use tracing::Instrument as _;
        #[allow(unused)]
        use $crate::retry::RetryableError;
        let mut attempts = 0;
        let time_spent = $crate::time::Instant::now();
        let span = tracing::trace_span!("retry");
        loop {
            let span = span.clone();
            #[allow(clippy::redundant_closure_call)]
            let res = $code.instrument(span).await;
            match res {
                Ok(v) => break Ok(v),
                Err(e) => {
                    if (&e).is_retryable() && attempts < $retry.retries() {
                        // A single retry is normal transient behavior; keep it at debug so a
                        // flapping endpoint doesn't flood prod warn logs (only exhaustion warns).
                        tracing::debug!(
                            attempt = attempts,
                            "retrying function that failed with error={}",
                            e.to_string()
                        );
                        let wait = match (&e).rate_limit_delay() {
                            Some(floor) => $retry
                                .backoff_at_least(attempts, time_spent, floor)
                                .filter(|wait| $crate::retry::fits_retry_deadline(*wait)),
                            None => $retry.backoff(attempts, time_spent),
                        };
                        if let Some(d) = wait {
                            attempts += 1;
                            $crate::time::sleep(d).await;
                        } else {
                            tracing::warn!(
                                attempts,
                                elapsed_ms = time_spent.elapsed().as_millis(),
                                "retry strategy exceeded max wait time, giving up: {}",
                                e.to_string()
                            );
                            break Err(e);
                        }
                    } else {
                        tracing::trace!("error is not retryable. {}", e);
                        break Err(e);
                    }
                }
            }
        }
    }};
}

#[macro_export]
macro_rules! retryable {
    ($error: ident) => {{
        #[allow(unused)]
        use $crate::retry::RetryableError;
        $error.is_retryable()
    }};
    ($error: expr) => {{
        use $crate::retry::RetryableError;
        $error.is_retryable()
    }};
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use thiserror::Error;
    use tokio::sync::mpsc;

    #[derive(Debug, Error)]
    enum SomeError {
        #[error("this is a retryable error")]
        ARetryableError,
        #[error("Dont retry")]
        DontRetryThis,
    }

    impl RetryableError for SomeError {
        fn is_retryable(&self) -> bool {
            matches!(self, Self::ARetryableError)
        }
    }

    fn retry_error_fn() -> Result<(), SomeError> {
        Err(SomeError::ARetryableError)
    }

    fn retryable_with_args(foo: usize, name: String, list: &Vec<String>) -> Result<(), SomeError> {
        println!("I am {foo} of {name} with items {list:?}");
        Err(SomeError::ARetryableError)
    }

    #[xmtp_macro::test]
    async fn it_retries_twice_and_succeeds() {
        let mut i = 0;
        let mut test_fn = || -> Result<(), SomeError> {
            if i == 2 {
                return Ok(());
            }
            i += 1;
            retry_error_fn()?;
            Ok(())
        };

        retry_async!(Retry::default(), (async { test_fn() })).unwrap();
    }

    #[xmtp_macro::test]
    async fn it_works_with_random_args() {
        let mut i = 0;
        let list = vec!["String".into(), "Foo".into()];
        let mut test_fn = || -> Result<(), SomeError> {
            if i == 2 {
                return Ok(());
            }
            i += 1;
            retryable_with_args(i, "Hello".to_string(), &list)
        };

        retry_async!(Retry::default(), (async { test_fn() })).unwrap();
    }

    #[xmtp_macro::test]
    async fn it_fails_on_three_retries() {
        let closure = || -> Result<(), SomeError> {
            retry_error_fn()?;
            Ok(())
        };
        let result: Result<(), SomeError> = retry_async!(Retry::default(), (async { closure() }));

        assert!(result.is_err())
    }

    #[xmtp_macro::test]
    async fn it_only_runs_non_retryable_once() {
        let mut attempts = 0;
        let mut test_fn = || -> Result<(), SomeError> {
            attempts += 1;
            Err(SomeError::DontRetryThis)
        };

        let _r = retry_async!(Retry::default(), (async { test_fn() }));

        assert_eq!(attempts, 1);
    }

    #[xmtp_macro::test]
    async fn it_works_async() {
        async fn retryable_async_fn(rx: &mut mpsc::Receiver<usize>) -> Result<(), SomeError> {
            let val = rx.recv().await.unwrap();
            if val == 2 {
                return Ok(());
            }
            // do some work
            crate::time::sleep(core::time::Duration::from_nanos(100)).await;
            Err(SomeError::ARetryableError)
        }

        let (tx, mut rx) = mpsc::channel(3);

        for i in 0..3 {
            tx.send(i).await.unwrap();
        }
        retry_async!(
            Retry::default(),
            (async { retryable_async_fn(&mut rx).await })
        )
        .unwrap();
        assert!(rx.is_empty());
    }

    #[xmtp_macro::test]
    async fn it_works_async_mut() {
        async fn retryable_async_fn(data: &mut usize) -> Result<(), SomeError> {
            if *data == 2 {
                return Ok(());
            }
            *data += 1;
            // do some work
            crate::time::sleep(core::time::Duration::from_nanos(100)).await;
            Err(SomeError::ARetryableError)
        }

        let mut data: usize = 0;
        retry_async!(
            Retry::default(),
            (async { retryable_async_fn(&mut data).await })
        )
        .unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[derive(Debug, Error)]
    #[error("rate limited")]
    struct RateLimited(Duration);

    #[cfg(not(target_arch = "wasm32"))]
    impl RetryableError for RateLimited {
        fn is_retryable(&self) -> bool {
            true
        }

        fn rate_limit_delay(&self) -> Option<Duration> {
            Some(self.0)
        }
    }

    /// Run until the second attempt or until the loop gives up, on tokio's
    /// paused clock. Returns the start of each attempt.
    #[cfg(not(target_arch = "wasm32"))]
    async fn attempt_starts(
        delay: Duration,
    ) -> (Vec<tokio::time::Instant>, Result<(), RateLimited>) {
        tokio::time::pause();
        let mut starts = Vec::new();
        let result = retry_async!(
            Retry::default(),
            (async {
                starts.push(tokio::time::Instant::now());
                if starts.len() == 1 {
                    Err(RateLimited(delay))
                } else {
                    Ok(())
                }
            })
        );
        (starts, result)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_macro::test]
    async fn rate_limited_retry_waits_the_delay() {
        let delay = Duration::from_secs(5);
        let (starts, result) = attempt_starts(delay).await;
        assert!(result.is_ok());
        assert_eq!(starts.len(), 2);
        assert!(starts[1] - starts[0] >= delay);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_macro::test]
    async fn rate_limit_delay_beyond_the_budget_returns_at_once() {
        let (starts, result) = attempt_starts(Duration::from_secs(200)).await;
        assert_eq!(result.unwrap_err().0, Duration::from_secs(200));
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0], tokio::time::Instant::now());
    }

    /// On the real clock, a wait that fits the whole budget but not what is
    /// left of it returns at once.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_macro::test]
    async fn a_rate_limit_wait_past_the_remaining_budget_returns_at_once() {
        let retry = Retry::builder()
            .retries(5)
            .with_strategy(
                ExponentialBackoff::builder()
                    .duration(Duration::ZERO)
                    .max_jitter(Duration::ZERO)
                    .total_wait_max(Duration::from_millis(300))
                    .build(),
            )
            .build();
        let mut attempts = 0;
        let result: Result<(), RateLimited> = retry_async!(
            retry,
            (async {
                attempts += 1;
                crate::time::sleep(Duration::from_millis(200)).await;
                Err(RateLimited(Duration::from_millis(150)))
            })
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    /// A rate-limit wait that would pass a scoped deadline returns at once,
    /// even though it fits the retry budget.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_macro::test]
    async fn a_rate_limit_wait_past_a_scoped_deadline_returns_at_once() {
        let mut attempts = 0;
        let result: Result<(), RateLimited> = with_retry_deadline(
            crate::time::Instant::now() + Duration::from_secs(1),
            async {
                retry_async!(
                    Retry::default(),
                    (async {
                        attempts += 1;
                        Err(RateLimited(Duration::from_secs(5)))
                    })
                )
            },
        )
        .await;
        assert_eq!(result.unwrap_err().0, Duration::from_secs(5));
        assert_eq!(attempts, 1);
    }

    #[xmtp_macro::test]
    fn backoff_at_least_keeps_the_longer_backoff() {
        let retry = Retry::default();
        let time_spent = crate::time::Instant::now();
        let wait = retry
            .backoff_at_least(3, time_spent, Duration::from_millis(1))
            .unwrap();
        assert!(wait >= Duration::from_millis(450));
    }

    #[xmtp_macro::test]
    fn backoff_retry() {
        let backoff_retry = Retry::default();
        let time_spent = crate::time::Instant::now();
        assert!(backoff_retry.backoff(1, time_spent).unwrap().as_millis() - 50 <= 25);
        assert!(backoff_retry.backoff(2, time_spent).unwrap().as_millis() - 150 <= 25);
        assert!(backoff_retry.backoff(3, time_spent).unwrap().as_millis() - 450 <= 25);
    }
}
