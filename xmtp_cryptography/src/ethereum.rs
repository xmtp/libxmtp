use alloy::primitives::{eip191_hash_message, Address, B256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use thiserror::Error;

// Constants for secp256k1 and Ethereum cryptography
const UNCOMPRESSED_PUBKEY_PREFIX: u8 = 0x04;
const PUBKEY_UNCOMPRESSED_LEN: usize = 65;
const PUBKEY_XY_LEN: usize = 64;
const PRIVATE_KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 65;
const HASH_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum EthereumCryptoError {
    #[error("invalid length")]
    InvalidLength,
    #[error("invalid key")]
    InvalidKey,
    #[error("signing failure")]
    SignFailure,
    #[error("pubkey decompress failure")]
    DecompressFailure,
}

/// Generate uncompressed public key (65 bytes: 0x04 || X || Y) from 32-byte private key
/// FFI-friendly wrapper around alloy's PrivateKeySigner
pub fn public_key_uncompressed(
    private_key32: &[u8],
) -> Result<[u8; PUBKEY_UNCOMPRESSED_LEN], EthereumCryptoError> {
    // Validate private key length
    if private_key32.len() != PRIVATE_KEY_LEN {
        return Err(EthereumCryptoError::InvalidLength);
    }
    let private_key_array: &[u8; PRIVATE_KEY_LEN] = private_key32.try_into().unwrap(); // Safe after length check

    // Create alloy signer (handles validation internally)
    let signer = PrivateKeySigner::from_slice(private_key_array)
        .map_err(|_| EthereumCryptoError::InvalidKey)?;

    // Get public key and convert to uncompressed format
    let xy: [u8; PUBKEY_XY_LEN] = signer.public_key().into(); // B512 -> [u8; 64] (X||Y)
    let mut out = [0u8; PUBKEY_UNCOMPRESSED_LEN];
    out[0] = UNCOMPRESSED_PUBKEY_PREFIX;
    out[1..].copy_from_slice(&xy);
    Ok(out)
}

/// Recoverable ECDSA signing (Ethereum-style) - FFI-friendly wrapper around alloy
/// Returns 65 bytes: r||s||v where v ∈ {27,28} (Ethereum standard recovery ID)
/// - if `hashing == true`: EIP-191 personal message signing
/// - else: `msg` must be a 32-byte prehash
pub fn sign_recoverable(
    msg: &[u8],
    private_key32: &[u8],
    hashing: bool,
) -> Result<[u8; SIGNATURE_LEN], EthereumCryptoError> {
    // Validate private key length
    if private_key32.len() != PRIVATE_KEY_LEN {
        return Err(EthereumCryptoError::InvalidLength);
    }
    let private_key_array: &[u8; PRIVATE_KEY_LEN] = private_key32.try_into().unwrap(); // Safe after length check

    // Create alloy signer (handles zero key validation internally)
    let signer = PrivateKeySigner::from_slice(private_key_array)
        .map_err(|_| EthereumCryptoError::InvalidKey)?;

    // Use alloy's built-in signing methods
    let signature = if hashing {
        // Use alloy's EIP-191 personal message signing
        signer
            .sign_message_sync(msg)
            .map_err(|_| EthereumCryptoError::SignFailure)?
    } else {
        // Sign pre-computed hash
        if msg.len() != HASH_LEN {
            return Err(EthereumCryptoError::InvalidLength);
        }
        let hash = B256::from_slice(msg);
        signer
            .sign_hash_sync(&hash)
            .map_err(|_| EthereumCryptoError::SignFailure)?
    };

    // Convert alloy signature to FFI-friendly byte array
    Ok(signature.as_bytes()) // alloy signatures are always 65 bytes
}

/// Derive Ethereum address from public key (accepts 65-byte 0x04||XY or 64-byte XY)
pub fn address_from_pubkey(pubkey: &[u8]) -> Result<String, EthereumCryptoError> {
    let xy = match pubkey.len() {
        PUBKEY_UNCOMPRESSED_LEN if pubkey[0] == UNCOMPRESSED_PUBKEY_PREFIX => &pubkey[1..],
        PUBKEY_XY_LEN => pubkey,
        _ => return Err(EthereumCryptoError::InvalidLength),
    };

    let addr = Address::from_raw_public_key(xy); // derives keccak(XY)[12..]
    Ok(format!("{addr:?}")) // lowercased 0x… (Debug prints raw lower-hex without checksum)
}

/// EIP-191 personal message hash: keccak256("\x19Ethereum Signed Message:\n{len}" || message)
pub fn hash_personal(message: &str) -> [u8; HASH_LEN] {
    eip191_hash_message(message).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_key_generation_and_address() {
        // Pre-calculated test constants
        let private_key = "90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb";
        let expected_ethereum_address = "0x34dd95109b587ca90778cde5e2dd87e022453699";

        // Convert private key from hex string to bytes
        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");

        // Test uncompressed public key generation
        let public_key_65 = public_key_uncompressed(&private_key_bytes)
            .expect("Should generate public key successfully");

        // Verify public key is 65 bytes and starts with 0x04
        assert_eq!(public_key_65.len(), PUBKEY_UNCOMPRESSED_LEN);
        assert_eq!(public_key_65[0], UNCOMPRESSED_PUBKEY_PREFIX);

        // Generate Ethereum address from 65-byte public key
        let address_from_65 =
            address_from_pubkey(&public_key_65).expect("Should derive address from 65-byte key");
        assert_eq!(
            address_from_65.to_lowercase(),
            expected_ethereum_address.to_lowercase()
        );

        // Test that we can also derive address from 64-byte public key (XY coordinates only)
        let public_key_64 = &public_key_65[1..]; // Remove 0x04 prefix to get XY coordinates
        let address_from_64 =
            address_from_pubkey(public_key_64).expect("Should derive address from 64-byte key");
        assert_eq!(
            address_from_64.to_lowercase(),
            expected_ethereum_address.to_lowercase()
        );
    }

    #[test]
    fn test_sign_recoverable_with_known_values() {
        // Pre-calculated test constants
        let private_key = "90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb";
        let message = "test message";

        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");
        let message_bytes = message.as_bytes();

        // Test with hashing enabled
        let signature = sign_recoverable(message_bytes, &private_key_bytes, true)
            .expect("Should sign successfully with hashing");

        // Verify signature is 65 bytes
        assert_eq!(signature.len(), SIGNATURE_LEN);

        // Verify recovery ID is 27 or 28 (Ethereum standard)
        let recovery_id = signature[PUBKEY_XY_LEN];
        assert!(recovery_id == 27 || recovery_id == 28);

        // Test with hashing disabled (message must be 32 bytes) - using pre-computed EIP-191 hash
        let hash = hash_personal(message);

        let signature_no_hash = sign_recoverable(&hash, &private_key_bytes, false)
            .expect("Should sign pre-hashed message");

        assert_eq!(signature_no_hash.len(), SIGNATURE_LEN);
        let recovery_id_no_hash = signature_no_hash[PUBKEY_XY_LEN];
        assert!(recovery_id_no_hash == 27 || recovery_id_no_hash == 28);
    }

    #[test]
    fn test_hash_personal() {
        let message = "test message";
        let hash = hash_personal(message);

        // Should always return 32 bytes
        assert_eq!(hash.len(), HASH_LEN);

        // Should be deterministic
        let hash2 = hash_personal(message);
        assert_eq!(hash, hash2);

        // Different messages should produce different hashes
        let different_hash = hash_personal("different message");
        assert_ne!(hash, different_hash);
    }

    #[test]
    fn test_invalid_inputs() {
        // Test invalid private keys
        let zero_key = [0u8; PRIVATE_KEY_LEN]; // All zeros - mathematically invalid
        assert!(public_key_uncompressed(&zero_key).is_err());
        assert!(sign_recoverable(b"test", &zero_key, true).is_err());

        // Test maximum value key (also invalid for secp256k1)
        let max_key = [0xFFu8; PRIVATE_KEY_LEN];
        assert!(public_key_uncompressed(&max_key).is_err());
        assert!(sign_recoverable(b"test", &max_key, true).is_err());

        // Test invalid pubkey lengths for address derivation
        assert!(address_from_pubkey(&[0u8; PUBKEY_XY_LEN - 1]).is_err()); // Too short
        assert!(address_from_pubkey(&[0u8; PUBKEY_UNCOMPRESSED_LEN + 1]).is_err()); // Too long

        // Test invalid 65-byte key (wrong prefix)
        let mut invalid_65 = [0u8; PUBKEY_UNCOMPRESSED_LEN];
        invalid_65[0] = 0x03; // Wrong prefix
        assert!(address_from_pubkey(&invalid_65).is_err());

        // Test sign_recoverable with wrong hash length when hashing=false
        let valid_key = [1u8; PRIVATE_KEY_LEN];
        assert!(sign_recoverable(&[0u8; HASH_LEN - 1], &valid_key, false).is_err());
        // Wrong hash length
    }

    #[test]
    fn test_eip191_hashing_compatibility() {
        use alloy::signers::{local::PrivateKeySigner, SignerSync};

        let private_key = "90b7388a7427358cb7fc7e9042805b1942eae47ee783e627a989719da35e76fb";
        let message = "Hello, Ethereum!";
        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");

        // Create signatures using both our function and alloy
        let our_signature = sign_recoverable(
            message.as_bytes(),
            &private_key_bytes,
            true, // Use EIP-191 hashing
        )
        .expect("Should sign successfully");

        let alloy_signer =
            PrivateKeySigner::from_slice(&private_key_bytes).expect("Valid private key");
        let alloy_signature = alloy_signer
            .sign_message_sync(message.as_bytes())
            .expect("Should sign");

        // Both signatures should recover to the same address when verified with the same message
        use alloy::primitives::Signature as AlloySignature;

        let our_parsed = AlloySignature::try_from(our_signature.as_slice()).expect("Should parse");
        let our_recovered = our_parsed
            .recover_address_from_msg(message)
            .expect("Should recover");

        let alloy_recovered = alloy_signature
            .recover_address_from_msg(message)
            .expect("Should recover");

        // Both should recover to the same address (the correct one for this private key)
        assert_eq!(
            our_recovered.to_string().to_lowercase(),
            alloy_recovered.to_string().to_lowercase(),
            "Both signatures should recover to the same address when using EIP-191 hashing"
        );
    }

    #[test]
    fn test_signature_round_trip_compatibility() {
        // This test ensures our signatures work with the same verification patterns
        // used elsewhere in the XMTP codebase
        use alloy::primitives::Signature as AlloySignature;

        let private_key = "a1b2c3d4e5f67890123456789012345678901234567890123456789012345678";
        let message = "XMTP signature test message";

        let private_key_bytes = hex::decode(private_key).expect("Valid hex private key");

        // Generate public key and address using our functions
        let public_key =
            public_key_uncompressed(&private_key_bytes).expect("Should generate public key");
        let _expected_address = address_from_pubkey(&public_key).expect("Should generate address");

        // Sign the message
        let signature = sign_recoverable(
            message.as_bytes(),
            &private_key_bytes,
            true, // Use EIP-191 hashing
        )
        .expect("Should sign message");

        // Test that alloy can parse our signature format
        let alloy_signature =
            AlloySignature::try_from(signature.as_slice()).expect("Should parse signature");
        let recovered_address = alloy_signature
            .recover_address_from_msg(message)
            .expect("Should recover address");

        // The recovered address should be a valid Ethereum address
        let recovered_str = recovered_address.to_string();
        assert!(
            recovered_str.starts_with("0x"),
            "Should be a valid Ethereum address"
        );
        assert_eq!(recovered_str.len(), 42, "Should be 42 characters long");

        // Test with wrong message should recover a different address
        let wrong_signature =
            AlloySignature::try_from(signature.as_slice()).expect("Should parse signature");
        let wrong_recovered = wrong_signature
            .recover_address_from_msg("Different message")
            .expect("Should recover some address");

        assert_ne!(
            wrong_recovered.to_string().to_lowercase(),
            recovered_address.to_string().to_lowercase(),
            "Wrong message should recover a different address"
        );
    }
}
