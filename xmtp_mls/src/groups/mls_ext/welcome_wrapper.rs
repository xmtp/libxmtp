use crate::configuration::POST_QUANTUM_CIPHERSUITE;
use crate::configuration::{CIPHERSUITE, WELCOME_HPKE_LABEL};
use openmls::ciphersuite::hpke::{encrypt_with_label, Error as OpenmlsHpkeError};
use openmls::prelude::hpke::decrypt_with_label;
use openmls::prelude::{tls_codec::Error as TlsCodecError, Ciphersuite};
use openmls_libcrux_crypto::CryptoProvider as LibcruxCryptoProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{crypto::OpenMlsCrypto, types::HpkeCiphertext};
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_common::RetryableError;
pub enum WrapperCiphersuite {
    Curve25519,
    #[allow(dead_code)]
    XWingMLKEM512,
}

impl WrapperCiphersuite {
    pub fn to_mls_ciphersuite(&self) -> Ciphersuite {
        match self {
            WrapperCiphersuite::Curve25519 => CIPHERSUITE,
            WrapperCiphersuite::XWingMLKEM512 => POST_QUANTUM_CIPHERSUITE,
        }
    }
}

#[derive(Debug, Error)]
pub enum WrapWelcomeError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
}

#[derive(Debug, Error)]
pub enum UnwrapWelcomeError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
}

impl RetryableError for WrapWelcomeError {
    fn is_retryable(&self) -> bool {
        false
    }
}

pub fn wrap_welcome(
    unwrapped_welcome: &[u8],
    hpke_public_key: &[u8],
    wrapper_ciphersuite: WrapperCiphersuite,
) -> Result<Vec<u8>, WrapWelcomeError> {
    match wrapper_ciphersuite {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        WrapperCiphersuite::Curve25519 => wrap_welcome_inner(
            &RustCrypto::default(),
            unwrapped_welcome,
            hpke_public_key,
            wrapper_ciphersuite.to_mls_ciphersuite(),
        ),
        WrapperCiphersuite::XWingMLKEM512 => wrap_welcome_inner(
            &LibcruxCryptoProvider::default(),
            unwrapped_welcome,
            hpke_public_key,
            wrapper_ciphersuite.to_mls_ciphersuite(),
        ),
    }
}

fn wrap_welcome_inner(
    crypto_provider: &impl OpenMlsCrypto,
    unwrapped_welcome: &[u8],
    hpke_public_key: &[u8],
    ciphersuite: Ciphersuite,
) -> Result<Vec<u8>, WrapWelcomeError> {
    Ok(encrypt_with_label(
        hpke_public_key,
        WELCOME_HPKE_LABEL,
        &[],
        unwrapped_welcome,
        ciphersuite,
        crypto_provider,
    )?
    .tls_serialize_detached()?)
}

pub fn unwrap_welcome(
    wrapped_welcome: &[u8],
    private_key: &[u8],
    wrapper_ciphersuite: WrapperCiphersuite,
) -> Result<Vec<u8>, UnwrapWelcomeError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(wrapped_welcome)?;

    match wrapper_ciphersuite {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        WrapperCiphersuite::Curve25519 => unwrap_welcome_inner(
            &RustCrypto::default(),
            &ciphertext,
            private_key,
            wrapper_ciphersuite.to_mls_ciphersuite(),
        ),
        WrapperCiphersuite::XWingMLKEM512 => unwrap_welcome_inner(
            &LibcruxCryptoProvider::default(),
            &ciphertext,
            private_key,
            wrapper_ciphersuite.to_mls_ciphersuite(),
        ),
    }
}

fn unwrap_welcome_inner(
    crypto_provider: &impl OpenMlsCrypto,
    ciphertext: &HpkeCiphertext,
    private_key: &[u8],
    wrapper_ciphersuite: Ciphersuite,
) -> Result<Vec<u8>, UnwrapWelcomeError> {
    Ok(decrypt_with_label(
        private_key,
        WELCOME_HPKE_LABEL,
        &[],
        &ciphertext,
        wrapper_ciphersuite,
        crypto_provider,
    )?)
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::builder::ClientBuilder;

    use super::*;

    #[xmtp_common::test]
    async fn round_trip_curve25519() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider().unwrap();

        let kp = client.identity().new_key_package(&provider).unwrap();
        let hpke_public_key = kp.hpke_init_key().as_slice();
        let to_encrypt = vec![1, 2, 3];

        // Encryption doesn't require any details about the sender, so we can test using one client
        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            hpke_public_key,
            WrapperCiphersuite::Curve25519,
        )
        .unwrap();

        let unwrapped =
            unwrap_welcome(hpke_public_key, &wrapped, WrapperCiphersuite::Curve25519).unwrap();

        assert_eq!(unwrapped, to_encrypt);
    }

    #[xmtp_common::test]
    async fn round_trip_xwing_mlkem512() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider().unwrap();

        let kp = client.identity().new_key_package(&provider).unwrap();
        let hpke_public_key = kp.hpke_init_key().as_slice();
        let to_encrypt = vec![1, 2, 3];

        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            hpke_public_key,
            WrapperCiphersuite::XWingMLKEM512,
        )
        .unwrap();

        let unwrapped =
            unwrap_welcome(hpke_public_key, &wrapped, WrapperCiphersuite::XWingMLKEM512).unwrap();

        assert_eq!(unwrapped, to_encrypt);
    }
}
