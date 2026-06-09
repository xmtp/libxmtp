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

pub struct NotSpecialized;

/// Specifies which errors are retryable.
/// All Errors are not retryable by-default.
pub trait RetryableError<SP = NotSpecialized>: std::error::Error + MaybeSend + MaybeSync {
    fn is_retryable(&self) -> bool;
}

impl<T> RetryableError for &'_ T
where
    T: RetryableError,
{
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
    }
}

impl<E: RetryableError> RetryableError for Box<E> {
    fn is_retryable(&self) -> bool {
        (**self).is_retryable()
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

    pub fn backoff(&self, attempts: usize, time_spent: crate::time::Instant) -> Option<Duration> {
        self.strategy.backoff(attempts, time_spent)
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
                        if let Some(d) = $retry.backoff(attempts, time_spent) {
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

    #[xmtp_macro::test]
    fn backoff_retry() {
        let backoff_retry = Retry::default();
        let time_spent = crate::time::Instant::now();
        assert!(backoff_retry.backoff(1, time_spent).unwrap().as_millis() - 50 <= 25);
        assert!(backoff_retry.backoff(2, time_spent).unwrap().as_millis() - 150 <= 25);
        assert!(backoff_retry.backoff(3, time_spent).unwrap().as_millis() - 450 <= 25);
    }
}

/// Behavior tests for the `#[derive(Retryable)]` proc macro.
///
/// These exercise the generated `RetryableError` impl across every variant rule
/// in the design (default-false, `#[retry]`, `#[from]` forward, `#[retry(true|
/// false)]` override, `#[retry(when = ...)]`, the `#[retry(default = ...)]`
/// container baseline, named-field forward, and struct support).
#[cfg(test)]
mod derive_tests {
    use super::RetryableError;
    use thiserror::Error;
    use xmtp_macro::Retryable;

    /// An inner error that is itself retryable, for testing forwarding.
    #[derive(Debug, Error, Retryable)]
    enum Inner {
        #[error("retryable inner")]
        #[retry]
        Transient,
        #[error("permanent inner")]
        Permanent,
    }

    #[test]
    fn unannotated_variant_is_not_retryable() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("nope")]
            Plain,
        }
        assert!(!E::Plain.is_retryable());
    }

