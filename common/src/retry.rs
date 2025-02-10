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

use crate::time::{Duration, Instant};
use arc_swap::ArcSwap;
use rand::Rng;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

pub struct NotSpecialized;

/// Specifies which errors are retryable.
/// All Errors are not retryable by-default.
pub trait RetryableError<SP = NotSpecialized>: std::error::Error {
    fn is_retryable(&self) -> bool;

    /// If the global scope this `Retry` operates in needs to be
    /// backed off on an error (e.g. a Rate Limit) this should return `true`
    fn needs_cooldown(&self) -> bool {
        false
    }
}

impl<T> RetryableError for &'_ T
where
    T: RetryableError,
{
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
    }

    fn needs_cooldown(&self) -> bool {
        (**self).needs_cooldown()
    }
}

impl<E: RetryableError> RetryableError for Box<E> {
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
    }

    fn needs_cooldown(&self) -> bool {
        (**self).needs_cooldown()
    }
}

/// Options to specify how to retry a function
#[derive(Debug, Clone)]
pub struct Retry<S = ExponentialBackoff, C = ()> {
    retries: usize,
    strategy: S,
    /// global cooldown for this retry strategy
    cooldown: C,
    /// whether we are currently in a cooldown period
    is_cooling: Arc<AtomicBool>,
    /// since when have we been cooling
    cooling_since: Arc<ArcSwap<crate::time::Instant>>,
    /// how many consecutive cooldown attempts
    cooldown_attempts: Arc<AtomicUsize>,
    /// the last error we got before cooling down
    last_err: Arc<AtomicBool>,
}

