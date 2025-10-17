use super::WrapperAlgorithm;
use openmls::ciphersuite::hpke::Error as OpenmlsHpkeError;
use openmls::prelude::tls_codec::Error as TlsCodecError;
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::types::HpkeCiphertext;
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_common::RetryableError;
use xmtp_configuration::WELCOME_HPKE_LABEL;

static LIBCRUX_CRYPTO_PROVIDER: std::sync::LazyLock<openmls_libcrux_crypto::CryptoProvider> =
    std::sync::LazyLock::new(|| {
        openmls_libcrux_crypto::CryptoProvider::new().expect("Failed to create CryptoProvider")
    });

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
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
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
    welcome: &[u8],
    welcome_metadata: &[u8],
    hpke_public_key: &[u8],
    wrapper_algorithm: WrapperAlgorithm,
) -> Result<(Vec<u8>, Vec<u8>), WrapWelcomeError> {
    // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_seal(...)
    // but uses the context to encrypt multiple messages at once using the same context
    // because openmls only supports one shot messages.

    let context = openmls::prelude::hpke::EncryptContext::new(WELCOME_HPKE_LABEL, vec![].into());
    let info = context.tls_serialize_detached()?;
    let aad = &[];

    let map_hpke_error = |e| match e {
        hpke_rs::HpkeError::InvalidConfig => openmls::prelude::CryptoError::SenderSetupError,
        _ => openmls::prelude::CryptoError::HpkeEncryptionError,
    };

    let pk_r = hpke_rs::HpkePublicKey::new(hpke_public_key.to_vec());
    let mut config = wrapper_algorithm.to_hpke_config();

    let (enc, mut ctxt) = config
        .setup_sender(&pk_r, &info, None, None, None)
        .map_err(map_hpke_error)?;

    let encrypted_welcome = ctxt
        .seal(aad, welcome)
        .map(|ct| HpkeCiphertext {
            kem_output: enc.into(),
            ciphertext: ct.into(),
        })
        .map_err(map_hpke_error)?;
    let encrypted_welcome_metadata = ctxt.seal(aad, welcome_metadata).map_err(map_hpke_error)?;

    Ok((
        encrypted_welcome.tls_serialize_detached()?,
        encrypted_welcome_metadata,
    ))
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

    // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_open(...)
    // but uses the context to decrypt multiple messages at once using the same context
    // because openmls only supports one shot messages.

    let context = openmls::prelude::hpke::EncryptContext::new(WELCOME_HPKE_LABEL, vec![].into());
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

pub fn wrap_welcome_symmetric(
    data: &[u8],
    aead_type: openmls::prelude::AeadType,
    symmetric_key: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, WrapWelcomeError> {
    (*LIBCRUX_CRYPTO_PROVIDER)
        .aead_encrypt(aead_type, symmetric_key, data, nonce, &[])
        .map_err(Into::into)
}

pub fn unwrap_welcome_symmetric(
    data: &[u8],
    aead_type: openmls::prelude::AeadType,
    symmetric_key: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, UnwrapWelcomeError> {
    (*LIBCRUX_CRYPTO_PROVIDER)
        .aead_decrypt(aead_type, symmetric_key, data, nonce, &[])
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::{MlsProviderExt, XmtpMlsStorageProvider};

    use crate::{
        builder::ClientBuilder,
        groups::mls_ext::{find_key_package_hash_ref, find_private_key},
        identity::NewKeyPackageResult,
    };

    use super::*;

    fn find_key_package_private_key(
        provider: &impl XmtpMlsStorageProvider,
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

        let private_key = find_key_package_private_key(
            provider.key_store(),
            hpke_public_key,
            WrapperAlgorithm::Curve25519,
        );

        let to_encrypt = xmtp_common::rand_vec::<1000>();
        let to_encrypt_metadata = xmtp_common::rand_vec::<32>();

        // Encryption doesn't require any details about the sender, so we can test using one client
        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            to_encrypt_metadata.as_slice(),
            hpke_public_key,
            WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        assert_ne!(&to_encrypt, &wrapped.0);
        assert_ne!(&to_encrypt_metadata, &wrapped.1);

        let unwrapped = unwrap_welcome(
            &wrapped.0,
            &wrapped.1,
            &private_key,
            WrapperAlgorithm::Curve25519,
        )
        .unwrap();

        assert_eq!(unwrapped, (to_encrypt, to_encrypt_metadata));
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
            provider.key_store(),
            &pq_pub_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        );
        let to_encrypt = xmtp_common::rand_vec::<1000>();
        let to_encrypt_metadata = xmtp_common::rand_vec::<32>();

        // Test error handling
        wrap_welcome(
            to_encrypt.as_slice(),
            to_encrypt_metadata.as_slice(),
            &pq_pub_key[..pq_pub_key.len().saturating_sub(1)],
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap_err();

        let wrapped = wrap_welcome(
            to_encrypt.as_slice(),
            to_encrypt_metadata.as_slice(),
            &pq_pub_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        assert_ne!(&to_encrypt, &wrapped.0);
        assert_ne!(&to_encrypt_metadata, &wrapped.1);

        let unwrapped = unwrap_welcome(
            &wrapped.0,
            &wrapped.1,
            &private_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        assert_eq!(unwrapped, (to_encrypt.clone(), to_encrypt_metadata));

        let unwrapped = unwrap_welcome(
            &wrapped.0,
            &[],
            &private_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap();

        assert_eq!(unwrapped, (to_encrypt, vec![]));

        unwrap_welcome(
            &unwrapped.0,
            &unwrapped.1,
            &private_key[..private_key.len().saturating_sub(1)],
            WrapperAlgorithm::XWingMLKEM768Draft6,
        )
        .unwrap_err();
    }

    fn wrap_welcome_inner(
        crypto_provider: &impl openmls_traits::crypto::OpenMlsCrypto,
        unwrapped_welcome: &[u8],
        hpke_public_key: &[u8],
        ciphersuite: openmls::prelude::Ciphersuite,
    ) -> Result<Vec<u8>, WrapWelcomeError> {
        Ok(openmls::prelude::hpke::encrypt_with_label(
            hpke_public_key,
            WELCOME_HPKE_LABEL,
            &[],
            unwrapped_welcome,
            ciphersuite,
            crypto_provider,
        )?
        .tls_serialize_detached()?)
    }

    fn unwrap_welcome_inner(
        crypto_provider: &impl openmls_traits::crypto::OpenMlsCrypto,
        ciphertext: &HpkeCiphertext,
        private_key: &[u8],
        wrapper_ciphersuite: openmls::prelude::Ciphersuite,
    ) -> Result<Vec<u8>, UnwrapWelcomeError> {
        Ok(openmls::prelude::hpke::decrypt_with_label(
            private_key,
            WELCOME_HPKE_LABEL,
            &[],
            ciphertext,
            wrapper_ciphersuite,
            crypto_provider,
        )?)
    }
    #[xmtp_common::test]
    async fn round_trip_xwing_mlkem512_current_to_previous_and_back() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.context.mls_provider();

        let NewKeyPackageResult {
            pq_pub_key: maybe_pq_pub_key,
            ..
        } = client.identity().new_key_package(&provider, true).unwrap();
        let pq_pub_key = maybe_pq_pub_key.unwrap();
        let private_key = find_key_package_private_key(
            provider.key_store(),
            &pq_pub_key,
            WrapperAlgorithm::XWingMLKEM768Draft6,
        );

        let to_encrypt = xmtp_common::rand_vec::<1000>();
        let to_encrypt_metadata = xmtp_common::rand_vec::<32>();

        // Test the current code to previous code round trip
        {
            let wrapped = wrap_welcome(
                &to_encrypt,
                &to_encrypt_metadata,
                &pq_pub_key,
                WrapperAlgorithm::XWingMLKEM768Draft6,
            )
            .unwrap();

            assert_ne!(to_encrypt_metadata, wrapped.1);

            let unwrapped = unwrap_welcome_inner(
                &openmls_libcrux_crypto::CryptoProvider::new().unwrap(),
                &HpkeCiphertext::tls_deserialize_exact(&wrapped.0).unwrap(),
                &private_key,
                WrapperAlgorithm::XWingMLKEM768Draft6.to_mls_ciphersuite(),
            )
            .unwrap();

            assert_eq!(unwrapped, to_encrypt);
        }

        // Test the previous code to current code round trip
        {
            let wrapped = wrap_welcome_inner(
                &openmls_libcrux_crypto::CryptoProvider::new().unwrap(),
                &to_encrypt,
                &pq_pub_key,
                WrapperAlgorithm::XWingMLKEM768Draft6.to_mls_ciphersuite(),
            )
            .unwrap();

            let unwrapped = unwrap_welcome(
                &wrapped,
                &[],
                &private_key,
                WrapperAlgorithm::XWingMLKEM768Draft6,
            )
            .unwrap();

            assert_eq!(unwrapped, (to_encrypt, vec![]));
        }
    }

    #[xmtp_common::test]
    async fn round_trip_curve_25519_current_to_previous_and_back() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.context.mls_provider();

        let NewKeyPackageResult { key_package, .. } =
            client.identity().new_key_package(&provider, false).unwrap();

        let hpke_public_key = key_package.hpke_init_key().as_slice();

        let private_key = find_key_package_private_key(
            provider.key_store(),
            hpke_public_key,
            WrapperAlgorithm::Curve25519,
        );
        let to_encrypt = xmtp_common::rand_vec::<1000>();
        let to_encrypt_metadata = xmtp_common::rand_vec::<32>();

        // Test the current code to previous code round trip
        {
            let wrapped = wrap_welcome(
                &to_encrypt,
                &to_encrypt_metadata,
                hpke_public_key,
                WrapperAlgorithm::Curve25519,
            )
            .unwrap();

            assert_ne!(to_encrypt_metadata, wrapped.1);

            let unwrapped = unwrap_welcome_inner(
                // Use old crypto provider to match previous code
                &openmls_rust_crypto::RustCrypto::default(),
                &HpkeCiphertext::tls_deserialize_exact(&wrapped.0).unwrap(),
                &private_key,
                WrapperAlgorithm::Curve25519.to_mls_ciphersuite(),
            )
            .unwrap();

            assert_eq!(unwrapped, to_encrypt);
        }

        // Test the previous code to current code round trip
        {
            let wrapped = wrap_welcome_inner(
                // Use old crypto provider to match previous code
                &openmls_rust_crypto::RustCrypto::default(),
                &to_encrypt,
                hpke_public_key,
                WrapperAlgorithm::Curve25519.to_mls_ciphersuite(),
            )
            .unwrap();

            let unwrapped =
                unwrap_welcome(&wrapped, &[], &private_key, WrapperAlgorithm::Curve25519).unwrap();

            assert_eq!(unwrapped, (to_encrypt, vec![]));
        }
    }
}
