#![allow(unused)]

use crate::client::ClientError;
pub use crate::groups::test_support::TestError;

impl From<ClientError> for TestError {
    fn from(error: ClientError) -> Self {
        Self::Other(Box::new(error))
    }
}
