pub mod basic_credential;
pub mod configuration;
pub mod ethereum;
pub mod hash;
pub mod rand;
pub mod signature;
pub mod utils;

pub use basic_credential::*;

pub type Secret = tls_codec::SecretVLBytes; // Byte array with ZeroizeOnDrop
