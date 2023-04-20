use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdsa::{signature::Verifier, RecoveryId, Signature, VerifyingKey},
    PublicKey, SecretKey,
};
use sha2::{digest::Update, Digest, Sha256};
use sha3::Keccak256;

/// diffie_hellman - compute the shared secret between a secret key and a public key
/// NOTE: This is a custom implementation of the diffie_hellman operation
/// because RustCrypto hides the `y` coordinate from visibility when constructing a SharedSecret.
/// https://github.com/RustCrypto/traits/blob/d57b54b9fcf5b28745547cb9fef313ab09780918/elliptic-curve/src/ecdh.rs#L60
/// XMTP uses the entire point in uncompressed format as secret material
fn diffie_hellman(secret_key: &SecretKey, public_key: &PublicKey) -> Result<Vec<u8>, String> {
    // Get the public projective point from the public key
    let public_point = public_key.to_projective();
    // Multiply with nonzero scalar of secret key
    let shared_secret_point = (public_point * secret_key.to_nonzero_scalar().as_ref()).to_affine();
    // Encode the entire point in uncompressed format
    let shared_secret_encoded = shared_secret_point.to_encoded_point(false);
    return Ok(shared_secret_encoded.as_bytes().to_vec());
}

/// diffie_hellman_byte_params - compute the shared secret between a secret key and a public key
/// but take the secret key and public key as byte arrays in XMTP proto serialized format
/// i.e. secret_key is a 32 byte array and public_key is a 65 byte array
pub fn diffie_hellman_byte_params(secret_key: &[u8], public_key: &[u8]) -> Result<Vec<u8>, String> {
    let secret_key = SecretKey::from_be_bytes(secret_key).map_err(|e| e.to_string())?;
    let public_key = PublicKey::from_sec1_bytes(public_key).map_err(|e| e.to_string())?;
    diffie_hellman(&secret_key, &public_key)
}

/// Verify given a compact signature, recovery_id, digest, and public key in uncompressed format
/// NOTE: the recovery_id situation is not necessary, but it is a good sanity check
pub fn verify_sha256(
    signed_by: &[u8],
    message: &[u8],
    signature: &[u8],
    recovery_id: u8,
) -> Result<bool, String> {
    let signing_key = VerifyingKey::from_sec1_bytes(signed_by).map_err(|e| e.to_string())?;
    let recovery_id = RecoveryId::try_from(recovery_id).map_err(|e| e.to_string())?;
    let signature = Signature::try_from(signature).map_err(|e| e.to_string())?;
    // Pre sha256 the message
    // let digested_message = Sha256::digest(message);
    let sha256 = Sha256::new().chain(message);
    let recovered_key = VerifyingKey::recover_from_digest(sha256, &signature, recovery_id)
        .map_err(|e| e.to_string())?;
    // Assert that the recovered key matches the signing key
    if signing_key != recovered_key {
        return Err("Recovered key does not match signing key".to_string());
    }
    signing_key
        .verify(message, &signature)
        .map_err(|e| e.to_string())
        .map(|_| true)
}

/// Return recovered key from a compact signature, recovery_id, and digest (does sha256 internally)
pub fn recover_public_key_predigest_sha256(
    message: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>, String> {
    // Recovery id is the last byte of the signature, split signature into 64:1
    let (signature, recovery_id) = signature.split_at(64);
    let recovery_id = recovery_id[0];
    let recovery_id = RecoveryId::try_from(recovery_id).map_err(|e| e.to_string())?;
    let signature = Signature::try_from(signature).map_err(|e| e.to_string())?;
    // Create a pre-digested FixedOutput digest
    let digest = Sha256::new().chain(message);
    let recovered_key = VerifyingKey::recover_from_digest(digest, &signature, recovery_id)
        .map_err(|e| e.to_string())?;
    Ok(recovered_key.to_encoded_point(false).as_bytes().to_vec())
}

/// Return recovered key from a compact signature, recovery_id, and message (does keccak256 internally)
pub fn recover_public_key_predigest_keccak256(
    message: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>, String> {
    // Recovery id is the last byte of the signature, split signature into 64:1
    let (signature, recovery_id) = signature.split_at(64);
    let recovery_id = recovery_id[0];
    let recovery_id = RecoveryId::try_from(recovery_id).map_err(|e| e.to_string())?;
    let signature = Signature::try_from(signature).map_err(|e| e.to_string())?;
    // Create a pre-digested FixedOutput digest
    let digest = Keccak256::new().chain(message);
    let recovered_key = VerifyingKey::recover_from_digest(digest, &signature, recovery_id)
        .map_err(|e| e.to_string())?;
    Ok(recovered_key.to_encoded_point(false).as_bytes().to_vec())
}

/// Get public key from secret key in uncompressed format
pub fn get_public_key(secret_key: &[u8]) -> Result<Vec<u8>, String> {
    let secret_key = SecretKey::from_be_bytes(secret_key).map_err(|e| e.to_string())?;
    Ok(secret_key
        .public_key()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec())
}
