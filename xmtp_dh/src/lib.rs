uniffi_macros::include_scaffolding!("xmtp_dh");

use xmtp_crypto::k256_helper;

// Uniffi requires enum errors that implement std::Error. We implement it
// manually here rather than pulling in thiserror to save binary size and compilation time.
#[derive(Debug)]
pub enum DiffieHellmanError {
    GenericError(String),
}

impl std::error::Error for DiffieHellmanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            DiffieHellmanError::GenericError(_) => None,
        }
    }
}

impl std::fmt::Display for DiffieHellmanError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            DiffieHellmanError::GenericError(ref message) => write!(f, "{}", message),
        }
    }
}

pub fn diffie_hellman_k256(
    private_key_bytes: Vec<u8>,
    public_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, DiffieHellmanError> {
    let shared_secret = k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .map_err(DiffieHellmanError::GenericError)?;
    Ok(shared_secret)
}
