use openmls::ciphersuite::hpke::Error as OpenmlsHpkeError;
use openmls::prelude::tls_codec::Error as TlsCodecError;
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::types::HpkeCiphertext;
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_common::Retryable;
use xmtp_id::key_package::WrapperAlgorithm;

static LIBCRUX_CRYPTO_PROVIDER: std::sync::LazyLock<openmls_libcrux_crypto::CryptoProvider> =
    std::sync::LazyLock::new(|| {
        openmls_libcrux_crypto::CryptoProvider::new().expect("Failed to create CryptoProvider")
    });

#[derive(Debug, Error, Retryable)]
pub enum WrapPayloadError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
}

#[derive(Debug, Error, Retryable)]
pub enum UnwrapPayloadError {
    #[error("OpenMLS HPKE error: {0}")]
    Hpke(#[from] OpenmlsHpkeError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
}

/// Wrap a payload (plus optional secondary payload) in an outer layer of HPKE
/// encryption using the specified [WrapperAlgorithm]. The algorithm and public
/// key type MUST match.
///
/// `label` is fed to the HPKE `EncryptContext` as the domain-separation label.
/// Use [`xmtp_configuration::WELCOME_HPKE_LABEL`] for welcome-flow compatibility.
///
/// For the `XWingMLKEM768Draft6` algorithm, `payload` and `secondary_payload`
/// are wrapped using the same HPKE public key. The first returned vec is the
/// `HpkeCiphertext` with TLS serialization. The second vec is just ciphertext.
pub fn wrap_payload_hpke(
    payload: &[u8],
    secondary_payload: &[u8],
    hpke_public_key: &[u8],
    wrapper_algorithm: WrapperAlgorithm,
    label: &str,
) -> Result<(Vec<u8>, Vec<u8>), WrapPayloadError> {
    // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_seal(...)
    // but uses the context to encrypt multiple messages at once using the same context
    // because openmls only supports one shot messages.

    let context = openmls::prelude::hpke::EncryptContext::from((label, [].as_slice()));
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

    let encrypted_payload = ctxt
        .seal(aad, payload)
        .map(|ct| HpkeCiphertext {
            kem_output: enc.into(),
            ciphertext: ct.into(),
        })
        .map_err(map_hpke_error)?;
    let encrypted_secondary_payload = ctxt.seal(aad, secondary_payload).map_err(map_hpke_error)?;

    Ok((
        encrypted_payload.tls_serialize_detached()?,
        encrypted_secondary_payload,
    ))
}

/// Unwrap a payload that was wrapped using the specified [WrapperAlgorithm].
/// The algorithm and private key type MUST match. `label` MUST match the value
/// used at wrap time.
pub fn unwrap_payload_hpke(
    wrapped_payload: &[u8],
    wrapped_secondary_payload: &[u8],
    private_key: &[u8],
    wrapper_algorithm: WrapperAlgorithm,
    label: &str,
) -> Result<(Vec<u8>, Vec<u8>), UnwrapPayloadError> {
    let ciphertext = HpkeCiphertext::tls_deserialize_exact(wrapped_payload)?;

    // The following implementation is the same as calling openmls_libcrux_crypto::CryptoProvider::hpke_open(...)
    // but uses the context to decrypt multiple messages at once using the same context
    // because openmls only supports one shot messages.

    let context = openmls::prelude::hpke::EncryptContext::from((label, [].as_slice()));
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

    let payload = ctxt
        .open(aad, ciphertext.ciphertext.as_ref())
        .map_err(map_hpke_error)?;
    let secondary_payload = if wrapped_secondary_payload.is_empty() {
        vec![]
    } else {
        ctxt.open(aad, wrapped_secondary_payload)
            .map_err(map_hpke_error)?
    };

    Ok((payload, secondary_payload))
}

/// Wrap a payload with symmetric AEAD encryption (caller-supplied key + nonce).
///
/// Domain separation is handled by construction: callers MUST scope the
/// symmetric key to a single use-case.
pub fn wrap_payload_symmetric(
    data: &[u8],
    aead_type: openmls::prelude::AeadType,
    symmetric_key: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, WrapPayloadError> {
    (*LIBCRUX_CRYPTO_PROVIDER)
        .aead_encrypt(aead_type, symmetric_key, data, nonce, &[])
        .map_err(Into::into)
}

/// Unwrap a payload that was wrapped with [`wrap_payload_symmetric`].
pub fn unwrap_payload_symmetric(
    data: &[u8],
    aead_type: openmls::prelude::AeadType,
    symmetric_key: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, UnwrapPayloadError> {
    (*LIBCRUX_CRYPTO_PROVIDER)
        .aead_decrypt(aead_type, symmetric_key, data, nonce, &[])
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls_traits::{crypto::OpenMlsCrypto, random::OpenMlsRand};
    use xmtp_configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE, WELCOME_HPKE_LABEL};

    const TEST_LABEL: &str = "test xmtp payload";

    fn fresh_curve25519_keypair() -> (Vec<u8>, Vec<u8>) {
        let crypto = openmls_rust_crypto::RustCrypto::default();
        let ikm = crypto.random_vec(CIPHERSUITE.hash_length()).unwrap();
        let kp = crypto
            .derive_hpke_keypair(CIPHERSUITE.hpke_config(), &ikm)
            .unwrap();
        (kp.public, kp.private.to_vec())
    }

    fn fresh_xwing_keypair() -> (Vec<u8>, Vec<u8>) {
        let crypto = openmls_libcrux_crypto::CryptoProvider::new().unwrap();
        let ikm = crypto
            .random_vec(POST_QUANTUM_CIPHERSUITE.hash_length())
            .unwrap();
        let kp = crypto
            .derive_hpke_keypair(POST_QUANTUM_CIPHERSUITE.hpke_config(), &ikm)
            .unwrap();
        (kp.public, kp.private.to_vec())
    }

    #[xmtp_common::test]
    fn round_trip_curve25519_hpke() {
        let (pk, sk) = fresh_curve25519_keypair();

        let payload = xmtp_common::rand_vec::<1000>();
        let secondary = xmtp_common::rand_vec::<32>();

        let wrapped = wrap_payload_hpke(
            &payload,
            &secondary,
            &pk,
            WrapperAlgorithm::Curve25519,
            TEST_LABEL,
        )
        .unwrap();

        assert_ne!(payload, wrapped.0);
        assert_ne!(secondary, wrapped.1);

        let unwrapped = unwrap_payload_hpke(
            &wrapped.0,
            &wrapped.1,
            &sk,
            WrapperAlgorithm::Curve25519,
            TEST_LABEL,
        )
        .unwrap();

        assert_eq!(unwrapped, (payload, secondary));
    }

    #[xmtp_common::test]
    fn round_trip_xwing_hpke() {
        let (pk, sk) = fresh_xwing_keypair();

        let payload = xmtp_common::rand_vec::<1000>();
        let secondary = xmtp_common::rand_vec::<32>();

        let wrapped = wrap_payload_hpke(
            &payload,
            &secondary,
            &pk,
            WrapperAlgorithm::XWingMLKEM768Draft6,
            TEST_LABEL,
        )
        .unwrap();

        assert_ne!(payload, wrapped.0);
        assert_ne!(secondary, wrapped.1);

        let unwrapped = unwrap_payload_hpke(
            &wrapped.0,
            &wrapped.1,
            &sk,
            WrapperAlgorithm::XWingMLKEM768Draft6,
            TEST_LABEL,
        )
        .unwrap();

        assert_eq!(unwrapped, (payload.clone(), secondary));

        // Empty secondary payload short-circuits to vec![].
        let unwrapped = unwrap_payload_hpke(
            &wrapped.0,
            &[],
            &sk,
            WrapperAlgorithm::XWingMLKEM768Draft6,
            TEST_LABEL,
        )
        .unwrap();

        assert_eq!(unwrapped, (payload, vec![]));
    }

    #[xmtp_common::test]
    fn wrong_key_fails_curve25519() {
        let (pk, _sk) = fresh_curve25519_keypair();
        let (_pk2, sk2) = fresh_curve25519_keypair();

        let payload = xmtp_common::rand_vec::<128>();
        let secondary = xmtp_common::rand_vec::<32>();

        let wrapped = wrap_payload_hpke(
            &payload,
            &secondary,
            &pk,
            WrapperAlgorithm::Curve25519,
            TEST_LABEL,
        )
        .unwrap();

        unwrap_payload_hpke(
            &wrapped.0,
            &wrapped.1,
            &sk2,
            WrapperAlgorithm::Curve25519,
            TEST_LABEL,
        )
        .unwrap_err();
    }

    #[xmtp_common::test]
    fn wrong_label_fails_curve25519() {
        let (pk, sk) = fresh_curve25519_keypair();

        let payload = xmtp_common::rand_vec::<128>();
        let secondary = xmtp_common::rand_vec::<32>();

        let wrapped = wrap_payload_hpke(
            &payload,
            &secondary,
            &pk,
            WrapperAlgorithm::Curve25519,
            TEST_LABEL,
        )
        .unwrap();

        unwrap_payload_hpke(
            &wrapped.0,
            &wrapped.1,
            &sk,
            WrapperAlgorithm::Curve25519,
            "different label",
        )
        .unwrap_err();
    }

    #[xmtp_common::test]
    fn welcome_label_round_trip_matches_xmtp_configuration() {
        // Sanity-check that the welcome label still round-trips correctly through
        // the generalized API. This is the configuration the welcome flow uses.
        let (pk, sk) = fresh_curve25519_keypair();

        let payload = xmtp_common::rand_vec::<256>();
        let secondary = xmtp_common::rand_vec::<32>();

        let wrapped = wrap_payload_hpke(
            &payload,
            &secondary,
            &pk,
            WrapperAlgorithm::Curve25519,
            WELCOME_HPKE_LABEL,
        )
        .unwrap();

        let unwrapped = unwrap_payload_hpke(
            &wrapped.0,
            &wrapped.1,
            &sk,
            WrapperAlgorithm::Curve25519,
            WELCOME_HPKE_LABEL,
        )
        .unwrap();

        assert_eq!(unwrapped, (payload, secondary));
    }

    #[xmtp_common::test]
    fn round_trip_symmetric() {
        let symmetric_key = xmtp_common::rand_array::<32>();
        let nonce = xmtp_common::rand_array::<12>();
        let data = xmtp_common::rand_array::<1000>();

        let wrapped = wrap_payload_symmetric(
            &data,
            openmls::prelude::AeadType::ChaCha20Poly1305,
            &symmetric_key,
            &nonce,
        )
        .unwrap();
        let unwrapped = unwrap_payload_symmetric(
            &wrapped,
            openmls::prelude::AeadType::ChaCha20Poly1305,
            &symmetric_key,
            &nonce,
        )
        .unwrap();
        assert_eq!(data.as_slice(), unwrapped.as_slice());
    }

    #[xmtp_common::test]
    fn symmetric_wrong_key_fails() {
        let symmetric_key = xmtp_common::rand_array::<32>();
        let wrong_key = xmtp_common::rand_array::<32>();
        let nonce = xmtp_common::rand_array::<12>();
        let data = xmtp_common::rand_array::<1000>();

        let wrapped = wrap_payload_symmetric(
            &data,
            openmls::prelude::AeadType::ChaCha20Poly1305,
            &symmetric_key,
            &nonce,
        )
        .unwrap();
        unwrap_payload_symmetric(
            &wrapped,
            openmls::prelude::AeadType::ChaCha20Poly1305,
            &wrong_key,
            &nonce,
        )
        .unwrap_err();
    }
}
