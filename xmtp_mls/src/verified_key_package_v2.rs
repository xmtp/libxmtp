use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential},
    prelude::{
        tls_codec::{Deserialize, Error as TlsCodecError},
        KeyPackage, KeyPackageIn, KeyPackageVerifyError,
    },
};
use openmls_rust_crypto::RustCrypto;
use prost::{DecodeError, Message};
use thiserror::Error;

use crate::configuration::MLS_PROTOCOL_VERSION;
use xmtp_proto::xmtp::identity::MlsCredential;

#[derive(Debug, Error)]
pub enum KeyPackageVerificationError {
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("mls validation: {0}")]
    MlsValidation(#[from] KeyPackageVerifyError),
    #[error("invalid lifetime")]
    InvalidLifetime,
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

pub struct VerifiedKeyPackageV2 {
    pub inner: KeyPackage,
    pub credential: MlsCredential,
    pub installation_public_key: Vec<u8>,
}

impl VerifiedKeyPackageV2 {
    /// Create a new verified key package from its raw parts.
    pub fn new(
        kp: KeyPackage,
        credential: MlsCredential,
        installation_public_key: Vec<u8>,
    ) -> Self {
        Self {
            inner: kp,
            credential,
            installation_public_key,
        }
    }

    /// Create a verified key pacakge from TLS-Serialized bytes.
    pub fn from_bytes(
        crypto_provider: &RustCrypto,
        data: &[u8],
    ) -> Result<Self, KeyPackageVerificationError> {
        let kp_in: KeyPackageIn = KeyPackageIn::tls_deserialize_exact(data)?;
        let kp = kp_in.validate(crypto_provider, MLS_PROTOCOL_VERSION)?;

        kp.try_into()
    }
}

impl TryFrom<KeyPackage> for VerifiedKeyPackageV2 {
    type Error = KeyPackageVerificationError;

    fn try_from(kp: KeyPackage) -> Result<Self, Self::Error> {
        let leaf_node = kp.leaf_node();
        let basic_credential = BasicCredential::try_from(leaf_node.credential())?;
        let pub_key_bytes = leaf_node.signature_key().as_slice().to_vec();
        let credential = MlsCredential::decode(basic_credential.identity())?;

        if !kp.life_time().is_valid() {
            return Err(KeyPackageVerificationError::InvalidLifetime);
        }

        Ok(Self::new(kp, credential, pub_key_bytes))
    }
}
