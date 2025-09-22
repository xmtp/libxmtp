use thiserror::Error;
use xmtp_cryptography::ethereum;

#[derive(Debug, Error, uniffi::Error)]
pub enum FfiCryptoError {
    #[error("invalid length")]
    InvalidLength,
    #[error("invalid key")]
    InvalidKey,
    #[error("signing failure")]
    SignFailure,
    #[error("pubkey decompress failure")]
    DecompressFailure,
}

impl From<ethereum::EthereumCryptoError> for FfiCryptoError {
    fn from(err: ethereum::EthereumCryptoError) -> Self {
        match err {
            ethereum::EthereumCryptoError::InvalidLength => FfiCryptoError::InvalidLength,
            ethereum::EthereumCryptoError::InvalidKey => FfiCryptoError::InvalidKey,
            ethereum::EthereumCryptoError::SignFailure => FfiCryptoError::SignFailure,
            ethereum::EthereumCryptoError::DecompressFailure => FfiCryptoError::DecompressFailure,
        }
    }
}

/// 1) Ethereum compatible public key from 32-byte private key.
///    Returns **65-byte uncompressed** (0x04 || X || Y)
///    Private key is automatically zeroized after use for security
#[uniffi::export]
fn ethereum_generate_public_key(private_key32: Vec<u8>) -> Result<Vec<u8>, FfiCryptoError> {
    let zeroizing_key = ethereum::zeroizing_private_key(&private_key32)?;
    let public_key = ethereum::public_key_uncompressed(zeroizing_key)?;
    Ok(public_key.to_vec())
}

/// 2) Ethereum recoverable signature (FFI).
///    Returns 65 bytes `r || s || v`, with **v ∈ {27,28}**
///    (legacy/Electrum encoding where **v = 27 + parity**, parity ∈ {0,1}).
///    - If `hashing == true`: signs per **EIP-191**
///      ("Ethereum Signed Message:\n{len(msg)}" || msg, then keccak256).
///    - If `hashing == false`: `msg` must be a **32-byte** prehash (e.g., keccak256/EIP-712 digest).
///    - Private key is automatically zeroized after signing for security
#[uniffi::export]
fn ethereum_sign_recoverable(
    msg: Vec<u8>,
    private_key32: Vec<u8>,
    hashing: bool,
) -> Result<Vec<u8>, FfiCryptoError> {
    // Create a zeroizing private key
    let zeroizing_key = ethereum::zeroizing_private_key(&private_key32)?;
    let signature = ethereum::sign_recoverable(&msg, zeroizing_key, hashing)?;
    Ok(signature.to_vec())
}

/// 3) Ethereum address from public key (accepts 65-byte 0x04||XY or 64-byte XY).
#[uniffi::export]
fn ethereum_address_from_pubkey(pubkey: Vec<u8>) -> Result<String, FfiCryptoError> {
    let address = ethereum::address_from_pubkey(&pubkey)?;
    Ok(address)
}

/// 4) EIP-191 personal message hash: keccak256("\x19Ethereum Signed Message:\n{len}" || message)
#[uniffi::export]
fn ethereum_hash_personal(message: String) -> Result<Vec<u8>, FfiCryptoError> {
    Ok(ethereum::hash_personal(&message).to_vec()) // 32 bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_error_mapping_invalid_private_key_length() {
        // Test that invalid private key lengths are properly mapped to FfiCryptoError::InvalidLength

        // Too short (31 bytes instead of 32)
        let short_key = vec![1u8; 31];
        let result = ethereum_generate_public_key(short_key);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));

        // Too long (33 bytes instead of 32)
        let long_key = vec![1u8; 33];
        let result = ethereum_generate_public_key(long_key);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));

        // Same for signing function
        let message = b"test message";
        let result = ethereum_sign_recoverable(message.to_vec(), vec![1u8; 31], true);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));
    }

    #[test]
    fn test_ffi_error_mapping_invalid_private_key() {
        // Test that invalid private keys (like all zeros) are properly mapped to FfiCryptoError::InvalidKey

        let zero_key = vec![0u8; 32]; // All zeros - mathematically invalid
        let result = ethereum_generate_public_key(zero_key.clone());
        assert!(matches!(result, Err(FfiCryptoError::InvalidKey)));

        // Same for signing function
        let message = b"test message";
        let result = ethereum_sign_recoverable(message.to_vec(), zero_key, true);
        assert!(matches!(result, Err(FfiCryptoError::InvalidKey)));
    }

    #[test]
    fn test_ffi_error_mapping_invalid_pubkey_length() {
        // Test that invalid public key lengths are properly mapped to FfiCryptoError::InvalidLength

        // Too short
        let short_pubkey = vec![0x04; 32]; // Should be 64 or 65 bytes
        let result = ethereum_address_from_pubkey(short_pubkey);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));

        // Too long
        let long_pubkey = vec![0x04; 100];
        let result = ethereum_address_from_pubkey(long_pubkey);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));

        // Wrong prefix for 65-byte key
        let mut wrong_prefix = vec![0u8; 65];
        wrong_prefix[0] = 0x03; // Should be 0x04 for uncompressed
        let result = ethereum_address_from_pubkey(wrong_prefix);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));
    }

    #[test]
    fn test_ffi_error_mapping_invalid_hash_length() {
        // Test that invalid hash lengths for pre-hashed signing are properly mapped

        let valid_key = vec![1u8; 32];

        // Too short hash (31 bytes instead of 32)
        let short_hash = vec![0u8; 31];
        let result = ethereum_sign_recoverable(short_hash, valid_key.clone(), false);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));

        // Too long hash (33 bytes instead of 32)
        let long_hash = vec![0u8; 33];
        let result = ethereum_sign_recoverable(long_hash, valid_key, false);
        assert!(matches!(result, Err(FfiCryptoError::InvalidLength)));
    }

    #[test]
    fn test_ffi_basic_functionality() {
        // Basic smoke test to ensure FFI functions work with valid inputs
        let private_key =
            hex::decode("90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb")
                .expect("Valid hex private key");

        // Test public key generation
        let public_key =
            ethereum_generate_public_key(private_key.clone()).expect("Should generate public key");
        assert_eq!(public_key.len(), 65);
        assert_eq!(public_key[0], 0x04);

        // Test address generation
        let address = ethereum_address_from_pubkey(public_key).expect("Should generate address");
        assert!(address.starts_with("0x"));
        assert_eq!(address.len(), 42);

        // Test signing
        let message = b"Hello, FFI!";
        let signature = ethereum_sign_recoverable(message.to_vec(), private_key, true)
            .expect("Should sign message");
        assert_eq!(signature.len(), 65);
        assert!(signature[64] == 27 || signature[64] == 28);

        // Test hashing
        let hash = ethereum_hash_personal("test message".to_string()).expect("Should hash message");
        assert_eq!(hash.len(), 32);
    }
}
