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
use std::future::Future;
use storage::StorageError;
use tokio::task::JoinHandle;
pub use trait_impls::*;

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
mod trait_impls {
    pub use inner::*;

    // native, release
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
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

    // wasm32, release
    #[cfg(all(not(test), target_arch = "wasm32"))]
    mod inner {
        use xmtp_proto::api_client::{
            ClientWithMetadata, LocalXmtpIdentityClient, LocalXmtpMlsClient, LocalXmtpMlsStreams,
        };
        pub trait XmtpApi
        where
            Self: LocalXmtpMlsClient
                + LocalXmtpMlsStreams
                + LocalXmtpIdentityClient
                + ClientWithMetadata,
        {
        }

        impl<T> XmtpApi for T where
            T: LocalXmtpMlsClient
                + LocalXmtpMlsStreams
                + LocalXmtpIdentityClient
                + ClientWithMetadata
                + ?Sized
        {
        }
    }

    // test, native
    #[cfg(all(test, not(target_arch = "wasm32")))]
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

    // test, wasm32
    #[cfg(all(test, target_arch = "wasm32"))]
    mod inner {
        use xmtp_proto::api_client::{
            ClientWithMetadata, LocalXmtpIdentityClient, LocalXmtpMlsClient, LocalXmtpMlsStreams,
        };

        pub trait XmtpApi
        where
            Self: LocalXmtpMlsClient
                + LocalXmtpMlsStreams
                + LocalXmtpIdentityClient
                + crate::LocalXmtpTestClient
                + ClientWithMetadata,
        {
        }

        impl<T> XmtpApi for T where
            T: LocalXmtpMlsClient
                + LocalXmtpMlsStreams
                + LocalXmtpIdentityClient
                + crate::LocalXmtpTestClient
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

#[cfg(target_arch = "wasm32")]
fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    tokio::task::spawn_local(future)
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: 'static + Send,
{
    tokio::task::spawn(future)
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub async fn sleep(duration: chrono::Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub async fn sleep(duration: chrono::Duration) {
    tokio::time::sleep(duration).await
}

#[cfg(test)]
pub(crate) mod tests {
    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    // Capture traces in a variable that can be checked in tests, as well as outputting them to stdout on test failure
    #[cfg_attr(not(target_arch = "wasm32"), tracing_test::traced_test)]
    #[cfg(not(target_arch = "wasm32"))]
    fn setup() {
        // Capture logs (e.g. log::info!()) as traces too
        let _ = tracing_log::LogTracer::init_with_filter(log::LevelFilter::Debug);
    }

    /// Note: tests that use this must have the #[traced_test] attribute
    #[macro_export]
    macro_rules! assert_logged {
        ( $search:expr , $occurrences:expr ) => {
            logs_assert(|lines: &[&str]| {
                let actual = lines.iter().filter(|line| line.contains($search)).count();
                if actual != $occurrences {
                    return Err(format!(
                        "Expected '{}' to be logged {} times, but was logged {} times instead",
                        $search, $occurrences, actual
                    ));
                }
                Ok(())
            });
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