impl Default for Retry {
    fn default() -> Retry {
        Retry {
            retries: 5,
            strategy: ExponentialBackoff::default(),
            cooldown: (),
            cooling_since: Arc::new(ArcSwap::from_pointee(Instant::now())),
            is_cooling: Arc::new(AtomicBool::new(false)),
            cooldown_attempts: Arc::new(AtomicUsize::new(0usize)),
            // whether the last error was a cooldown error
            last_err: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<S: Strategy, C: Strategy> Retry<S, C> {
    /// Get the number of retries this is configured with.
    pub fn retries(&self) -> usize {
        self.retries
    }

    pub fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration> {
        self.strategy.backoff(attempts, time_spent)
    }

    pub async fn cooldown(&self) {
        if self.is_cooling.load(Ordering::SeqCst) {
            if let Some(c) = self.cooldown.backoff(
                self.cooldown_attempts.load(Ordering::SeqCst),
                **self.cooling_since.load(),
            ) {
                crate::time::sleep(c.saturating_sub(self.cooling_since.load().elapsed())).await;
                self.cooldown_off();
            }
        }
    }

    pub fn toggle_cooldown(&self) {
        if self.is_cooling.load(Ordering::SeqCst) {
            return;
        }
        self.cooling_since.store(crate::time::Instant::now().into());
        // if the last error was also a cooldown, increase attempts
        if self.last_err.load(Ordering::SeqCst) {
            let attempts = self.cooldown_attempts.fetch_add(1, Ordering::SeqCst);
            tracing::info!("Attempts: {}", attempts);
        } else {
            self.cooldown_attempts.store(0, Ordering::SeqCst);
        }
        self.is_cooling.store(true, Ordering::SeqCst);
    }

    fn cooldown_off(&self) {
        self.is_cooling.store(false, Ordering::SeqCst);
        if self.last_err.load(Ordering::SeqCst) {}
    }

    pub fn last_err(&self, err: impl RetryableError) {
        if !self.is_cooling.load(Ordering::SeqCst) {
            self.last_err.store(err.needs_cooldown(), Ordering::SeqCst)
        }
    }
}

impl<S: Strategy + 'static, C: Strategy + 'static> Retry<S, C> {
    pub fn boxed(self) -> Retry<Box<dyn Strategy>, Box<dyn Strategy>> {
        Retry {
            strategy: Box::new(self.strategy),
            cooldown: Box::new(self.cooldown),
            retries: self.retries,
            is_cooling: self.is_cooling,
            cooling_since: self.cooling_since,
            cooldown_attempts: self.cooldown_attempts,
            last_err: self.last_err,
        }
    }
}

/// The strategy interface
pub trait Strategy {
    /// A time that this retry should backoff
    /// Returns None when we should no longer backoff,
    /// despite possibly being below attempts
    fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration>;
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
            duration: Duration::from_millis(25),
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
            ..Default::default()
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
        let distr = rand::distributions::Uniform::new_inclusive(Duration::ZERO, &self.max_jitter);
        let jitter = rand::thread_rng().sample(distr);
        let wait = duration + jitter;
        Some(wait)
    }
}

/// Builder for [`Retry`]
#[derive(Default, Debug, Copy, Clone)]
pub struct RetryBuilder<S, C> {
    retries: Option<usize>,
    strategy: S,
    cooldown: C,
}

impl RetryBuilder<ExponentialBackoff, ()> {
    pub fn new() -> Self {
        Self {
            retries: Some(5),
            strategy: ExponentialBackoff::default(),
            cooldown: (),
        }
    }
}

/// Builder for [`Retry`].
///
/// # Example
/// ```
/// use xmtp_common::retry::RetryBuilder;
///
/// RetryBuilder::default()
///     .retries(5)
///     .with_strategy(ExponentialBackoff::default())
///     .build();
/// ```
impl<S: Strategy, C: Strategy> RetryBuilder<S, C> {
    pub fn build(self) -> Retry<S, C> {
        let mut retry = Retry {
            retries: 5usize,
            strategy: self.strategy,
            cooldown: self.cooldown,
            cooling_since: Arc::new(ArcSwap::from_pointee(Instant::now())),
            is_cooling: Arc::new(AtomicBool::new(false)),
            cooldown_attempts: Arc::new(AtomicUsize::new(0)),
            last_err: Arc::new(AtomicBool::new(false)),
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

    pub fn with_strategy<St: Strategy>(self, strategy: St) -> RetryBuilder<St, C> {
        RetryBuilder {
            retries: self.retries,
            strategy,
            cooldown: self.cooldown,
        }
    }

    /// Choose a cooldown strategy.
    /// the Cooldown strategy is independant of the retry strategy.
    /// The cooldown strategy additionally operates over the entire scope
    /// of `Retry`.
    /// This means any retry!() blocks using the same `Retry` strategy
    /// would also be paused for the duration of the cooldown backoff.
    /// By default this strategy resolves immediately (there is no cooldown period.)
    pub fn with_cooldown<Cd>(self, cooldown: Cd) -> RetryBuilder<S, Cd> {
        RetryBuilder {
            retries: self.retries,
            strategy: self.strategy,
            cooldown,
        }
    }
}

impl Retry {
    /// Get the builder for [`Retry`]
    pub fn builder() -> RetryBuilder<ExponentialBackoff, ()> {
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
/// #[tokio::main]
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
        let mut time_spent = $crate::time::Instant::now();
        let span = tracing::trace_span!("retry");
        loop {
            let span = span.clone();
            #[allow(clippy::redundant_closure_call)]
            $retry.cooldown().await;
            let res = $code.instrument(span).await;
            match res {
                Ok(v) => break Ok(v),
                Err(e) => {
                    $retry.last_err(&e);
                    if (&e).needs_cooldown() {
                        tracing::warn!("Hit {}, cooling down", e);
                        $retry.toggle_cooldown();
                        continue;
                    }
                    if (&e).is_retryable() && attempts < $retry.retries() {
                        tracing::warn!(
                            "retrying function that failed with error={}",
                            e.to_string()
                        );
                        if let Some(d) = $retry.backoff(attempts, time_spent) {
                            attempts += 1;
                            $crate::time::sleep(d).await;
                        } else {
                            tracing::warn!("retry strategy exceeded max wait time");
                            break Err(e);
                        }
                    } else {
                        tracing::info!("error is not retryable. {:?}:{}", e, e);
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
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
        println!("I am {} of {} with items {:?}", foo, name, list);
        Err(SomeError::ARetryableError)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn it_fails_on_three_retries() {
        let closure = || -> Result<(), SomeError> {
            retry_error_fn()?;
            Ok(())
        };
        let result: Result<(), SomeError> = retry_async!(Retry::default(), (async { closure() }));

        assert!(result.is_err())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn it_only_runs_non_retryable_once() {
        let mut attempts = 0;
        let mut test_fn = || -> Result<(), SomeError> {
            attempts += 1;
            Err(SomeError::DontRetryThis)
        };

        let _r = retry_async!(Retry::default(), (async { test_fn() }));

        assert_eq!(attempts, 1);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn backoff_retry() {
        let backoff_retry = Retry::default();

        assert!(backoff_retry.duration(1).as_millis() - 50 <= 25);
        assert!(backoff_retry.duration(2).as_millis() - 150 <= 25);
        assert!(backoff_retry.duration(3).as_millis() - 450 <= 25);
    }
}
