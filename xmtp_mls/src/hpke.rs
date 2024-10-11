use crate::{
    configuration::{CIPHERSUITE, WELCOME_HPKE_LABEL},
    retry::RetryableError,
    retryable,
    storage::sql_key_store::{SqlKeyStoreError, KEY_PACKAGE_REFERENCES},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};
use openmls::{
    ciphersuite::hash_ref::KeyPackageRef,
    prelude::tls_codec::{Deserialize, Error as TlsCodecError, Serialize},
};
use openmls::{
    ciphersuite::hpke::{decrypt_with_label, encrypt_with_label, Error as OpenmlsHpkeError},
    key_packages::KeyPackageBundle,
};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::{storage::StorageProvider, types::HpkeCiphertext};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HpkeError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("Storage error: {0}")]
    StorageError(#[from] SqlKeyStoreError),
    #[error("Key not found")]
    KeyNotFound,
}

impl RetryableError for HpkeError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::StorageError(storage) => retryable!(storage),
            _ => false,
        }
    }
}

/// Encrypt a welcome message using the provided HPKE private key
#[tracing::instrument(level = "trace", skip_all)]
pub fn encrypt_welcome(welcome_payload: &[u8], hpke_key: &[u8]) -> Result<Vec<u8>, HpkeError> {
    let crypto = RustCrypto::default();
    let ciphertext = encrypt_with_label(
        hpke_key,
        WELCOME_HPKE_LABEL,
        &[],
        welcome_payload,
        CIPHERSUITE,
        &crypto,
    )?;

    let serialized_ciphertext = ciphertext.tls_serialize_detached()?;

    Ok(serialized_ciphertext)
}

/// Decrypt a welcome message using the private key associated with the provided public key
pub fn decrypt_welcome(
    provider: &XmtpOpenMlsProvider,
    hpke_public_key: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, HpkeError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(ciphertext)?;

    let serialized_hpke_public_key = hpke_public_key.tls_serialize_detached()?;

    let hash_ref: Option<KeyPackageRef> = provider
        .storage()
        .read(KEY_PACKAGE_REFERENCES, &serialized_hpke_public_key)?;

    if let Some(hash_ref) = hash_ref {
        // With the hash reference we can read the key package.
        let key_package: Option<KeyPackageBundle> = provider.storage().key_package(&hash_ref)?;

        if let Some(kp) = key_package {
            return Ok(decrypt_with_label(
                kp.init_private_key(),
                WELCOME_HPKE_LABEL,
                &[],
                &ciphertext,
                CIPHERSUITE,
                &RustCrypto::default(),
            )?);
        }
    }

    Err(HpkeError::KeyNotFound)
}
