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

/// 1) Public key from 32-byte private key.
///    Returns **65-byte uncompressed** (0x04 || X || Y)
#[uniffi::export]
fn secp_generate_public_key(private_key32: Vec<u8>) -> Result<Vec<u8>, FfiCryptoError> {
    if private_key32.len() != 32 {
        return Err(FfiCryptoError::InvalidLength);
    }
    let private_key_array: [u8; 32] = private_key32
        .try_into()
        .map_err(|_| FfiCryptoError::InvalidLength)?;
    let public_key = ethereum::public_key_uncompressed(&private_key_array)?;
    Ok(public_key.to_vec())
}

/// 2) Recoverable ECDSA (Ethereum-style).
///    Returns **65 bytes r||s||v**, with **v in {0,1}** (parity bit).
///    - if `hashing == true`: keccak256(message) then sign_hash
///    - else: `msg` must be a 32-byte prehash
#[uniffi::export]
fn secp_sign_recoverable(
    msg: Vec<u8>,
    private_key32: Vec<u8>,
    hashing: bool,
) -> Result<Vec<u8>, FfiCryptoError> {
    if private_key32.len() != 32 {
        return Err(FfiCryptoError::InvalidLength);
    }
    let private_key_array: [u8; 32] = private_key32
        .try_into()
        .map_err(|_| FfiCryptoError::InvalidLength)?;
    let signature = ethereum::sign_recoverable(&msg, &private_key_array, hashing)?;
    Ok(signature.to_vec())
}

/// 3) Ethereum address from public key (accepts 65-byte 0x04||XY or 64-byte XY).
#[uniffi::export]
fn ethereum_address_from_pubkey(pubkey: Vec<u8>) -> String {
    ethereum::address_from_pubkey(&pubkey).unwrap_or_else(|_| "0x".to_string())
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
    fn test_secp_generate_public_key_and_address() {
        // Pre-calculated test constants
        let private_key = "90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb";
        let expected_ethereum_address = "0x34dd95109B587ca90778Cde5e2Dd87E022453699"; // Replace with actual calculated address

        // Convert private key from hex string to bytes
        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");

        // Generate public key from private key
        let public_key = secp_generate_public_key(private_key_bytes)
            .expect("Should generate public key successfully");

        // Verify public key is 65 bytes and starts with 0x04
        assert_eq!(public_key.len(), 65);
        assert_eq!(public_key[0], 0x04);

        // Generate Ethereum address from public key
        let generated_address = ethereum_address_from_pubkey(public_key);

        // Verify the generated address matches our expected address
        assert_eq!(
            generated_address.to_lowercase(),
            expected_ethereum_address.to_lowercase()
        );

        // Also test with 64-byte public key (without 0x04 prefix)
        let public_key_64 = secp_generate_public_key(hex::decode(private_key).unwrap())
            .expect("Should generate public key")[1..]
            .to_vec(); // Remove 0x04 prefix

        let address_from_64_bytes = ethereum_address_from_pubkey(public_key_64);
        assert_eq!(
            address_from_64_bytes.to_lowercase(),
            expected_ethereum_address.to_lowercase()
        );
    }

    #[test]
    fn test_secp_sign_recoverable_with_known_values() {
        // Pre-calculated test constants
        let private_key = "90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb";
        let message = "test message";

        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");
        let message_bytes = message.as_bytes().to_vec();

        // Test with hashing enabled
        let signature =
            secp_sign_recoverable(message_bytes.clone(), private_key_bytes.clone(), true)
                .expect("Should sign successfully with hashing");

        // Verify signature is 65 bytes
        assert_eq!(signature.len(), 65);

        // Verify recovery ID is 0 or 1
        let recovery_id = signature[64];
        assert!(recovery_id == 0 || recovery_id == 1);

        // Test with hashing disabled (message must be 32 bytes)
        let hash = ethereum_hash_personal(message.to_string()).expect("Should hash message");

        let signature_no_hash = secp_sign_recoverable(hash, private_key_bytes, false)
            .expect("Should sign pre-hashed message");

        assert_eq!(signature_no_hash.len(), 65);
        let recovery_id_no_hash = signature_no_hash[64];
        assert!(recovery_id_no_hash == 0 || recovery_id_no_hash == 1);
    }
}