    #[test]
    fn retry_attribute_marks_variant_retryable() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("yes")]
            #[retry]
            Yes,
            #[error("no")]
            No,
        }
        assert!(E::Yes.is_retryable());
        assert!(!E::No.is_retryable());
    }

    #[test]
    fn retry_true_and_false_are_explicit() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("t")]
            #[retry(true)]
            T,
            #[error("f")]
            #[retry(false)]
            F,
        }
        assert!(E::T.is_retryable());
        assert!(!E::F.is_retryable());
    }

    #[test]
    fn from_variant_without_attr_uses_baseline() {
        // `#[from]` carries NO retry semantics: an unannotated wrapper variant
        // takes the container baseline (false), even when the inner error is
        // retryable. Forwarding is always explicit via `#[retry(inherit)]`.
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error(transparent)]
            Wrapped(#[from] Inner),
        }
        assert!(!E::Wrapped(Inner::Transient).is_retryable());
        assert!(!E::Wrapped(Inner::Permanent).is_retryable());
    }

    #[test]
    fn from_variant_with_inherit_forwards() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error(transparent)]
            #[retry(inherit)]
            Wrapped(#[from] Inner),
        }
        assert!(E::Wrapped(Inner::Transient).is_retryable());
        assert!(!E::Wrapped(Inner::Permanent).is_retryable());
    }

    #[test]
    fn retry_false_on_from_variant_is_false() {
        // Explicit false on a foreign-wrapping #[from] variant.
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error(transparent)]
            #[retry(false)]
            Parse(#[from] core::num::ParseIntError),
        }
        let err: E = "x".parse::<i32>().unwrap_err().into();
        assert!(!err.is_retryable());
    }

    #[test]
    fn default_true_covers_from_variants() {
        // Under `default = true`, a bare #[from] variant is `true` — it does
        // NOT forward. This lets retryable-unless-listed enums stay clean.
        #[derive(Debug, Error, Retryable)]
        #[retry(default = true)]
        enum E {
            #[error(transparent)]
            Wrapped(#[from] Inner),
        }
        // Inner::Permanent is non-retryable, but baseline-true wins: no forward.
        assert!(E::Wrapped(Inner::Permanent).is_retryable());
    }

    #[test]
    fn retry_inherit_forwards_without_from() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("inner: {0}")]
            #[retry(inherit)]
            Inner(Inner),
        }
        assert!(E::Inner(Inner::Transient).is_retryable());
        assert!(!E::Inner(Inner::Permanent).is_retryable());
    }

    #[test]
    fn when_expression_on_tuple_binds_this() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("generic: {0}")]
            #[retry(when = this.contains("database is locked"))]
            Generic(String),
        }
        assert!(E::Generic("database is locked".to_string()).is_retryable());
        assert!(!E::Generic("syntax error".to_string()).is_retryable());
    }

    #[test]
    fn when_expression_on_named_fields_binds_names() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("slow")]
            #[retry(when = *latency > 500)]
            Slow { latency: u64 },
        }
        assert!(E::Slow { latency: 900 }.is_retryable());
        assert!(!E::Slow { latency: 100 }.is_retryable());
    }

    #[test]
    fn container_default_true_flips_baseline() {
        #[derive(Debug, Error, Retryable)]
        #[retry(default = true)]
        enum E {
            #[error("a")]
            A,
            #[error("b")]
            B,
            #[error("c")]
            #[retry(false)]
            C,
        }
        assert!(E::A.is_retryable());
        assert!(E::B.is_retryable());
        assert!(!E::C.is_retryable());
    }

    #[test]
    fn struct_error_uses_container_default() {
        #[derive(Debug, Error, Retryable)]
        #[error("retryable struct")]
        #[retry(default = true)]
        struct Retryful;

        #[derive(Debug, Error, Retryable)]
        #[error("non-retryable struct")]
        struct NotRetryful;

        assert!(Retryful.is_retryable());
        assert!(!NotRetryful.is_retryable());
    }

    #[test]
    fn boxed_inner_forwards_through_blanket_impl() {
        // Box<Inner: RetryableError + Sized> gets the blanket Box<E> impl, so
        // the generated `inner.is_retryable()` resolves on it. (For
        // `Box<dyn RetryableError>` resolution is via auto-deref instead — the
        // blanket impl requires `E: Sized`.)
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("boxed")]
            #[retry(inherit)]
            Boxed(Box<Inner>),
        }
        assert!(E::Boxed(Box::new(Inner::Transient)).is_retryable());
        assert!(!E::Boxed(Box::new(Inner::Permanent)).is_retryable());
    }

    #[test]
    fn generic_enum_derives() {
        // The impl must carry the type's generics: impl<T: ...> RetryableError for E<T>.
        #[derive(Debug, Error, Retryable)]
        enum E<T: std::fmt::Debug + Send + Sync + 'static> {
            #[error("wrapped")]
            #[retry]
            Wrapped(T),
            #[error("plain")]
            Plain,
        }
        assert!(E::Wrapped("payload".to_string()).is_retryable());
        assert!(!E::<String>::Plain.is_retryable());
    }

    #[test]
    fn when_on_named_fields_binds_only_referenced_fields() {
        // `context` is not referenced by the expression; the generated arm must
        // not bind it (an unused binding fails -Dwarnings builds).
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("slow")]
            #[retry(when = *latency > 500)]
            Slow { latency: u64, context: String },
        }
        assert!(
            E::Slow {
                latency: 900,
                context: "ctx".into()
            }
            .is_retryable()
        );
        assert!(
            !E::Slow {
                latency: 100,
                context: "ctx".into()
            }
            .is_retryable()
        );
    }

    #[test]
    fn when_on_multi_tuple_binds_positionally() {
        // First field binds `this0`, second `this1`; only `this0` is referenced,
        // so `this1`'s position must not produce an unused binding.
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("pair")]
            #[retry(when = *this0 > 10)]
            Pair(u32, String),
        }
        assert!(E::Pair(11, "x".into()).is_retryable());
        assert!(!E::Pair(9, "x".into()).is_retryable());
    }

    #[test]
    fn when_on_unit_variant_is_allowed() {
        #[derive(Debug, Error, Retryable)]
        enum E {
            #[error("unit")]
            #[retry(when = 1 > 0)]
            Unit,
        }
        assert!(E::Unit.is_retryable());
    }

    #[test]
    fn empty_enum_derives() {
        // Uninhabited error enums must still derive a valid impl.
        #[derive(Debug, Error, Retryable)]
        enum Never {}

        fn assert_impl<T: RetryableError>() {}
        assert_impl::<Never>();
    }
}
