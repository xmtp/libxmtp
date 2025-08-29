use alloy::primitives::{eip191_hash_message, keccak256, Address, B256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use thiserror::Error;

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
pub fn public_key_uncompressed(private_key32: &[u8; 32]) -> Result<[u8; 65], EthereumCryptoError> {
    let signer =
        PrivateKeySigner::from_slice(private_key32).map_err(|_| EthereumCryptoError::InvalidKey)?;
    let xy: [u8; 64] = signer.public_key().into(); // B512 -> [u8; 64] (X||Y)

    let mut out = [0u8; 65];
    out[0] = 0x04;
    out[1..].copy_from_slice(&xy);
    Ok(out)
}

/// Generate raw XY coordinates (64 bytes) from 32-byte private key
pub fn public_key_xy(private_key32: &[u8; 32]) -> Result<[u8; 64], EthereumCryptoError> {
    let signer =
        PrivateKeySigner::from_slice(private_key32).map_err(|_| EthereumCryptoError::InvalidKey)?;
    Ok(signer.public_key().into())
}

/// Recoverable ECDSA signing (Ethereum-style)
/// Returns 65 bytes: r||s||v where v ∈ {0,1} (parity bit)
/// - if `hashing == true`: keccak256(message) then sign_hash
/// - else: `msg` must be a 32-byte prehash
pub fn sign_recoverable(
    msg: &[u8],
    private_key32: &[u8; 32],
    hashing: bool,
) -> Result<[u8; 65], EthereumCryptoError> {
    let signer =
        PrivateKeySigner::from_slice(private_key32).map_err(|_| EthereumCryptoError::InvalidKey)?;

    let digest: B256 = if hashing {
        keccak256(msg) // Keccak-256 (Ethereum)
    } else {
        if msg.len() != 32 {
            return Err(EthereumCryptoError::InvalidLength);
        }
        B256::from_slice(msg)
    };

    let sig = signer
        .sign_hash_sync(&digest)
        .map_err(|_| EthereumCryptoError::SignFailure)?;

    // Compose 65 bytes manually to ensure v={0,1}
    let r = sig.r().to_be_bytes::<32>();
    let s = sig.s().to_be_bytes::<32>();
    let v_byte = if sig.v() { 1u8 } else { 0u8 }; // parity bit as 0/1

    let mut out = [0u8; 65];
    out[0..32].copy_from_slice(&r);
    out[32..64].copy_from_slice(&s);
    out[64] = v_byte;
    Ok(out)
}

/// Derive Ethereum address from public key (accepts 65-byte 0x04||XY or 64-byte XY)
pub fn address_from_pubkey(pubkey: &[u8]) -> Result<String, EthereumCryptoError> {
    let xy = match pubkey.len() {
        65 if pubkey[0] == 0x04 => &pubkey[1..],
        64 => pubkey,
        _ => return Err(EthereumCryptoError::InvalidLength),
    };

    let addr = Address::from_raw_public_key(xy); // derives keccak(XY)[12..]
    Ok(format!("{addr:?}")) // lowercased 0x… (Debug prints raw lower-hex without checksum)
}

/// EIP-191 personal message hash: keccak256("\x19Ethereum Signed Message:\n{len}" || message)
pub fn hash_personal(message: &str) -> [u8; 32] {
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
        let private_key_bytes: [u8; 32] = hex::decode(private_key)
            .expect("Valid hex private key")
            .try_into()
            .expect("32 bytes");

        // Test uncompressed public key generation
        let public_key_65 = public_key_uncompressed(&private_key_bytes)
            .expect("Should generate public key successfully");

        // Verify public key is 65 bytes and starts with 0x04
        assert_eq!(public_key_65.len(), 65);
        assert_eq!(public_key_65[0], 0x04);

        // Test XY coordinates generation
        let public_key_64 =
            public_key_xy(&private_key_bytes).expect("Should generate XY coordinates");
        assert_eq!(public_key_64.len(), 64);

        // Verify XY matches the uncompressed key (minus prefix)
        assert_eq!(&public_key_65[1..], &public_key_64[..]);

        // Generate Ethereum address from 65-byte public key
        let address_from_65 =
            address_from_pubkey(&public_key_65).expect("Should derive address from 65-byte key");
        assert_eq!(
            address_from_65.to_lowercase(),
            expected_ethereum_address.to_lowercase()
        );

        // Generate Ethereum address from 64-byte public key
        let address_from_64 =
            address_from_pubkey(&public_key_64).expect("Should derive address from 64-byte key");
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

        let private_key_bytes: [u8; 32] = hex::decode(private_key)
            .expect("Valid hex private key")
            .try_into()
            .expect("32 bytes");
        let message_bytes = message.as_bytes();

        // Test with hashing enabled
        let signature = sign_recoverable(message_bytes, &private_key_bytes, true)
            .expect("Should sign successfully with hashing");

        // Verify signature is 65 bytes
        assert_eq!(signature.len(), 65);

        // Verify recovery ID is 0 or 1
        let recovery_id = signature[64];
        assert!(recovery_id == 0 || recovery_id == 1);

        // Test with hashing disabled (message must be 32 bytes)
        let hash = hash_personal(message);

        let signature_no_hash = sign_recoverable(&hash, &private_key_bytes, false)
            .expect("Should sign pre-hashed message");

        assert_eq!(signature_no_hash.len(), 65);
        let recovery_id_no_hash = signature_no_hash[64];
        assert!(recovery_id_no_hash == 0 || recovery_id_no_hash == 1);
    }

    #[test]
    fn test_hash_personal() {
        let message = "test message";
        let hash = hash_personal(message);

        // Should always return 32 bytes
        assert_eq!(hash.len(), 32);

        // Should be deterministic
        let hash2 = hash_personal(message);
        assert_eq!(hash, hash2);

        // Different messages should produce different hashes
        let different_hash = hash_personal("different message");
        assert_ne!(hash, different_hash);
    }

    #[test]
    fn test_invalid_inputs() {
        // Should fail with invalid key length (simulated by all zeros which is invalid)
        let zero_key = [0u8; 32];
        assert!(public_key_uncompressed(&zero_key).is_err());
        assert!(public_key_xy(&zero_key).is_err());
        assert!(sign_recoverable(b"test", &zero_key, true).is_err());

        // Test invalid pubkey lengths for address derivation
        assert!(address_from_pubkey(&[0u8; 63]).is_err()); // Too short
        assert!(address_from_pubkey(&[0u8; 66]).is_err()); // Too long

        // Test invalid 65-byte key (wrong prefix)
        let mut invalid_65 = [0u8; 65];
        invalid_65[0] = 0x03; // Wrong prefix
        assert!(address_from_pubkey(&invalid_65).is_err());

        // Test sign_recoverable with wrong hash length when hashing=false
        let valid_key = [1u8; 32];
        assert!(sign_recoverable(&[0u8; 31], &valid_key, false).is_err()); // Wrong hash length
    }
}
