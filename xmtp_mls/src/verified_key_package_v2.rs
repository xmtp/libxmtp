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

#[derive(Debug, Error, Clone)]
pub enum KeyPackageVerificationError {
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("mls validation: {0}")]
    MlsValidation(#[from] KeyPackageVerifyError),
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// A wrapper around the MLS key package struct with some additional fields
#[derive(Clone, Debug)]
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

    /// Create a verified key package from TLS-Serialized bytes.
    pub fn from_bytes(
        crypto_provider: &RustCrypto,
        data: &[u8],
    ) -> Result<Self, KeyPackageVerificationError> {
        let kp_in: KeyPackageIn = KeyPackageIn::tls_deserialize_exact(data)?;
        let kp = kp_in.validate(crypto_provider, MLS_PROTOCOL_VERSION)?;

        kp.try_into()
    }

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner.leaf_node().signature_key().as_slice().to_vec()
    }

    pub fn hpke_init_key(&self) -> Vec<u8> {
        self.inner.hpke_init_key().as_slice().to_vec()
    }
}

impl TryFrom<KeyPackage> for VerifiedKeyPackageV2 {
    type Error = KeyPackageVerificationError;

    fn try_from(kp: KeyPackage) -> Result<Self, Self::Error> {
        let leaf_node = kp.leaf_node();
        let basic_credential = BasicCredential::try_from(leaf_node.credential().clone())?;
        let pub_key_bytes = leaf_node.signature_key().as_slice().to_vec();
        let credential = MlsCredential::decode(basic_credential.identity())?;

        Ok(Self::new(kp, credential, pub_key_bytes))
    }
}
