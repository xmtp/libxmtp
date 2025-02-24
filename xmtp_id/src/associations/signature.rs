use ed25519_dalek::{DigestSigner, Signature, VerifyingKey};
use ethers::signers::{LocalWallet, Signer};
use prost::Message;
use sha2::{Digest as _, Sha512};
use std::array::TryFromSliceError;
use thiserror::Error;
use xmtp_cryptography::{
    CredentialSign, CredentialVerify, SignerError, SigningContextProvider,
    XmtpInstallationCredential,
};
use xmtp_proto::xmtp::message_contents::{
    signed_private_key, SignedPrivateKey as LegacySignedPrivateKeyProto,
};

use super::{
    unverified::{UnverifiedLegacyDelegatedSignature, UnverifiedRecoverableEcdsaSignature},
    verified_signature::VerifiedSignature,
};

use ethers::core::k256::ecdsa::Signature as K256Signature;

#[derive(Debug, Error)]
pub enum SignatureError {
    // ethers errors
    #[error(transparent)]
    ProviderError(#[from] ethers::providers::ProviderError),
    #[error(transparent)]
    WalletError(#[from] ethers::signers::WalletError),
    #[error(transparent)]
    ECDSAError(#[from] ethers::types::SignatureError),
    #[error("Malformed legacy key: {0}")]
    MalformedLegacyKey(String),
    #[error(transparent)]
    CryptoSignatureError(#[from] xmtp_cryptography::signature::SignatureError),
    #[error(transparent)]
    VerifierError(#[from] crate::scw_verifier::VerifierError),
    #[error("ed25519 Signature failed {0}")]
    Ed25519Error(#[from] ed25519_dalek::SignatureError),
    #[error(transparent)]
    TryFromSliceError(#[from] TryFromSliceError),
    #[error("Signature validation failed")]
    Invalid,
    #[error(transparent)]
    AddressValidationError(#[from] xmtp_cryptography::signature::AddressValidationError),
    #[error("Invalid account address")]
    InvalidAccountAddress(#[from] rustc_hex::FromHexError),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    AccountIdError(#[from] AccountIdError),
    #[error(transparent)]
    Signer(#[from] SignerError),
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
}

impl std::fmt::Display for SignatureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SignatureKind::Erc191 => write!(f, "erc-191"),
            SignatureKind::Erc1271 => write!(f, "erc-1271"),
            SignatureKind::InstallationKey => write!(f, "installation-key"),
            SignatureKind::LegacyDelegated => write!(f, "legacy-delegated"),
        }
    }
}

#[derive(Debug, Error)]
pub enum AccountIdError {
    #[error("Chain ID is not a valid u64")]
    InvalidChainId,
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
pub async fn sign_with_legacy_key(
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
    let wallet: LocalWallet = hex::encode(legacy_private_key).parse::<LocalWallet>()?;
    let signature = wallet.sign_message(signature_text).await?;

    let legacy_signed_public_key_proto =
        legacy_signed_private_key_proto
            .public_key
            .ok_or(SignatureError::MalformedLegacyKey(
                "Missing public_key field".to_string(),
            ))?;

    Ok(UnverifiedLegacyDelegatedSignature::new(
        UnverifiedRecoverableEcdsaSignature::new(signature.to_vec()),
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
        Some(normalized) => normalized.to_vec(),
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
    use super::to_lower_s;
    use ethers::core::k256::{ecdsa::Signature as K256Signature, elliptic_curve::scalar::IsHigh};
    use ethers::signers::{LocalWallet, Signer};
    use rand::thread_rng;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_to_lower_s() {
        // Create a test wallet
        let wallet = LocalWallet::new(&mut thread_rng());

        // Sign a test message
        let message = "test message";
        let signature = wallet.sign_message(message).await.unwrap();
        let sig_bytes = signature.to_vec();

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
