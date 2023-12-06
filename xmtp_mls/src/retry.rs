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

use std::time::Duration;

use smart_default::SmartDefault;

/// Specifies which errors are retryable.
/// All Errors are not retryable by-default.
pub trait RetryableError: std::error::Error {
    fn is_retryable(&self) -> bool {
        false
    }
}

// we use &T and make use of autoref specialization
impl<T> RetryableError for &T where T: std::error::Error {}

/// Options to specify how to retry a function
#[derive(SmartDefault, Debug, PartialEq, Eq, Copy, Clone)]
pub struct Retry {
    #[default = 3]
    retries: usize,
    #[default(_code = "std::time::Duration::from_millis(100)")]
    duration: std::time::Duration,
}

impl Retry {
    /// Get the number of retries this is configured with.
    pub fn retries(&self) -> usize {
        self.retries
    }

    /// Get the duration to wait between retries.
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

/// Builder for [`Retry`]
#[derive(Default, PartialEq, Eq, Copy, Clone)]
pub struct RetryBuilder {
    retries: Option<usize>,
    duration: Option<std::time::Duration>,
}

/// Builder for [`Retry`].
///
/// # Example
/// ```
/// use xmtp_mls::retry::RetryBuilder;
///
/// RetryBuilder::default()
///     .retries(5)
///     .duration(std::time::Duration::from_millis(1000))
///     .build();
/// ```
impl RetryBuilder {
    /// Specify the  of retries to allow
    pub fn retries(mut self, retries: usize) -> Self {
        self.retries = Some(retries);
        self
    }

    /// Specify the duration to wait before retrying again
    pub fn duration(mut self, duration: std::time::Duration) -> Self {
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

/// Retry a function, specifying the strategy with $retry.
///
/// # Example
///  ```
///  use thiserror::Error;
///  use xmtp_mls::{retry, retry::{RetryableError, Retry}};
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
/// fn fallable_fn(i: usize) -> Result<(), MyError> {
///     if i == 2 {
///         return Ok(());
///     }
///
///     Err(MyError::Retryable)
/// }
///
/// fn main() {
///      let mut i = 0;
///      retry!(Retry::default(), (|| -> Result<(), MyError> {
///         let res = fallable_fn(i);
///         i += 1;
///         res
///      })).unwrap();
///
///  }
/// ```
#[macro_export]
macro_rules! retry {
    ($retry: expr, $code: tt) => {{
        #[allow(unused)]
        use $crate::retry::RetryableError;
        let mut attempts = 0;
        loop {
            #[allow(clippy::redundant_closure_call)]
            match $code() {
                Ok(v) => break Ok(v),
                Err(e) => {
                    if (&e).is_retryable() && attempts < $retry.retries() {
                        log::debug!(
                            "retrying function that failed with error=`{}`",
                            e.to_string()
                        );
                        attempts += 1;
                        std::thread::sleep($retry.duration());
                    } else {
                        break Err(e);
                    }
                }
            }
        }
    }};
}

/// Retry but for an async context
/// ```
/// use xmtp_mls::{retry_async, retry::{RetryableError, Retry}};
/// use thiserror::Error;
/// use flume::bounded;
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
/// async fn fallable_fn(rx: &flume::Receiver<usize>) -> Result<(), MyError> {
///     if rx.recv_async().await.unwrap() == 2 {
///         return Ok(());
///     }
///     Err(MyError::Retryable)
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), MyError> {
///     
///     let (tx, rx) = flume::bounded(3);
///
///     for i in 0..3 {
///         tx.send(i).unwrap();
///     }
///     retry_async!(Retry::default(), (async {
///         fallable_fn(&rx.clone()).await
///     }))
/// }
/// ```
#[macro_export]
macro_rules! retry_async {
    ($retry: expr, $code: tt) => {{
        #[allow(unused)]
        use $crate::retry::RetryableError;
        let mut attempts = 0;
        loop {
            #[allow(clippy::redundant_closure_call)]
            match $code.await {
                Ok(v) => break Ok(v),
                Err(e) => {
                    if (&e).is_retryable() && attempts < $retry.retries() {
                        log::debug!("retrying function that failed with error={}", e.to_string());
                        attempts += 1;
                        tokio::time::sleep($retry.duration()).await;
                    } else {
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
        (&$error).is_retryable()
    }};
    ($error: expr) => {{
        use $crate::retry::RetryableError;
        (&$error).is_retryable()
    }};
}

// network errors should generally be retryable, unless there's a bug in our code
impl RetryableError for xmtp_proto::api_client::Error {
    fn is_retryable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thiserror::Error;

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

    #[test]
    fn it_retries_twice_and_succeeds() {
        crate::tests::setup();

        let mut i = 0;
        let mut test_fn = || -> Result<(), SomeError> {
            if i == 2 {
                return Ok(());
            }
            i += 1;
            retry_error_fn()?;
            Ok(())
        };

        retry!(Retry::default(), test_fn).unwrap();
    }

    #[test]
    fn it_works_with_random_args() {
        crate::tests::setup();

        let mut i = 0;
        let list = vec!["String".into(), "Foo".into()];
        let mut test_fn = || -> Result<(), SomeError> {
            if i == 2 {
                return Ok(());
            }
            i += 1;
            retryable_with_args(i, "Hello".to_string(), &list)
        };

        retry!(Retry::default(), test_fn).unwrap();
    }

    #[test]
    fn it_fails_on_three_retries() {
        crate::tests::setup();

        let result: Result<(), SomeError> = retry!(
            Retry::default(),
            (|| -> Result<(), SomeError> {
                retry_error_fn()?;
                Ok(())
            })
        );

        assert!(result.is_err())
    }

    #[test]
    fn it_only_runs_non_retryable_once() {
        crate::tests::setup();

        let mut attempts = 0;
        let mut test_fn = || -> Result<(), SomeError> {
            attempts += 1;
            Err(SomeError::DontRetryThis)
        };

        let _r = retry!(Retry::default(), test_fn);

        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn it_works_async() {
        crate::tests::setup();

        async fn retryable_async_fn(rx: &flume::Receiver<usize>) -> Result<(), SomeError> {
            let val = rx.recv_async().await.unwrap();
            if val == 2 {
                return Ok(());
            }
            // do some work
            tokio::time::sleep(std::time::Duration::from_nanos(100)).await;
            Err(SomeError::ARetryableError)
        }

        let (tx, rx) = flume::bounded(3);

        for i in 0..3 {
            tx.send(i).unwrap();
        }
        retry_async!(
            Retry::default(),
            (async { retryable_async_fn(&rx.clone()).await })
        )
        .unwrap();
        assert!(rx.is_empty());
    }

    #[tokio::test]
    async fn it_works_async_mut() {
        crate::tests::setup();

        async fn retryable_async_fn(data: &mut usize) -> Result<(), SomeError> {
            if *data == 2 {
                return Ok(());
            }
            *data += 1;
            // do some work
            tokio::time::sleep(std::time::Duration::from_nanos(100)).await;
            Err(SomeError::ARetryableError)
        }

        let mut data: usize = 0;
        retry_async!(
            Retry::default(),
            (async { retryable_async_fn(&mut data).await })
        )
        .unwrap();
    }
}
