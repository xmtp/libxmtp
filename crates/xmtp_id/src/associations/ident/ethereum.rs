use serde::{Deserialize, Serialize};
use std::fmt::Display;
use xmtp_cryptography::signature::{IdentifierValidationError, sanitize_evm_addresses};

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

    /// Whether the address is in the one form a signer is derived in: `0x`
    /// followed by exactly 40 lowercase hexadecimal characters.
    pub fn is_canonical(&self) -> bool {
        self.0.strip_prefix("0x").is_some_and(|hex| {
            hex.len() == 40 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
    }
}

impl Display for Ethereum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
