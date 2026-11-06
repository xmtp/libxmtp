//! Compile only in unit tests or with the `test-hooks` feature.
#![cfg(any(test, feature = "test-hooks"))]

pub use crate::groups::test_mocks::*;
pub use crate::identity_lifecycle::test_mocks::*;
