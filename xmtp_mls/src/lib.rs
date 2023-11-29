pub mod api_client_wrapper;
pub mod association;
pub mod builder;
pub mod client;
pub mod codecs;
mod configuration;
pub mod groups;
pub mod identity;
pub mod mock_xmtp_api_client;
pub mod owner;
mod proto_wrapper;
pub mod retry;
pub mod storage;
pub mod types;
pub mod utils;
pub mod verified_key_package;
mod xmtp_openmls_provider;

pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

// Inserts a model to the underlying data store
pub trait Store<StorageConnection> {
    fn store(&self, into: &StorageConnection) -> Result<(), StorageError>;
}

pub trait Fetch<Model> {
    type Key;
    fn fetch(&self, key: &Self::Key) -> Result<Option<Model>, StorageError>;
}

pub trait Delete<Model> {
    type Key;
    fn delete(&self, key: Self::Key) -> Result<usize, StorageError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Once;
    static INIT: Once = Once::new();

    /// Setup for tests
    pub fn setup() {
        INIT.call_once(|| {
            tracing_subscriber::fmt::init();
        })
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
