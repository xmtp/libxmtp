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

#[derive(Debug)]
pub enum PPPPError {
    GenericError(String),
}

impl std::error::Error for PPPPError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            PPPPError::GenericError(_) => None,
        }
    }
}

impl std::fmt::Display for PPPPError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            PPPPError::GenericError(ref message) => write!(f, "{}", message),
        }
    }
}

#[derive(Debug)]
pub enum VerifyError {
    GenericError(String),
}

impl std::error::Error for VerifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            VerifyError::GenericError(_) => None,
        }
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            VerifyError::GenericError(ref message) => write!(f, "{}", message),
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

pub fn verify_k256_sha256(
    signed_by: Vec<u8>,
    message: Vec<u8>,
    signature: Vec<u8>,
    recovery_id: u8,
) -> Result<bool, VerifyError> {
    let result = k256_helper::verify_sha256(
        signed_by.as_slice(),
        message.as_slice(),
        signature.as_slice(),
        recovery_id,
    )
    .map_err(VerifyError::GenericError)?;

    Ok(result)
}

pub fn pppp_encrypt(
    public_key_bytes: Vec<u8>,
    private_key_bytes: Vec<u8>,
    message_bytes: Vec<u8>,
) -> Result<Vec<u8>, EciesError> {
    let ciphertext = xmtp_pppp::encrypt_message(
        public_key_bytes.as_slice(),
        private_key_bytes.as_slice(),
        message_bytes.as_slice(),
    )
    .map_err(|e| EciesError::GenericError(e))?;

    Ok(ciphertext)
}

pub fn pppp_decrypt(
    public_key_bytes: Vec<u8>,
    private_key_bytes: Vec<u8>,
    message_bytes: Vec<u8>,
) -> Result<Vec<u8>, EciesError> {
    let ciphertext = xmtp_pppp::decrypt_message(
        public_key_bytes.as_slice(),
        private_key_bytes.as_slice(),
        message_bytes.as_slice(),
    )
    .map_err(|e| EciesError::GenericError(e))?;

    Ok(ciphertext)
}

pub fn generate_private_preferences_topic_identifier(
    private_key_bytes: Vec<u8>,
) -> Result<String, EciesError> {
    xmtp_pppp::topic::generate_private_preferences_topic_identifier(private_key_bytes.as_slice())
        .map_err(|e| EciesError::GenericError(e))
}
