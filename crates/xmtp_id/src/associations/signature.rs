use ed25519_dalek::{DigestSigner, Signature, VerifyingKey};
use sha2::{Digest as _, Sha512};
use std::array::TryFromSliceError;
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_cryptography::{
    CredentialSign, CredentialVerify, SignerError, SigningContextProvider,
    XmtpInstallationCredential,
};

use alloy::signers::k256::ecdsa::Signature as K256Signature;

#[derive(Debug, Error, ErrorCode)]
pub enum SignatureError {
    #[error(transparent)]
    #[error_code(inherit)]
    CryptoSignatureError(#[from] xmtp_cryptography::signature::SignatureError),
    #[error(transparent)]
    #[error_code(inherit)]
    VerifierError(#[from] crate::scw_verifier::VerifierError),
    /// Ed25519 signature failed.
    ///
    /// Ed25519 signature verification failed. Not retryable.
    #[error("ed25519 Signature failed {0}")]
    Ed25519Error(#[from] ed25519_dalek::SignatureError),
    /// Slice conversion error.
    ///
    /// Byte slice conversion failed. Not retryable.
    #[error(transparent)]
    TryFromSliceError(#[from] TryFromSliceError),
    /// Signature validation failed.
    ///
    /// Signature did not verify. Not retryable.
    #[error("Signature validation failed")]
    Invalid,
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidationError(#[from] xmtp_cryptography::signature::IdentifierValidationError),
    /// URL parse error.
    ///
    /// URL parsing failed. Not retryable.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    /// Decode error.
    ///
    /// Protobuf decoding failed. Not retryable.
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    #[error_code(inherit)]
    AccountIdError(#[from] AccountIdError),
    /// Signer error.
    ///
    /// Cryptographic signer operation failed. Not retryable.
    #[error(transparent)]
    Signer(#[from] SignerError),
    /// Invalid public key.
    ///
    /// Public key is not valid. Not retryable.
    #[error("Invalid public key")]
    InvalidPublicKey,
    /// Invalid client data.
    ///
    /// Client data is malformed. Not retryable.
    #[error("client_data is invalid")]
    InvalidClientData,
    /// Alloy signer error.
    ///
    /// Ethereum signer failed. Not retryable.
    #[error(transparent)]
    SignerError(#[from] alloy::signers::Error),
    /// Alloy signature error.
    ///
    /// Ethereum signature parsing failed. Not retryable.
    #[error(transparent)]
    Signature(#[from] alloy::primitives::SignatureError),
}

impl RetryableError for SignatureError {
    fn is_retryable(&self) -> bool {
        match self {
            // Smart contract wallet verification goes through an RPC provider;
            // transient provider/IO failures must surface as retryable so the
            // welcome sync path does not permanently advance the cursor past
            // welcomes involving SCW users. See xmtp/libxmtp#3394.
            SignatureError::VerifierError(e) => e.is_retryable(),
            _ => false,
        }
    }
}

/// Xmtp Installation Credential for Specialized for XMTP Identity
pub struct InboxIdInstallationCredential;

pub struct InstallationKeyContext;
pub struct PublicContext;

impl CredentialSign<InboxIdInstallationCredential> for XmtpInstallationCredential {
    type Error = SignatureError;

    fn credential_sign<T: SigningContextProvider>(
        &self,
        text: impl AsRef<str>,
    ) -> Result<Vec<u8>, Self::Error> {
        let mut prehashed: Sha512 = Sha512::new();
        prehashed.update(text.as_ref());
        let context = self.with_context(T::context())?;
        let sig = context
            .try_sign_digest(prehashed)
            .map_err(SignatureError::from)?;
        Ok(sig.to_bytes().into())
    }
}

impl CredentialVerify<InboxIdInstallationCredential> for ed25519_dalek::VerifyingKey {
    type Error = SignatureError;

    fn credential_verify<T: SigningContextProvider>(
        &self,
        signature_text: impl AsRef<str>,
        signature_bytes: &[u8; 64],
    ) -> Result<(), Self::Error> {
        let signature = Signature::from_bytes(signature_bytes);
        let mut prehashed = Sha512::new();
        prehashed.update(signature_text.as_ref());
        self.verify_prehashed(prehashed, Some(T::context()), &signature)?;
        Ok(())
    }
}

impl SigningContextProvider for InstallationKeyContext {
    fn context() -> &'static [u8] {
        crate::constants::INSTALLATION_KEY_SIGNATURE_CONTEXT
    }
}

impl SigningContextProvider for PublicContext {
    fn context() -> &'static [u8] {
        crate::constants::PUBLIC_SIGNATURE_CONTEXT
    }
}

pub fn verify_signed_with_public_context(
    signature_text: impl AsRef<str>,
    signature_bytes: &[u8; 64],
    public_key: &[u8; 32],
) -> Result<(), SignatureError> {
    let verifying_key = VerifyingKey::from_bytes(public_key)?;
    verifying_key.credential_verify::<PublicContext>(signature_text, signature_bytes)
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignatureKind {
    // We might want to have some sort of LegacyErc191 Signature Kind for the `CreateIdentity` signatures only
    Erc191,
    Erc1271,
    InstallationKey,
    P256,
}

impl std::fmt::Display for SignatureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SignatureKind::Erc191 => write!(f, "erc-191"),
            SignatureKind::Erc1271 => write!(f, "erc-1271"),
            SignatureKind::InstallationKey => write!(f, "installation-key"),
            SignatureKind::P256 => write!(f, "p256"),
        }
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum AccountIdError {
    /// Invalid chain ID.
    ///
    /// Chain ID is not a u64 in canonical decimal form. Not retryable.
    #[error("Chain ID is not a u64 in canonical decimal form")]
    InvalidChainId,
    /// Missing EIP-155 prefix.
    ///
    /// Chain ID is not prefixed with `eip155:`. Not retryable.
    #[error("Chain ID is not prefixed with eip155:")]
    MissingEip155Prefix,
}

// CAIP-10[https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md]
#[derive(Debug, Clone, PartialEq)]
pub struct AccountId {
    pub(crate) chain_id: String,
    pub(crate) account_address: String,
}

impl AccountId {
    pub fn new(chain_id: String, account_address: String) -> Self {
        AccountId {
            chain_id,
            account_address,
        }
    }

    pub fn new_evm(chain_id: u64, account_address: String) -> Self {
        Self::new(format!("eip155:{}", chain_id), account_address)
    }

    pub fn is_evm_chain(&self) -> bool {
        self.chain_id.starts_with("eip155")
    }

    pub fn get_account_address(&self) -> &str {
        &self.account_address
    }

    pub fn get_chain_id(&self) -> &str {
        &self.chain_id
    }

    /// The chain id of an `eip155` account, whose reference must be the
    /// chain id in canonical decimal form: no sign, no leading zero.
    pub fn get_chain_id_u64(&self) -> Result<u64, AccountIdError> {
        let reference = self
            .chain_id
            .strip_prefix("eip155:")
            .ok_or(AccountIdError::MissingEip155Prefix)?;
        reference
            .parse::<u64>()
            .ok()
            .filter(|chain_id| chain_id.to_string() == reference)
            .ok_or(AccountIdError::InvalidChainId)
    }
}

/// Converts a signature to use the lower-s value to prevent signature malleability
pub fn to_lower_s(sig_bytes: &[u8]) -> Result<Vec<u8>, SignatureError> {
    // Check if we have a recovery id byte
    let (sig_data, recovery_id) = match sig_bytes.len() {
        64 => (sig_bytes, None),                       // No recovery id
        65 => (&sig_bytes[..64], Some(sig_bytes[64])), // Recovery id present
        _ => return Err(SignatureError::Invalid),
    };

    // Parse the signature bytes into a K256Signature
    let sig = K256Signature::try_from(sig_data)?;

    // If s is already normalized (lower-s), return the original bytes
    let normalized = match sig.normalize_s() {
        None => sig_data.to_vec(),
        Some(normalized) => normalized.to_bytes().to_vec(),
    };

    // Add back recovery id if it was present
    if let Some(rid) = recovery_id {
        let mut result = normalized;
        result.push(rid);
        Ok(result)
    } else {
        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::SignatureError;
    use super::to_lower_s;
    use crate::scw_verifier::VerifierError;
    use alloy::signers::k256::ecdsa::Signature as K256Signature;
    use alloy::signers::k256::elliptic_curve::scalar::IsHigh;
    use alloy::signers::{SignerSync, local::LocalSigner};
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_common::RetryableError;

    #[xmtp_common::test]
    fn test_signature_error_verifier_retryable_propagates() {
        // Retryable verifier error (NoVerifier) should make SignatureError retryable.
        let err = SignatureError::VerifierError(VerifierError::NoVerifier("eip155:1".to_string()));
        assert!(
            err.is_retryable(),
            "SignatureError wrapping a retryable VerifierError must be retryable"
        );
    }

    #[xmtp_common::test]
    fn test_signature_error_verifier_non_retryable_propagates() {
        // Non-retryable verifier error (MalformedEipUrl) should stay non-retryable.
        let err = SignatureError::VerifierError(VerifierError::MalformedEipUrl);
        assert!(
            !err.is_retryable(),
            "SignatureError wrapping a non-retryable VerifierError must not be retryable"
        );
    }

    #[xmtp_common::test]
    fn test_signature_error_non_verifier_variants_not_retryable() {
        // Terminal crypto/parse failures are never retryable.
        assert!(!SignatureError::Invalid.is_retryable());
        assert!(!SignatureError::InvalidPublicKey.is_retryable());
        assert!(!SignatureError::InvalidClientData.is_retryable());
    }

    #[xmtp_common::test]
    fn test_to_lower_s() {
        // Create a test wallet
        let signer = LocalSigner::random();

        // Sign a test message
        let message = "test message";
        let signature = signer.sign_message_sync(message.as_bytes()).unwrap();
        let sig_bytes: Vec<u8> = signature.into();

        // Test normalizing an already normalized signature
        let normalized = to_lower_s(&sig_bytes).unwrap();
        assert_eq!(
            normalized, sig_bytes,
            "Already normalized signature should not change"
        );

        // Create a signature with high-s value by manipulating the s component
        let mut high_s_sig = sig_bytes.clone();
        // Flip bits in the s component (last 32 bytes) to create a high-s value
        for byte in high_s_sig[32..64].iter_mut() {
            *byte = !*byte;
        }

        // Normalize the manipulated signature
        let normalized_high_s = to_lower_s(&high_s_sig).unwrap();
        assert_ne!(
            normalized_high_s, high_s_sig,
            "High-s signature should be normalized"
        );

        // Verify the normalized signature is valid
        let recovered_sig = K256Signature::try_from(&normalized_high_s.as_slice()[..64]).unwrap();
        let is_high: bool = recovered_sig.s().is_high().into();
        assert!(!is_high, "Normalized signature should have low-s value");
    }

    #[wasm_bindgen_test(unsupported = test)]
    fn test_invalid_signature() {
        // Test with invalid signature bytes
        let invalid_sig = vec![0u8; 65];
        let result = to_lower_s(&invalid_sig);
        assert!(result.is_err(), "Should fail with invalid signature");

        // Test with wrong length
        let wrong_length = vec![0u8; 63];
        let result = to_lower_s(&wrong_length);
        assert!(result.is_err(), "Should fail with wrong length");
    }
}
