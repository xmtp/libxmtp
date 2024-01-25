use openmls::ciphersuite::hpke::{
    decrypt_with_label, encrypt_with_label, Error as OpenmlsHpkeError,
};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::types::HpkeCiphertext;
use openmls_traits::OpenMlsProvider;
use openmls_traits::{key_store::OpenMlsKeyStore, types::HpkePrivateKey};
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};

use crate::{
    configuration::{CIPHERSUITE, WELCOME_HPKE_LABEL},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Error)]
pub enum HpkeError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS codec error: {0}")]
    Tls(#[from] tls_codec::Error),
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
    let private_key = provider
        .key_store()
        .read::<HpkePrivateKey>(hpke_public_key)
        .ok_or(HpkeError::KeyNotFound)?;

    let ciphertext = HpkeCiphertext::tls_deserialize_exact(ciphertext)?;

    Ok(decrypt_with_label(
        private_key.to_vec().as_slice(),
        WELCOME_HPKE_LABEL,
        &[],
        &ciphertext,
        CIPHERSUITE,
        &RustCrypto::default(),
    )?)
}
