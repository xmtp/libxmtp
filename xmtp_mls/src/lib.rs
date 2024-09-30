#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod api;
pub mod builder;
pub mod client;
pub mod codecs;
pub mod configuration;
pub mod groups;
mod hpke;
pub mod identity;
mod identity_updates;
mod mutex_registry;
pub mod retry;
pub mod storage;
pub mod subscriptions;
pub mod types;
pub mod utils;
pub mod verified_key_package_v2;
mod xmtp_openmls_provider;

pub use client::{Client, Network};
use storage::StorageError;

pub use trait_impls::*;

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
mod trait_impls {
    pub use inner::*;

    // native, release
    #[cfg(not(test))]
    mod inner {
        use xmtp_proto::api_client::{
            ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams,
        };

        pub trait XmtpApi
        where
            Self: XmtpMlsClient
                + XmtpMlsStreams
                + XmtpIdentityClient
                + ClientWithMetadata
                + Send
                + Sync,
        {
        }
        impl<T> XmtpApi for T where
            T: XmtpMlsClient
                + XmtpMlsStreams
                + XmtpIdentityClient
                + ClientWithMetadata
                + Send
                + Sync
                + ?Sized
        {
        }
    }

    // test, native
    #[cfg(test)]
    mod inner {
        use xmtp_proto::api_client::{
            ClientWithMetadata, XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams,
        };

        pub trait XmtpApi
        where
            Self: XmtpMlsClient
                + XmtpMlsStreams
                + XmtpIdentityClient
                + crate::XmtpTestClient
                + ClientWithMetadata
                + Send
                + Sync,
        {
        }
        impl<T> XmtpApi for T where
            T: XmtpMlsClient
                + XmtpMlsStreams
                + XmtpIdentityClient
                + crate::XmtpTestClient
                + ClientWithMetadata
                + Send
                + Sync
                + ?Sized
        {
        }
    }
}

#[cfg(any(test, feature = "test-utils", feature = "bench"))]
#[trait_variant::make(XmtpTestClient: Send)]
pub trait LocalXmtpTestClient {
    async fn create_local() -> Self;
    async fn create_dev() -> Self;
}

pub use xmtp_id::InboxOwner;

/// Inserts a model to the underlying data store, erroring if it already exists
pub trait Store<StorageConnection> {
    fn store(&self, into: &StorageConnection) -> Result<(), StorageError>;
}

/// Inserts a model to the underlying data store, silent no-op on unique constraint violations
pub trait StoreOrIgnore<StorageConnection> {
    fn store_or_ignore(&self, into: &StorageConnection) -> Result<(), StorageError>;
}

/// Fetches a model from the underlying data store, returning None if it does not exist
pub trait Fetch<Model> {
    type Key;
    fn fetch(&self, key: &Self::Key) -> Result<Option<Model>, StorageError>;
}

/// Deletes a model from the underlying data store
pub trait Delete<Model> {
    type Key;
    fn delete(&self, key: Self::Key) -> Result<usize, StorageError>;
}

#[cfg(test)]
pub use self::tests::traced_test;

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use std::{io, sync::Arc};
    use tracing_subscriber::{
        filter::EnvFilter,
        fmt::{self, MakeWriter},
        prelude::*,
    };

    // Execute once before any tests are run
    #[ctor::ctor]
    fn setup() {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
    }

    thread_local! {
        pub static LOG_BUFFER: TestWriter = TestWriter::new();
    }

    /// Thread local writer which stores logs in memory
    pub struct TestWriter(Arc<Mutex<Vec<u8>>>);
    impl TestWriter {
        pub fn new() -> Self {
            Self(Arc::new(Mutex::new(vec![])))
        }

        pub fn as_string(&self) -> String {
            let buf = self.0.lock();
            String::from_utf8(buf.clone()).expect("Not valid UTF-8")
        }

        pub fn clear(&self) {
            let mut buf = self.0.lock();
            buf.clear();
        }
        pub fn flush(&self) {
            let mut buf = self.0.lock();
            std::io::Write::flush(&mut *buf).unwrap();
        }
    }

    impl io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut this = self.0.lock();
            Vec::<u8>::write(&mut this, buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            let mut this = self.0.lock();
            Vec::<u8>::flush(&mut this)
        }
    }

    impl Clone for TestWriter {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl MakeWriter<'_> for TestWriter {
        type Writer = TestWriter;

        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    /// Only works with current-thread
    pub fn traced_test<Fut>(f: impl Fn() -> Fut)
    where
        Fut: futures::Future<Output = ()>,
    {
        LOG_BUFFER.with(|buf| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .thread_name("tracing-test")
                .enable_time()
                .enable_io()
                .build()
                .unwrap();
            buf.clear();

            let subscriber = fmt::Subscriber::builder()
                .with_env_filter(format!("{}=debug", env!("CARGO_PKG_NAME")))
                .with_writer(buf.clone())
                .with_level(true)
                .with_ansi(false)
                .finish();

            let dispatch = tracing::Dispatch::new(subscriber);
            tracing::dispatcher::with_default(&dispatch, || {
                rt.block_on(f());
            });

            buf.clear();
        });
    }

    /// macro that can assert logs in tests.
    /// Note: tests that use this must be used in `traced_test` function
    /// and only with tokio's `current` runtime.
    #[macro_export]
    macro_rules! assert_logged {
        ( $search:expr , $occurrences:expr ) => {
            $crate::tests::LOG_BUFFER.with(|buf| {
                let lines = {
                    buf.flush();
                    buf.as_string()
                };
                let lines = lines.lines();
                let actual = lines.filter(|line| line.contains($search)).count();
                if actual != $occurrences {
                    panic!(
                        "Expected '{}' to be logged {} times, but was logged {} times instead",
                        $search, $occurrences, actual
                    );
                }
            })
        };
    }

    /// wrapper over assert!(matches!()) for Errors
    /// assert_err!(fun(), StorageError::Explosion)
    ///
    /// or the message variant,
    /// assert_err!(fun(), StorageError::Explosion, "the storage did not explode");
    #[macro_export]
    macro_rules! assert_err {
        ( $x:expr , $y:pat $(,)? ) => {
            assert!(matches!($x, Err($y)))
        };

        ( $x:expr, $y:pat $(,)?, $($msg:tt)+) => {{
            assert!(matches!($x, Err($y)), $($msg)+)
        }}
    }

    /// wrapper over assert! macros for Ok's
    ///
    /// Make sure something is Ok(_) without caring about return value.
    /// assert_ok!(fun());
    ///
    /// Against an expected value, e.g Ok(true)
    /// assert_ok!(fun(), true);
    ///
    /// or the message variant,
    /// assert_ok!(fun(), Ok(_), "the storage is not ok");
    #[macro_export]
    macro_rules! assert_ok {

        ( $e:expr ) => {
            assert_ok!($e,)
        };

        ( $e:expr, ) => {{
            use std::result::Result::*;
            match $e {
                Ok(v) => v,
                Err(e) => panic!("assertion failed: Err({:?})", e),
            }
        }};

        ( $x:expr , $y:expr $(,)? ) => {
            assert_eq!($x, Ok($y.into()));
        };

        ( $x:expr, $y:expr $(,)?, $($msg:tt)+) => {{
            assert_eq!($x, Ok($y.into()), $($msg)+);
        }}
    }
}
