use super::WrapperAlgorithm;
use openmls::ciphersuite::hpke::{Error as OpenmlsHpkeError, encrypt_with_label};
use openmls::prelude::hpke::decrypt_with_label;
use openmls::prelude::{Ciphersuite, tls_codec::Error as TlsCodecError};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{crypto::OpenMlsCrypto, types::HpkeCiphertext};
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_common::RetryableError;
use xmtp_configuration::WELCOME_HPKE_LABEL;

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
///
/// For the XWingMLKEM768Draft6 algorithm, the openmls_welcome and welcome_metadata are wrapped using the same HPKE public key
/// and the first vec returned is the HpkeCiphertext with tls serialization. The second vec is just ciphertext.
pub fn wrap_welcome(
    openmls_welcome: &[u8],
    welcome_metadata: &[u8],
    hpke_public_key: &[u8],
    wrapper_algorithm: &WrapperAlgorithm,
) -> Result<(Vec<u8>, Vec<u8>), WrapWelcomeError> {
    match wrapper_algorithm {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        // Any receiver using this doesn't care about welcome metadata
        WrapperAlgorithm::Curve25519 => Ok((
            wrap_welcome_inner(
                &RustCrypto::default(),
                openmls_welcome,
                hpke_public_key,
                wrapper_algorithm.to_mls_ciphersuite(),
            )?,
            // Ignore welcome metadata since the receiver is too old to support it
            vec![],
        )),
        WrapperAlgorithm::XWingMLKEM768Draft6 => {
            // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_seal(...)
            // but uses the context to encrypt multiple messages at once using the same context
            // because openmls only supports one shot messages.

            let context =
                openmls::prelude::hpke::EncryptContext::new(WELCOME_HPKE_LABEL, vec![].into());
            let info = context.tls_serialize_detached()?;
            let aad = &[];

            let mut config = wrapper_algorithm.to_hpke_config();

            let map_hpke_error = |e| match e {
                hpke_rs::HpkeError::InvalidConfig => {
                    openmls::prelude::CryptoError::SenderSetupError
                }
                _ => openmls::prelude::CryptoError::HpkeEncryptionError,
            };

            let pk_r = hpke_rs::HpkePublicKey::new(hpke_public_key.to_vec());

            let (enc, mut ctxt) = config
                .setup_sender(&pk_r, &info, None, None, None)
                .map_err(map_hpke_error)?;

            let encrypted_welcome = ctxt
                .seal(aad, openmls_welcome)
                .map(|ct| HpkeCiphertext {
                    kem_output: enc.into(),
                    ciphertext: ct.into(),
                })
                .map_err(map_hpke_error)?;
            let encrypted_welcome_metadata =
                ctxt.seal(aad, welcome_metadata).map_err(map_hpke_error)?;

            Ok((
                encrypted_welcome.tls_serialize_detached()?,
                encrypted_welcome_metadata,
            ))
        }
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
    wrapped_welcome_metadata: &[u8],
    private_key: &[u8],
    wrapper_algorithm: WrapperAlgorithm,
) -> Result<(Vec<u8>, Vec<u8>), UnwrapWelcomeError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(wrapped_welcome)?;

    match wrapper_algorithm {
        // I am taking the conservative approach here and not changing the crypto provider
        // for Curve25519 even if Libcrux supports it.

        // Once we move everything to Libcrux this can be removed.
        WrapperAlgorithm::Curve25519 => Ok((
            unwrap_welcome_inner(
                &RustCrypto::default(),
                &ciphertext,
                private_key,
                wrapper_algorithm.to_mls_ciphersuite(),
            )?,
            vec![],
        )),
        WrapperAlgorithm::XWingMLKEM768Draft6 => {
            // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_seal(...)
            // but uses the context to encrypt multiple messages at once using the same context
            // because openmls only supports one shot messages.

            let context =
                openmls::prelude::hpke::EncryptContext::new(WELCOME_HPKE_LABEL, vec![].into());
            let info = context.tls_serialize_detached()?;
            let aad = &[];

            let config = wrapper_algorithm.to_hpke_config();

            let sk_r = hpke_rs::HpkePrivateKey::new(private_key.to_vec());

            let map_hpke_error = |_| openmls::ciphersuite::hpke::Error::DecryptionFailed;

            let mut ctxt = config
                .setup_receiver(
                    ciphertext.kem_output.as_ref(),
                    &sk_r,
                    &info,
                    None,
                    None,
                    None,
                )
                .map_err(map_hpke_error)?;

            let welcome = ctxt
                .open(aad, ciphertext.ciphertext.as_ref())
                .map_err(map_hpke_error)?;
            let welcome_metadata = if wrapped_welcome_metadata.is_empty() {
                vec![]
            } else {
                ctxt.open(aad, wrapped_welcome_metadata)
                    .map_err(map_hpke_error)?
            };

            Ok((welcome, welcome_metadata))
        }
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
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::MlsProviderExt;

    use crate::{
        builder::ClientBuilder,
        groups::mls_ext::{find_key_package_hash_ref, find_private_key},
        identity::NewKeyPackageResult,
    };

    use super::*;

    fn find_key_package_private_key(
        provider: &impl MlsProviderExt,
        hpke_public_key: &[u8],
        wrapper_algorithm: WrapperAlgorithm,
    ) -> Vec<u8> {
        let hash_ref = find_key_package_hash_ref(provider, hpke_public_key).unwrap();
        find_private_key(provider, &hash_ref, &wrapper_algorithm).unwrap()
    }

    #[xmtp_common::test]
    async fn round_trip_curve25519() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.context.mls_provider();

        let NewKeyPackageResult { key_package, .. } =
            client.identity().new_key_package(&provider, false).unwrap();

        let hpke_public_key = key_package.hpke_init_key().as_slice();

        let private_key =
            find_key_package_private_key(&provider, hpke_public_key, WrapperAlgorithm::Curve25519);

        let to_encrypt = vec![1, 2, 3];
        let to_encrypt_metadata = vec![4, 5, 6];

        // Encryption doesn't require any details about the sender, so we can test using one client
        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            to_encrypt_metadata.as_slice(),
            hpke_public_key,
            &WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        let unwrapped = unwrap_welcome(
            &wrapped.0,
            &wrapped.1,
            &private_key,
            WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        assert_eq!(unwrapped, (to_encrypt, vec![]));
    }

    #[xmtp_common::test]
    async fn round_trip_xwing_mlkem512() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.context.mls_provider();

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
        let to_encrypt_metadata = vec![4, 5, 6];

        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            to_encrypt_metadata.as_slice(),
            &pq_pub_key,
            &WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        let unwrapped = unwrap_welcome(
            &wrapped.0,
            &wrapped.1,
            &private_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        assert_eq!(unwrapped, (to_encrypt, to_encrypt_metadata));
    }
}
