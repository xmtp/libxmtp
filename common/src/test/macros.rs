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
