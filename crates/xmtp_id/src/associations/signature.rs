use alloy::signers::{SignerSync, local::LocalSigner};
use ed25519_dalek::{DigestSigner, Signature, VerifyingKey};
use prost::Message;
use sha2::{Digest as _, Sha512};
use std::array::TryFromSliceError;
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_cryptography::{
    CredentialSign, CredentialVerify, SignerError, SigningContextProvider,
    XmtpInstallationCredential,
};
use xmtp_proto::xmtp::message_contents::{
    SignedPrivateKey as LegacySignedPrivateKeyProto, signed_private_key,
};

use super::{
    unverified::{UnverifiedLegacyDelegatedSignature, UnverifiedRecoverableEcdsaSignature},
    verified_signature::VerifiedSignature,
};

#[derive(Debug, Error, ErrorCode)]
pub enum SignatureError {
    /// Malformed legacy key.
    ///
    /// Legacy key format is invalid. Not retryable.
    #[error("Malformed legacy key: {0}")]
    MalformedLegacyKey(String),
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
    LegacyDelegated,
    P256,
}

impl std::fmt::Display for SignatureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SignatureKind::Erc191 => write!(f, "erc-191"),
            SignatureKind::Erc1271 => write!(f, "erc-1271"),
            SignatureKind::InstallationKey => write!(f, "installation-key"),
            SignatureKind::LegacyDelegated => write!(f, "legacy-delegated"),
            SignatureKind::P256 => write!(f, "p256"),
        }
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum AccountIdError {
    /// Invalid chain ID.
    ///
    /// Chain ID is not a valid u64. Not retryable.
    #[error("Chain ID is not a valid u64")]
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

    pub fn get_chain_id_u64(&self) -> Result<u64, AccountIdError> {
        let stripped = self
            .chain_id
            .strip_prefix("eip155:")
            .ok_or(AccountIdError::MissingEip155Prefix)?;

        stripped
            .parse::<u64>()
            .map_err(|_| AccountIdError::InvalidChainId)
    }
}

/// Decode the `legacy_signed_private_key` to legacy private / public key pairs & sign the `signature_text` with the private key.
pub fn sign_with_legacy_key(
    signature_text: String,
    legacy_signed_private_key: Vec<u8>,
) -> Result<UnverifiedLegacyDelegatedSignature, SignatureError> {
    let legacy_signed_private_key_proto =
        LegacySignedPrivateKeyProto::decode(legacy_signed_private_key.as_slice())?;
    let signed_private_key::Union::Secp256k1(secp256k1) = legacy_signed_private_key_proto
        .union
        .ok_or(SignatureError::MalformedLegacyKey(
            "Missing secp256k1.union field".to_string(),
        ))?;
    let legacy_private_key = secp256k1.bytes;
    let signer = LocalSigner::from_slice(legacy_private_key.as_slice())?;
    let signature = signer.sign_message_sync(signature_text.as_bytes())?;

    let legacy_signed_public_key_proto =
        legacy_signed_private_key_proto
            .public_key
            .ok_or(SignatureError::MalformedLegacyKey(
                "Missing public_key field".to_string(),
            ))?;

    Ok(UnverifiedLegacyDelegatedSignature::new(
        UnverifiedRecoverableEcdsaSignature::new(signature.as_bytes().to_vec()),
        legacy_signed_public_key_proto,
    ))
}

#[derive(Clone, Debug)]
pub struct ValidatedLegacySignedPublicKey {
    pub(crate) account_address: String,
    pub(crate) serialized_key_data: Vec<u8>,
    pub(crate) wallet_signature: VerifiedSignature,
    pub(crate) public_key_bytes: Vec<u8>,
    pub(crate) created_ns: u64,
}

impl ValidatedLegacySignedPublicKey {
    fn header_text() -> String {
        let label = "Create Identity".to_string();
        format!("XMTP : {}", label)
    }

    fn body_text(serialized_legacy_key: &[u8]) -> String {
        hex::encode(serialized_legacy_key)
    }

    fn footer_text() -> String {
        "For more info: https://xmtp.org/signatures/".to_string()
    }

    pub fn text(serialized_legacy_key: &[u8]) -> String {
        format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(serialized_legacy_key),
            Self::footer_text()
        )
        .to_string()
    }

    pub fn account_address(&self) -> String {
        self.account_address.clone()
    }

    pub fn key_bytes(&self) -> Vec<u8> {
        self.public_key_bytes.clone()
    }

    pub fn created_ns(&self) -> u64 {
        self.created_ns
    }
}

#[cfg(test)]
mod tests {
    use super::SignatureError;
    use crate::scw_verifier::VerifierError;
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
        assert!(!SignatureError::MalformedLegacyKey("missing field".to_string()).is_retryable(),);
    }
}
