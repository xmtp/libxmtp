use crate::configuration::POST_QUANTUM_CIPHERSUITE;
use openmls::prelude::HpkeKeyPair;
use openmls_libcrux_crypto::Provider as LibcruxProvider;
use openmls_traits::{OpenMlsProvider, crypto::OpenMlsCrypto, random::OpenMlsRand};
use thiserror::Error;
use tls_codec::SecretVLBytes;

/// Error type for generating a post quantum key pair
#[derive(Debug, Error)]
pub enum GeneratePostQuantumKeyError {
    #[error(transparent)]
    Crypto(#[from] openmls_traits::types::CryptoError),
    #[error(transparent)]
    Rand(#[from] openmls_libcrux_crypto::RandError),
}

/// Generate a new key pair using our post quantum ciphersuite
pub fn generate_post_quantum_key() -> Result<HpkeKeyPair, GeneratePostQuantumKeyError> {
    let provider = LibcruxProvider::default();
    let rand = provider.rand();

    let ikm: SecretVLBytes = rand
        .random_vec(POST_QUANTUM_CIPHERSUITE.hash_length())?
        .into();

    Ok(provider
        .crypto()
        .derive_hpke_keypair(POST_QUANTUM_CIPHERSUITE.hpke_config(), ikm.as_slice())?)
}
