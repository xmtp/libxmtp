use k256::ecdsa::signature::DigestVerifier;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdsa::{signature::Verifier, RecoveryId, Signature, VerifyingKey},
    PublicKey,
};
use sha3::{Digest, Keccak256};

use super::super::proto;
use protobuf::{Message, MessageField};

pub fn recover_wallet_public_key(
    message: &[u8],
    signature: &proto::signature::Signature,
) -> Result<PublicKey, String> {
    // Expect ecdsa_compact field with subfields: bytes, recovery_id
    if !signature.has_wallet_ecdsa_compact() {
        return Err("No wallet_ecdsa_compact field found".to_string());
    }
    let wallet_ecdsa_compact = signature.wallet_ecdsa_compact();
    let signature_bytes = wallet_ecdsa_compact.bytes.as_slice();
    let recovery_id_result = RecoveryId::try_from(wallet_ecdsa_compact.recovery as u8);
    if recovery_id_result.is_err() {
        return Err(recovery_id_result.err().unwrap().to_string());
    }
    let recovery_id = recovery_id_result.unwrap();
    let ecdsa_signature_result = Signature::try_from(signature_bytes);
    if ecdsa_signature_result.is_err() {
        return Err(ecdsa_signature_result.err().unwrap().to_string());
    }
    let ec_signature = ecdsa_signature_result.unwrap();

    let recovered_key_result = VerifyingKey::recover_from_digest(
        Keccak256::new_with_prefix(message),
        &ec_signature,
        recovery_id,
    );

    if recovered_key_result.is_err() {
        return Err(recovered_key_result.err().unwrap().to_string());
    }
    let recovered_key = recovered_key_result.unwrap();

    // First extract the public key from the recovered key
    let public_key = PublicKey::from(&recovered_key);

    // Finally use the recovered key in a re-verification, may not strictly be required
    if VerifyingKey::from(&public_key)
        .verify_digest(Keccak256::new_with_prefix(&message), &ec_signature)
        .is_err()
    {
        return Err("Signature verification failed".to_string());
    }
    return Ok(public_key);
}

// Need to do two layers of proto deserialization, key_bytes is just the bytes of the PublicKey proto
pub fn signed_public_key_from_proto(
    proto: &proto::public_key::SignedPublicKey,
) -> Result<PublicKey, String> {
    let mut public_key_proto_bytes = proto.key_bytes.as_slice();
    let public_key_proto_result: Result<proto::public_key::PublicKey, protobuf::Error> =
        protobuf::Message::parse_from_bytes(&mut public_key_proto_bytes);
    if public_key_proto_result.is_err() {
        return Err(public_key_proto_result.err().unwrap().to_string());
    }
    let public_key_result = PublicKey::from_sec1_bytes(
        public_key_proto_result
            .unwrap()
            .secp256k1_uncompressed()
            .bytes
            .as_slice(),
    );

    if public_key_result.is_err() {
        return Err(format!(
            "Error parsing sec1 bytes: {}",
            public_key_result.err().unwrap().to_string()
        ));
    }
    return Ok(public_key_result.unwrap());
}

pub fn public_key_from_proto(proto: &proto::public_key::PublicKey) -> Result<PublicKey, String> {
    let public_key_bytes = proto.secp256k1_uncompressed().bytes.as_slice();
    let public_key_result = PublicKey::from_sec1_bytes(public_key_bytes);
    if public_key_result.is_err() {
        return Err(public_key_result.err().unwrap().to_string());
    }
    return Ok(public_key_result.unwrap());
}

pub fn to_unsigned_public_key_proto(
    public_key: &PublicKey,
    created_at: u64,
) -> proto::public_key::UnsignedPublicKey {
    // First, create the UnsignedPublicKey and set the secp256k1_uncompressed field
    let mut unsigned_public_key = proto::public_key::UnsignedPublicKey::new();

    // Get the uncompressed bytes of the public key
    let binding = public_key.to_encoded_point(false);
    let public_key_bytes = binding.as_bytes();
    let mut secp256k1_uncompressed =
        proto::public_key::unsigned_public_key::Secp256k1Uncompressed::new();
    secp256k1_uncompressed.bytes = public_key_bytes.to_vec();
    unsigned_public_key.set_secp256k1_uncompressed(secp256k1_uncompressed);
    unsigned_public_key.created_ns = created_at;
    return unsigned_public_key;
}

pub fn to_signed_public_key_proto(
    public_key: &PublicKey,
    created_at: u64,
) -> proto::public_key::SignedPublicKey {
    // First, get the UnsignedPublicKey proto
    let unsigned_public_key = to_unsigned_public_key_proto(public_key, created_at);

    let mut signed_public_key = proto::public_key::SignedPublicKey::new();
    signed_public_key.key_bytes = unsigned_public_key.write_to_bytes().unwrap();
    // TODO: STOPSHIP: Need to set the Signature
    return signed_public_key;
}
