pub mod basic_credential;
pub mod configuration;
pub mod ethereum;
pub mod hash;
pub mod rand;
pub mod signature;
pub mod utils;

pub use basic_credential::*;
pub use openmls;

pub type Secret = tls_codec::SecretVLBytes; // Byte array with ZeroizeOnDrop

// When upgrading to reqwest 0.13 and disabling the default aws-lc-rs crypto provider
// some tests fail without initializing the ring crypto provider. Doing this here because
// it is crypto related and all crates depend on this crate.
#[cfg(not(target_arch = "wasm32"))]
#[ctor::ctor]
fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
