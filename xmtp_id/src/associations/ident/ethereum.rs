use serde::{Deserialize, Serialize};
use std::fmt::Display;
use xmtp_cryptography::signature::{sanitize_evm_addresses, IdentifierValidationError};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Ethereum(pub String);

impl Ethereum {
    #[cfg(any(test, feature = "test-utils"))]
    pub fn rand() -> Self {
        Self(xmtp_common::rand_hexstring())
    }

    pub fn sanitize(self) -> Result<Self, IdentifierValidationError> {
        let mut sanitized = sanitize_evm_addresses(&[self.0])?;
        Ok(Self(sanitized.pop().expect("Always should be one")))
    }
}

impl Display for Ethereum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
