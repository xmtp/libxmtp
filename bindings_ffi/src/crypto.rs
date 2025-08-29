use alloy::primitives::{keccak256, eip191_hash_message, Address, B256};
use alloy::signers::{SignerSync};
use alloy::signers::local::PrivateKeySigner;
use thiserror::Error;

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

/// 1) Public key from 32-byte private key.
/// Returns **65-byte uncompressed** (0x04 || X || Y) to match your Swift.
#[uniffi::export]
fn secp_generate_public_key(private_key32: Vec<u8>) -> Result<Vec<u8>, FfiCryptoError> {
    if private_key32.len() != 32 {
        return Err(FfiCryptoError::InvalidLength);
    }
    let signer = PrivateKeySigner::from_slice(&private_key32)
        .map_err(|_| FfiCryptoError::InvalidKey)?;
    let xy: [u8; 64] = signer.public_key().into(); // B512 -> [u8; 64] (X||Y) :contentReference[oaicite:3]{index=3}
    let mut out = Vec::with_capacity(65);
    out.push(0x04);
    out.extend_from_slice(&xy);
    Ok(out)
}

/// 2) Recoverable ECDSA (Ethereum-style).
/// Returns **65 bytes r||s||v**, with **v in {0,1}** (parity bit).
/// - if `hashing == true`: keccak256(message) then sign_hash
/// - else: `msg` must be a 32-byte prehash
#[uniffi::export]
fn secp_sign_recoverable(msg: Vec<u8>, private_key32: Vec<u8>, hashing: bool) -> Result<Vec<u8>, FfiCryptoError> {
    if private_key32.len() != 32 { return Err(FfiCryptoError::InvalidLength); }
    let signer = PrivateKeySigner::from_slice(&private_key32)
        .map_err(|_| FfiCryptoError::InvalidKey)?;

    let digest: B256 = if hashing {
        keccak256(&msg)                     // Keccak-256 (Ethereum) :contentReference[oaicite:4]{index=4}
    } else {
        if msg.len() != 32 { return Err(FfiCryptoError::InvalidLength); }
        B256::from_slice(&msg)
    };

    let sig = signer.sign_hash_sync(&digest).map_err(|_| FfiCryptoError::SignFailure)?; // :contentReference[oaicite:5]{index=5}

    // Compose 65 bytes manually to ensure v={0,1}
    let r = sig.r().to_be_bytes::<32>();
    let s = sig.s().to_be_bytes::<32>();
    let v_byte = if sig.v() { 1u8 } else { 0u8 }; // parity bit as 0/1 :contentReference[oaicite:6]{index=6}

    let mut out = Vec::with_capacity(65);
    out.extend_from_slice(&r);
    out.extend_from_slice(&s);
    out.push(v_byte);
    Ok(out)
}

/// 3) Ethereum address from public key (accepts 65-byte 0x04||XY or 64-byte XY).
#[uniffi::export]
fn ethereum_address_from_pubkey(pubkey: Vec<u8>) -> String {
    let xy = match pubkey.len() {
        65 if pubkey[0] == 0x04 => &pubkey[1..],
        64 => &pubkey[..],
        _ => return "0x".to_string(),
    };
    let addr = Address::from_raw_public_key(xy);   // derives keccak(XY)[12..] :contentReference[oaicite:7]{index=7}
    format!("{addr:?}") // lowercased 0x… (Debug prints raw lower-hex without checksum) :contentReference[oaicite:8]{index=8}
}

/// 4) EIP-191 personal message hash: keccak256("\x19Ethereum Signed Message:\n{len}" || message)
#[uniffi::export]
fn ethereum_hash_personal(message: String) -> Result<Vec<u8>, FfiCryptoError> {
    Ok(eip191_hash_message(message).to_vec()) // 32 bytes :contentReference[oaicite:9]{index=9}
}
