use crate::{
    configuration::{CIPHERSUITE, WELCOME_HPKE_LABEL},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};
use openmls::ciphersuite::hpke::{
    decrypt_with_label, encrypt_with_label, Error as OpenmlsHpkeError,
};
use openmls::prelude::tls_codec::{Deserialize, Error as TlsCodecError, Serialize};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::types::HpkeCiphertext;
use openmls_traits::OpenMlsProvider;
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

    match provider
        .storage()
        .read_list(WELCOME_HPKE_LABEL.as_bytes(), hpke_public_key)
    {
        Ok(private_key) => Ok(decrypt_with_label(
            serde_json::from_slice::<&[u8]>(&private_key).map_err(|_e| HpkeError::KeyNotFound)?,
            WELCOME_HPKE_LABEL,
            &[],
            &ciphertext,
            CIPHERSUITE,
            &RustCrypto::default(),
        )?),
        Err(_e) => Err(HpkeError::KeyNotFound),
    }
}
