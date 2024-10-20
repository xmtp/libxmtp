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

use rand::Rng;

/// Specifies which errors are retryable.
/// All Errors are not retryable by-default.
pub trait RetryableError: std::error::Error {
    fn is_retryable(&self) -> bool;
}

/// Options to specify how to retry a function
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Retry {
    retries: usize,
    duration: core::time::Duration,
    // The amount to multiply the duration on each subsequent attempt
    multiplier: u32,
    max_jitter_ms: usize,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            retries: 5,
            duration: core::time::Duration::from_millis(50),
            multiplier: 3,
            max_jitter_ms: 25,
        }
    }
}

impl Retry {
    /// Get the number of retries this is configured with.
    pub fn retries(&self) -> usize {
        self.retries
    }

    /// Get the duration to wait between retries.
    /// Multiples the duration by the multiplier for each subsequent attempt
    /// and adds a random jitter to avoid repeated collisions
    pub fn duration(&self, attempts: usize) -> core::time::Duration {
        let mut duration = self.duration;
        for _ in 0..attempts - 1 {
            duration *= self.multiplier;
        }

        let jitter = rand::thread_rng().gen_range(0..=self.max_jitter_ms);
        duration + core::time::Duration::from_millis(jitter as u64)
    }
}

/// Builder for [`Retry`]
#[derive(Default, PartialEq, Eq, Copy, Clone)]
pub struct RetryBuilder {
    retries: Option<usize>,
    duration: Option<core::time::Duration>,
}

/// Builder for [`Retry`].
///
/// # Example
/// ```
/// use xmtp_mls::retry::RetryBuilder;
///
/// RetryBuilder::default()
///     .retries(5)
///     .duration(core::time::Duration::from_millis(1000))
///     .build();
/// ```
impl RetryBuilder {
    /// Specify the  of retries to allow
    pub fn retries(mut self, retries: usize) -> Self {
        self.retries = Some(retries);
        self
    }

    /// Specify the duration to wait before retrying again
    pub fn duration(mut self, duration: core::time::Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Build the Retry Strategy
    pub fn build(self) -> Retry {
        let mut retry = Retry::default();

        if let Some(retries) = self.retries {
            retry.retries = retries;
        }

        if let Some(duration) = self.duration {
            retry.duration = duration;
        }

        retry
    }
}

impl Retry {
    /// Get the builder for [`Retry`]
    pub fn builder() -> RetryBuilder {
        RetryBuilder::default()
    }
}

/// Retry but for an async context
/// ```
/// use xmtp_mls::{retry_async, retry::{RetryableError, Retry}};
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
        let span = tracing::trace_span!("retry");
        loop {
            let span = span.clone();
            #[allow(clippy::redundant_closure_call)]
            let res = $code.instrument(span).await;
            match res {
                Ok(v) => break Ok(v),
                Err(e) => {
                    if (&e).is_retryable() && attempts < $retry.retries() {
                        tracing::warn!(
                            "retrying function that failed with error={}",
                            e.to_string()
                        );
                        attempts += 1;
                        $crate::sleep($retry.duration(attempts)).await;
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

// network errors should generally be retryable, unless there's a bug in our code
impl RetryableError for xmtp_proto::api_client::Error {
    fn is_retryable(&self) -> bool {
        true
    }
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
            crate::sleep(core::time::Duration::from_nanos(100)).await;
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
            crate::sleep(core::time::Duration::from_nanos(100)).await;
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
