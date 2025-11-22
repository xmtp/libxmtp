pub mod basic_credential;
pub mod configuration;
pub mod ethereum;
pub mod hash;
pub mod rand;
pub mod signature;
pub mod utils;

pub use basic_credential::*;

pub type Secret = tls_codec::SecretVLBytes; // Byte array with ZeroizeOnDrop

pub mod openmls {
    pub use openmls::*;
}

#[cfg(test)]
mod test {
    // common depends on cryptography
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
}
