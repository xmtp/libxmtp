use crate::{
    configuration::{CIPHERSUITE, WELCOME_HPKE_LABEL},
    storage::sql_key_store::KEY_PACKAGE_REFERENCES,
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
    #[error("Key not found")]
    KeyNotFound,
}

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

pub fn decrypt_welcome(
    provider: &XmtpOpenMlsProvider,
    hpke_public_key: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, HpkeError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(ciphertext)?;

    let hpke_public_key_serialized = match hpke_public_key.tls_serialize_detached() {
        Ok(serialized) => serialized,
        Err(_) => return Err(HpkeError::KeyNotFound),
    };

    let hash_ref: Option<KeyPackageRef> = match provider
        .storage()
        .read(KEY_PACKAGE_REFERENCES, &hpke_public_key_serialized)
    {
        Ok(hash_ref) => hash_ref,
        Err(_) => return Err(HpkeError::KeyNotFound),
    };

    if let Some(hash_ref) = hash_ref {
        // With the hash reference we can read the key package.
        let key_package: Option<KeyPackageBundle> = match provider.storage().key_package(&hash_ref)
        {
            Ok(key_package) => key_package,
            Err(_) => return Err(HpkeError::KeyNotFound),
        };

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
