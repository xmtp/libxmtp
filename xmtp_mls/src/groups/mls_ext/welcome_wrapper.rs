use super::WrapperAlgorithm;
use crate::configuration::WELCOME_HPKE_LABEL;
use openmls::ciphersuite::hpke::{encrypt_with_label, Error as OpenmlsHpkeError};
use openmls::prelude::hpke::decrypt_with_label;
use openmls::prelude::{tls_codec::Error as TlsCodecError, Ciphersuite};
use openmls_libcrux_crypto::CryptoProvider as LibcruxCryptoProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{crypto::OpenMlsCrypto, types::HpkeCiphertext};
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_common::RetryableError;

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

impl RetryableError for UnwrapWelcomeError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// Wrap a message in an outer layer of encryption using
/// the specified [WrapperAlgorithm].
/// The algorithm and public key type MUST match
pub fn wrap_welcome(
    unwrapped_welcome: &[u8],
    hpke_public_key: &[u8],
    wrapper_algorithm: &WrapperAlgorithm,
) -> Result<Vec<u8>, WrapWelcomeError> {
    match wrapper_algorithm {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        WrapperAlgorithm::Curve25519 => wrap_welcome_inner(
            &RustCrypto::default(),
            unwrapped_welcome,
            hpke_public_key,
            wrapper_algorithm.to_mls_ciphersuite(),
        ),
        WrapperAlgorithm::XWingMLKEM768Draft6 => wrap_welcome_inner(
            &LibcruxCryptoProvider::new().expect(
                "Failed to create LibcruxCryptoProvider because of insufficient randomness",
            ),
            unwrapped_welcome,
            hpke_public_key,
            wrapper_algorithm.to_mls_ciphersuite(),
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

/// Unwrap a message that was wrapped using the specified [WrapperAlgorithm].
/// The algorithm and private key type MUST match.
pub fn unwrap_welcome(
    wrapped_welcome: &[u8],
    private_key: &[u8],
    wrapper_algorithm: WrapperAlgorithm,
) -> Result<Vec<u8>, UnwrapWelcomeError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(wrapped_welcome)?;

    match wrapper_algorithm {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        WrapperAlgorithm::Curve25519 => unwrap_welcome_inner(
            &RustCrypto::default(),
            &ciphertext,
            private_key,
            wrapper_algorithm.to_mls_ciphersuite(),
        ),
        WrapperAlgorithm::XWingMLKEM768Draft6 => unwrap_welcome_inner(
            &LibcruxCryptoProvider::new().expect(
                "Failed to create LibcruxCryptoProvider because of insufficient randomness",
            ),
            &ciphertext,
            private_key,
            wrapper_algorithm.to_mls_ciphersuite(),
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
        ciphertext,
        wrapper_ciphersuite,
        crypto_provider,
    )?)
}

#[cfg(test)]
mod tests {
    use openmls::storage::StorageProvider;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::XmtpOpenMlsProvider;

    use crate::{
        builder::ClientBuilder,
        groups::mls_ext::{find_key_package_hash_ref, find_private_key},
        identity::NewKeyPackageResult,
    };

    use super::*;

    fn find_key_package_private_key<S: StorageProvider>(
        provider: &XmtpOpenMlsProvider<S>,
        hpke_public_key: &[u8],
        wrapper_algorithm: WrapperAlgorithm,
    ) -> Vec<u8> {
        let hash_ref = find_key_package_hash_ref(provider, hpke_public_key).unwrap();
        find_private_key(provider, &hash_ref, &wrapper_algorithm).unwrap()
    }

    #[xmtp_common::test]
    async fn round_trip_curve25519() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider();

        let NewKeyPackageResult { key_package, .. } =
            client.identity().new_key_package(&provider, false).unwrap();

        let hpke_public_key = key_package.hpke_init_key().as_slice();

        let private_key =
            find_key_package_private_key(&provider, hpke_public_key, WrapperAlgorithm::Curve25519);

        let to_encrypt = vec![1, 2, 3];

        // Encryption doesn't require any details about the sender, so we can test using one client
        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            hpke_public_key,
            &WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        let unwrapped =
            unwrap_welcome(&wrapped, &private_key, WrapperAlgorithm::Curve25519).unwrap();

        assert_eq!(unwrapped, to_encrypt);
    }

    #[xmtp_common::test]
    async fn round_trip_xwing_mlkem512() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider();

        let NewKeyPackageResult {
            pq_pub_key: maybe_pq_pub_key,
            ..
        } = client.identity().new_key_package(&provider, true).unwrap();
        let pq_pub_key = maybe_pq_pub_key.unwrap();
        let private_key = find_key_package_private_key(
            &provider,
            &pq_pub_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        );
        let to_encrypt = vec![1, 2, 3];

        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            &pq_pub_key,
            &WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        let unwrapped = unwrap_welcome(
            &wrapped,
            &private_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        assert_eq!(unwrapped, to_encrypt);
    }
}
