#![recursion_limit = "256"]
pub mod api;
pub mod builder;
pub mod client;
pub mod codecs;
pub mod configuration;
pub mod credential;
pub mod groups;
mod hpke;
pub mod identity;
mod identity_updates;
pub mod owner;
pub mod retry;
pub mod storage;
pub mod subscriptions;
pub mod types;
pub mod utils;
pub mod verified_key_package;
pub mod verified_key_package_v2;
mod xmtp_openmls_provider;

pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

/// XMTP Api Super Trait
/// Implements all Trait Network APIs for convenience.
#[cfg(not(test))]
pub trait XmtpApi
where
    Self: XmtpMlsClient + XmtpIdentityClient,
{
}
#[cfg(not(test))]
impl<T> XmtpApi for T where T: XmtpMlsClient + XmtpIdentityClient + ?Sized {}

#[cfg(test)]
pub trait XmtpApi
where
    Self: XmtpMlsClient + XmtpIdentityClient + XmtpTestClient,
{
}

#[cfg(test)]
impl<T> XmtpApi for T where T: XmtpMlsClient + XmtpIdentityClient + XmtpTestClient + ?Sized {}

#[cfg(test)]
#[async_trait::async_trait]
pub trait XmtpTestClient {
    async fn create_local() -> Self;
    async fn create_dev() -> Self;
}

pub trait InboxOwner {
    /// Get address of the wallet.
    fn get_address(&self) -> String;
    /// Sign text with the wallet.
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

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
mod tests {
    use log::LevelFilter;
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    use tracing_test::traced_test;

    // Execute once before any tests are run
    // Capture traces in a variable that can be checked in tests, as well as outputting them to stdout on test failure
    // #[traced_test]
    #[ctor::ctor]
    fn setup() {
        // Capture logs (e.g. log::info!()) as traces too
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env())
            .init();
        // let _ = tracing_log::LogTracer::init_with_filter(LevelFilter::Debug);
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
            assert!(matches!($x, Err($y)));
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
